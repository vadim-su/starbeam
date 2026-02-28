//! GPU render pipeline for 2D Radiance Cascades.
//!
//! Connects the CPU-side extraction (`RcInputData`, `RcLightingConfig`) to the
//! WGSL compute shaders (`radiance_cascades.wgsl`, `rc_finalize.wgsl`).
//!
//! Each frame:
//! 1. `ExtractResource` copies `RcInputData`, `RcLightingConfig`, and
//!    `RcGpuImages` into the render world.
//! 2. `prepare_rc_textures` uploads CPU buffers to GPU textures and resizes
//!    cascade/lightmap textures when dimensions change.
//! 3. `prepare_rc_bind_groups` creates per-cascade and finalize bind groups.
//! 4. `RcComputeNode` dispatches cascades (high → low) then finalize.

use std::borrow::Cow;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::extract_resource::ExtractResource;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_graph::{self, Node, NodeRunError, RenderGraphContext, RenderLabel};
use bevy::render::render_resource::binding_types::{
    texture_2d, texture_storage_2d, uniform_buffer,
};
use bevy::render::render_resource::{
    encase, BindGroup, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
    BufferInitDescriptor, BufferUsages, CachedComputePipelineId, ComputePassDescriptor,
    ComputePipelineDescriptor, Extent3d, Origin3d, PipelineCache, ShaderStages, ShaderType,
    StorageTextureAccess, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect,
    TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue};
use bevy::render::texture::GpuImage;
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};

use super::rc_lighting::{RcInputData, RcLightingConfig};

// ---------------------------------------------------------------------------
// GPU uniform structs — must match WGSL layout exactly (48 bytes each)
// ---------------------------------------------------------------------------

/// Uniforms for the cascade compute shader (`radiance_cascades.wgsl`).
#[derive(ShaderType, Clone, Copy)]
struct RcUniformsGpu {
    input_size: UVec2,
    cascade_index: u32,
    cascade_count: u32,
    viewport_offset: UVec2,
    viewport_size: UVec2,
    bounce_damping: f32,
    _pad0: f32,
    _pad1: UVec2,
}

/// Uniforms for the finalize compute shader (`rc_finalize.wgsl`).
#[derive(ShaderType, Clone, Copy)]
struct FinalizeUniformsGpu {
    input_size: UVec2,
    viewport_offset: UVec2,
    viewport_size: UVec2,
    _pad: UVec2,
}

// ---------------------------------------------------------------------------
// GPU image handles — created in main world, extracted to render world
// ---------------------------------------------------------------------------

/// Handles to all GPU textures used by the RC pipeline.
///
/// Created in the main world with small default sizes and extracted to the
/// render world each frame. The render-side prepare system uploads CPU data
/// and the main-world system resizes them when dimensions change.
#[derive(Resource, Clone, ExtractResource)]
pub struct RcGpuImages {
    pub density: Handle<Image>,
    pub density_bg: Handle<Image>,
    pub emissive: Handle<Image>,
    pub albedo: Handle<Image>,
    /// Double-buffer A for cascade storage.
    pub cascade_a: Handle<Image>,
    /// Double-buffer B for cascade storage.
    pub cascade_b: Handle<Image>,
    /// Current-frame lightmap output.
    pub lightmap: Handle<Image>,
    /// Previous-frame lightmap (for bounce light).
    pub lightmap_prev: Handle<Image>,
}

// ---------------------------------------------------------------------------
// Render-world resources
// ---------------------------------------------------------------------------

/// Cached compute pipeline IDs and bind group layout descriptors.
#[derive(Resource)]
struct RcPipeline {
    cascade_layout: BindGroupLayoutDescriptor,
    finalize_layout: BindGroupLayoutDescriptor,
    cascade_pipeline: CachedComputePipelineId,
    finalize_pipeline: CachedComputePipelineId,
}

/// Per-frame bind groups rebuilt in `prepare_rc_bind_groups`.
#[derive(Resource, Default)]
struct RcBindGroups {
    /// One bind group per cascade dispatch (highest → 0).
    cascade_bind_groups: Vec<BindGroup>,
    finalize_bind_group: Option<BindGroup>,
}

/// Tracks the last uploaded input dimensions for the render-world node.
#[derive(Resource, Default)]
struct RcTextureMeta {
    input_w: u32,
    input_h: u32,
    viewport_w: u32,
    viewport_h: u32,
    cascade_count: u32,
}

// ---------------------------------------------------------------------------
// Render graph label
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, RenderLabel)]
struct RcComputeLabel;

// ---------------------------------------------------------------------------
// Plugin wiring (called from RcLightingPlugin)
// ---------------------------------------------------------------------------

/// Sets up the render-side pipeline. Called from `RcLightingPlugin::build`.
pub(crate) fn setup_render_pipeline(app: &mut App) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };

    render_app
        .init_resource::<RcBindGroups>()
        .init_resource::<RcTextureMeta>()
        .add_systems(RenderStartup, init_rc_pipeline)
        .add_systems(
            Render,
            (
                prepare_rc_textures.in_set(RenderSystems::PrepareResources),
                prepare_rc_bind_groups.in_set(RenderSystems::PrepareBindGroups),
            ),
        );

    // Add compute node to the render graph.
    let mut render_graph = render_app
        .world_mut()
        .resource_mut::<render_graph::RenderGraph>();
    render_graph.add_node(RcComputeLabel, RcComputeNode);
    render_graph.add_node_edge(RcComputeLabel, bevy::render::graph::CameraDriverLabel);
}

// ---------------------------------------------------------------------------
// One-time pipeline initialisation (RenderStartup)
// ---------------------------------------------------------------------------

fn init_rc_pipeline(
    mut commands: Commands,
    pipeline_cache: Res<PipelineCache>,
    asset_server: Res<AssetServer>,
) {
    let cascade_shader = asset_server.load("shaders/radiance_cascades.wgsl");
    let finalize_shader = asset_server.load("shaders/rc_finalize.wgsl");

    // --- Cascade bind group layout (matches radiance_cascades.wgsl @group(0)) ---
    let cascade_layout = BindGroupLayoutDescriptor::new(
        "rc_cascade_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                uniform_buffer::<RcUniformsGpu>(false), // @binding(0)
                texture_2d(TextureSampleType::Float { filterable: false }), // @binding(1) density
                texture_2d(TextureSampleType::Float { filterable: false }), // @binding(2) emissive
                texture_2d(TextureSampleType::Float { filterable: false }), // @binding(3) albedo
                texture_2d(TextureSampleType::Float { filterable: false }), // @binding(4) lightmap_prev
                texture_2d(TextureSampleType::Float { filterable: false }), // @binding(5) cascade_read
                texture_storage_2d(TextureFormat::Rgba16Float, StorageTextureAccess::WriteOnly), // @binding(6)
            ),
        ),
    );

    // --- Finalize bind group layout (matches rc_finalize.wgsl @group(0)) ---
    let finalize_layout = BindGroupLayoutDescriptor::new(
        "rc_finalize_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                uniform_buffer::<FinalizeUniformsGpu>(false), // @binding(0)
                texture_2d(TextureSampleType::Float { filterable: false }), // @binding(1) cascade_0
                texture_storage_2d(TextureFormat::Rgba16Float, StorageTextureAccess::WriteOnly), // @binding(2)
            ),
        ),
    );

    // --- Queue compute pipelines ---
    let cascade_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("rc_cascade_pipeline".into()),
        layout: vec![cascade_layout.clone()],
        shader: cascade_shader,
        entry_point: Some(Cow::from("main")),
        ..default()
    });

    let finalize_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("rc_finalize_pipeline".into()),
        layout: vec![finalize_layout.clone()],
        shader: finalize_shader,
        entry_point: Some(Cow::from("main")),
        ..default()
    });

    commands.insert_resource(RcPipeline {
        cascade_layout,
        finalize_layout,
        cascade_pipeline,
        finalize_pipeline,
    });
}

// ---------------------------------------------------------------------------
// Prepare: upload CPU data to GPU textures
// ---------------------------------------------------------------------------

fn prepare_rc_textures(
    input: Option<Res<RcInputData>>,
    config: Option<Res<RcLightingConfig>>,
    gpu_images_res: Option<Res<RcGpuImages>>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    render_queue: Res<RenderQueue>,
    mut meta: ResMut<RcTextureMeta>,
) {
    let (Some(input), Some(config), Some(handles)) = (input, config, gpu_images_res) else {
        return;
    };

    // NOTE: We upload every frame unconditionally. The `dirty` flag on
    // `RcInputData` cannot be reset from the render world (it's a clone via
    // `ExtractResource`), and the main-world system always sets it to `true`.
    // Skipping uploads when the camera is stationary would require proper
    // change detection — left as a future optimisation.

    let w = config.input_size.x;
    let h = config.input_size.y;

    if w == 0 || h == 0 {
        return;
    }

    meta.input_w = w;
    meta.input_h = h;
    meta.viewport_w = config.viewport_size.x;
    meta.viewport_h = config.viewport_size.y;
    meta.cascade_count = config.cascade_count;

    let extent = Extent3d {
        width: w,
        height: h,
        depth_or_array_layers: 1,
    };

    // Upload density (R8Unorm — 1 byte per texel)
    if let Some(gpu_img) = gpu_images.get(&handles.density) {
        let row_bytes = w; // 1 byte per texel
        let (padded, aligned_bpr) = pad_rows(&input.density, row_bytes, h);
        render_queue.write_texture(
            TexelCopyTextureInfo {
                texture: &gpu_img.texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &padded,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(aligned_bpr),
                rows_per_image: Some(h),
            },
            extent,
        );
    }

    // Upload emissive (Rgba16Float — 8 bytes per texel)
    if let Some(gpu_img) = gpu_images.get(&handles.emissive) {
        let emissive_bytes = emissive_to_f16_bytes(&input.emissive);
        let row_bytes = w * 8;
        let (padded, aligned_bpr) = pad_rows(&emissive_bytes, row_bytes, h);
        render_queue.write_texture(
            TexelCopyTextureInfo {
                texture: &gpu_img.texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &padded,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(aligned_bpr),
                rows_per_image: Some(h),
            },
            extent,
        );
    }

    // Upload albedo (Rgba8Unorm — 4 bytes per texel)
    if let Some(gpu_img) = gpu_images.get(&handles.albedo) {
        let albedo_bytes: &[u8] = input.albedo.as_flattened();
        let row_bytes = w * 4;
        let (padded, aligned_bpr) = pad_rows(albedo_bytes, row_bytes, h);
        render_queue.write_texture(
            TexelCopyTextureInfo {
                texture: &gpu_img.texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &padded,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(aligned_bpr),
                rows_per_image: Some(h),
            },
            extent,
        );
    }
}

/// wgpu requires `bytes_per_row` to be a multiple of `COPY_BYTES_PER_ROW_ALIGNMENT` (256).
const COPY_ROW_ALIGN: u32 = 256;

/// Round up to the next multiple of [`COPY_ROW_ALIGN`].
fn align_bytes_per_row(unaligned: u32) -> u32 {
    unaligned.div_ceil(COPY_ROW_ALIGN) * COPY_ROW_ALIGN
}

/// Build a row-aligned copy of `src` for `write_texture`.
/// `row_bytes` is the unpadded byte width of one row.
/// Returns the padded buffer and the aligned bytes-per-row value.
fn pad_rows(src: &[u8], row_bytes: u32, h: u32) -> (Vec<u8>, u32) {
    let aligned = align_bytes_per_row(row_bytes);
    if aligned == row_bytes {
        return (src.to_vec(), aligned);
    }
    let mut buf = vec![0u8; (aligned * h) as usize];
    for y in 0..h as usize {
        let src_start = y * row_bytes as usize;
        let dst_start = y * aligned as usize;
        buf[dst_start..dst_start + row_bytes as usize]
            .copy_from_slice(&src[src_start..src_start + row_bytes as usize]);
    }
    (buf, aligned)
}

/// Convert `[f32; 4]` emissive data to half-float bytes for `Rgba16Float`.
fn emissive_to_f16_bytes(data: &[[f32; 4]]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(data.len() * 8);
    for pixel in data {
        for &channel in pixel {
            bytes.extend_from_slice(&f32_to_f16_bits(channel).to_le_bytes());
        }
    }
    bytes
}

/// Minimal f32 → f16 conversion (IEEE 754 half-precision).
fn f32_to_f16_bits(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xFF) as i32;
    let mantissa = bits & 0x007F_FFFF;

    if exponent == 255 {
        // Inf / NaN
        return sign | 0x7C00 | if mantissa != 0 { 0x0200 } else { 0 };
    }

    let new_exp = exponent - 127 + 15;
    if new_exp >= 31 {
        return sign | 0x7C00; // overflow → Inf
    }
    if new_exp <= 0 {
        return sign; // underflow → zero (skip denormals for simplicity)
    }

    sign | ((new_exp as u16) << 10) | ((mantissa >> 13) as u16)
}

// ---------------------------------------------------------------------------
// Prepare: create bind groups
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn prepare_rc_bind_groups(
    mut bind_groups: ResMut<RcBindGroups>,
    pipeline: Option<Res<RcPipeline>>,
    config: Option<Res<RcLightingConfig>>,
    gpu_images_res: Option<Res<RcGpuImages>>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    meta: Res<RcTextureMeta>,
) {
    bind_groups.cascade_bind_groups.clear();
    bind_groups.finalize_bind_group = None;

    let (Some(pipeline), Some(config), Some(handles)) = (pipeline, config, gpu_images_res) else {
        return;
    };

    if meta.input_w == 0 || meta.input_h == 0 {
        return;
    }

    // Resolve all GPU image views
    let (
        Some(density),
        Some(emissive),
        Some(albedo),
        Some(cascade_a),
        Some(cascade_b),
        Some(lightmap),
        Some(lightmap_prev),
    ) = (
        gpu_images.get(&handles.density),
        gpu_images.get(&handles.emissive),
        gpu_images.get(&handles.albedo),
        gpu_images.get(&handles.cascade_a),
        gpu_images.get(&handles.cascade_b),
        gpu_images.get(&handles.lightmap),
        gpu_images.get(&handles.lightmap_prev),
    )
    else {
        return;
    };

    let cascade_layout = pipeline_cache.get_bind_group_layout(&pipeline.cascade_layout);
    let finalize_layout_gpu = pipeline_cache.get_bind_group_layout(&pipeline.finalize_layout);

    let cascade_count = config.cascade_count;

    // Build one bind group per cascade (highest → 0).
    // Even cascades write to A, read from B; odd cascades write to B, read from A.
    for i in (0..cascade_count).rev() {
        let writes_to_a = i % 2 == 0;
        let (write_tex, _read_tex) = if writes_to_a {
            (&cascade_a.texture_view, &cascade_b.texture_view)
        } else {
            (&cascade_b.texture_view, &cascade_a.texture_view)
        };

        // For the highest cascade, cascade_read is unused (no upper cascade).
        // We still bind it to satisfy the layout — the shader won't sample it.
        let cascade_read_view = if i == cascade_count - 1 {
            // Bind the "other" buffer as a dummy read texture
            if writes_to_a {
                &cascade_b.texture_view
            } else {
                &cascade_a.texture_view
            }
        } else {
            // The upper cascade was just written; read from it.
            let upper_wrote_to_a = (i + 1) % 2 == 0;
            if upper_wrote_to_a {
                &cascade_a.texture_view
            } else {
                &cascade_b.texture_view
            }
        };

        // Build uniform buffer
        let uniforms = RcUniformsGpu {
            input_size: config.input_size,
            cascade_index: i,
            cascade_count,
            viewport_offset: config.viewport_offset,
            viewport_size: config.viewport_size,
            bounce_damping: config.bounce_damping,
            _pad0: 0.0,
            _pad1: UVec2::ZERO,
        };

        let mut uniform_buf = encase::UniformBuffer::new(Vec::<u8>::new());
        if let Err(e) = uniform_buf.write(&uniforms) {
            warn!("RC cascade uniform write failed: {e}");
            return;
        }
        let uniform_bytes = uniform_buf.into_inner();

        let gpu_uniform = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("rc_cascade_uniform"),
            contents: &uniform_bytes,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let bg = render_device.create_bind_group(
            "rc_cascade_bind_group",
            &cascade_layout,
            &BindGroupEntries::sequential((
                gpu_uniform.as_entire_binding(),
                &density.texture_view,
                &emissive.texture_view,
                &albedo.texture_view,
                &lightmap_prev.texture_view,
                cascade_read_view,
                write_tex,
            )),
        );

        bind_groups.cascade_bind_groups.push(bg);
    }

    // --- Finalize bind group ---
    // Cascade 0 always writes to A (0 % 2 == 0).
    let cascade_0_view = &cascade_a.texture_view;

    let finalize_uniforms = FinalizeUniformsGpu {
        input_size: config.input_size,
        viewport_offset: config.viewport_offset,
        viewport_size: config.viewport_size,
        _pad: UVec2::ZERO,
    };

    let mut uniform_buf = encase::UniformBuffer::new(Vec::<u8>::new());
    if let Err(e) = uniform_buf.write(&finalize_uniforms) {
        warn!("RC finalize uniform write failed: {e}");
        return;
    }
    let uniform_bytes = uniform_buf.into_inner();

    let gpu_uniform = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("rc_finalize_uniform"),
        contents: &uniform_bytes,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    });

    let finalize_bg = render_device.create_bind_group(
        "rc_finalize_bind_group",
        &finalize_layout_gpu,
        &BindGroupEntries::sequential((
            gpu_uniform.as_entire_binding(),
            cascade_0_view,
            &lightmap.texture_view,
        )),
    );

    bind_groups.finalize_bind_group = Some(finalize_bg);
}

// ---------------------------------------------------------------------------
// Render graph node: dispatch compute shaders
// ---------------------------------------------------------------------------

struct RcComputeNode;

impl Node for RcComputeNode {
    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let pipeline_cache = world.resource::<PipelineCache>();
        let Some(pipeline) = world.get_resource::<RcPipeline>() else {
            return Ok(());
        };
        let Some(bind_groups) = world.get_resource::<RcBindGroups>() else {
            return Ok(());
        };
        let Some(config) = world.get_resource::<RcLightingConfig>() else {
            return Ok(());
        };
        let meta = world.resource::<RcTextureMeta>();

        if meta.input_w == 0 || meta.input_h == 0 {
            return Ok(());
        }

        // Check that pipelines are compiled
        let Some(cascade_pipeline) = pipeline_cache.get_compute_pipeline(pipeline.cascade_pipeline)
        else {
            return Ok(());
        };
        let Some(finalize_pipeline) =
            pipeline_cache.get_compute_pipeline(pipeline.finalize_pipeline)
        else {
            return Ok(());
        };

        if bind_groups.cascade_bind_groups.is_empty() || bind_groups.finalize_bind_group.is_none() {
            return Ok(());
        }

        let cascade_count = config.cascade_count;

        // Dispatch cascades from highest to 0.
        // Each cascade gets its own compute pass so the pass drop provides an
        // implicit barrier — cascade N must finish writing before cascade N-1
        // reads from the same texture.
        // bind_groups.cascade_bind_groups[0] = highest cascade,
        // bind_groups.cascade_bind_groups[last] = cascade 0.
        for (dispatch_idx, bg) in bind_groups.cascade_bind_groups.iter().enumerate() {
            let cascade_idx = cascade_count - 1 - dispatch_idx as u32;
            let spacing = 1u32 << cascade_idx;
            let probes_w = meta.input_w / spacing.max(1);
            let probes_h = meta.input_h / spacing.max(1);

            if probes_w == 0 || probes_h == 0 {
                continue;
            }

            let workgroups_x = probes_w.div_ceil(8);
            let workgroups_y = probes_h.div_ceil(8);

            let mut pass =
                render_context
                    .command_encoder()
                    .begin_compute_pass(&ComputePassDescriptor {
                        label: Some("rc_cascade_pass"),
                        timestamp_writes: None,
                    });
            pass.set_pipeline(cascade_pipeline);
            pass.set_bind_group(0, bg, &[]);
            pass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
            // pass drops here → implicit barrier before next cascade
        }

        // Finalize compute pass: extract irradiance from cascade 0 into lightmap.
        {
            let mut pass =
                render_context
                    .command_encoder()
                    .begin_compute_pass(&ComputePassDescriptor {
                        label: Some("rc_finalize_pass"),
                        timestamp_writes: None,
                    });

            pass.set_pipeline(finalize_pipeline);
            pass.set_bind_group(0, bind_groups.finalize_bind_group.as_ref().unwrap(), &[]);

            let workgroups_x = meta.viewport_w.div_ceil(8);
            let workgroups_y = meta.viewport_h.div_ceil(8);
            pass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Main-world helpers
// ---------------------------------------------------------------------------

/// Create a GPU texture handle with the given format and usage flags.
fn make_gpu_texture(
    images: &mut Assets<Image>,
    w: u32,
    h: u32,
    format: TextureFormat,
) -> Handle<Image> {
    let size = Extent3d {
        width: w.max(1),
        height: h.max(1),
        depth_or_array_layers: 1,
    };
    // Compute fill pixel size from format.
    let pixel_bytes = match format {
        TextureFormat::R8Unorm => 1,
        TextureFormat::Rgba8Unorm => 4,
        TextureFormat::Rgba16Float => 8,
        _ => 4,
    };
    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &vec![0u8; pixel_bytes],
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage =
        TextureUsages::COPY_DST | TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING;
    images.add(image)
}

/// Create a GPU texture initialized to white (1.0 in all channels, Rgba16Float).
/// Used for lightmap textures so tiles render at full brightness before the RC
/// pipeline produces its first output.
fn make_white_gpu_texture(images: &mut Assets<Image>, w: u32, h: u32) -> Handle<Image> {
    let size = Extent3d {
        width: w.max(1),
        height: h.max(1),
        depth_or_array_layers: 1,
    };
    // f16 1.0 = 0x3C00 → little-endian [0x00, 0x3C]
    let white_f16: [u8; 8] = [0x00, 0x3C, 0x00, 0x3C, 0x00, 0x3C, 0x00, 0x3C];
    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &white_f16,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage =
        TextureUsages::COPY_DST | TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING;
    images.add(image)
}

/// Create all GPU image handles with small default sizes.
/// Called once during plugin setup.
pub(crate) fn create_gpu_images(images: &mut Assets<Image>) -> RcGpuImages {
    let s = 64;
    RcGpuImages {
        density: make_gpu_texture(images, s, s, TextureFormat::R8Unorm),
        density_bg: make_gpu_texture(images, s, s, TextureFormat::R8Unorm),
        emissive: make_gpu_texture(images, s, s, TextureFormat::Rgba16Float),
        albedo: make_gpu_texture(images, s, s, TextureFormat::Rgba8Unorm),
        cascade_a: make_gpu_texture(images, s * 2, s * 2, TextureFormat::Rgba16Float),
        cascade_b: make_gpu_texture(images, s * 2, s * 2, TextureFormat::Rgba16Float),
        lightmap: make_white_gpu_texture(images, s, s),
        lightmap_prev: make_white_gpu_texture(images, s, s),
    }
}

/// Swap lightmap ↔ lightmap_prev each frame so the previous frame's output
/// becomes the bounce-light input for the next frame.
pub(crate) fn swap_lightmap_handles(mut gpu_images: ResMut<RcGpuImages>) {
    let images = gpu_images.as_mut();
    let tmp = images.lightmap.clone();
    images.lightmap = images.lightmap_prev.clone();
    images.lightmap_prev = tmp;
}

/// Resize GPU textures when the RC input dimensions change.
/// Replaces image handles in `RcGpuImages` with new ones of the correct size.
pub(crate) fn resize_gpu_textures(
    mut config: ResMut<RcLightingConfig>,
    mut gpu_images: ResMut<RcGpuImages>,
    mut images: ResMut<Assets<Image>>,
) {
    let input_w = config.input_size.x;
    let input_h = config.input_size.y;
    let vp_w = config.viewport_size.x;
    let vp_h = config.viewport_size.y;

    if input_w == 0 || input_h == 0 {
        return;
    }

    // Check if density texture needs resize by comparing with config
    let needs_resize = images.get(&gpu_images.density).is_none_or(|img| {
        img.texture_descriptor.size.width != input_w
            || img.texture_descriptor.size.height != input_h
    });

    if !needs_resize {
        return;
    }

    // Recreate input textures at new size
    gpu_images.density = make_gpu_texture(&mut images, input_w, input_h, TextureFormat::R8Unorm);
    gpu_images.density_bg = make_gpu_texture(&mut images, input_w, input_h, TextureFormat::R8Unorm);
    gpu_images.emissive =
        make_gpu_texture(&mut images, input_w, input_h, TextureFormat::Rgba16Float);
    gpu_images.albedo = make_gpu_texture(&mut images, input_w, input_h, TextureFormat::Rgba8Unorm);

    // Cascade textures: sized for the largest cascade's packed probe×direction grid.
    // Cascade 0 has 4 directions (2×2 per probe), spacing=1, so texture = input_w*2 × input_h*2.
    let cascade_w = input_w * 2;
    let cascade_h = input_h * 2;
    gpu_images.cascade_a = make_gpu_texture(
        &mut images,
        cascade_w,
        cascade_h,
        TextureFormat::Rgba16Float,
    );
    gpu_images.cascade_b = make_gpu_texture(
        &mut images,
        cascade_w,
        cascade_h,
        TextureFormat::Rgba16Float,
    );

    // Lightmap textures: viewport-sized, initialized to white to avoid dark flash
    gpu_images.lightmap = make_white_gpu_texture(&mut images, vp_w.max(1), vp_h.max(1));
    gpu_images.lightmap_prev = make_white_gpu_texture(&mut images, vp_w.max(1), vp_h.max(1));

    config.lightmap_size = UVec2::new(vp_w.max(1), vp_h.max(1));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_bytes_per_row_already_aligned() {
        assert_eq!(align_bytes_per_row(256), 256);
        assert_eq!(align_bytes_per_row(512), 512);
    }

    #[test]
    fn align_bytes_per_row_rounds_up() {
        assert_eq!(align_bytes_per_row(1), 256);
        assert_eq!(align_bytes_per_row(97), 256);
        assert_eq!(align_bytes_per_row(257), 512);
    }

    #[test]
    fn pad_rows_no_padding_needed() {
        // 256 bytes per row — already aligned
        let src = vec![42u8; 256 * 2];
        let (padded, bpr) = pad_rows(&src, 256, 2);
        assert_eq!(bpr, 256);
        assert_eq!(padded, src);
    }

    #[test]
    fn pad_rows_adds_padding() {
        // 97 bytes per row, 2 rows → aligned to 256
        let src: Vec<u8> = (0..97 * 2).map(|i| (i % 256) as u8).collect();
        let (padded, bpr) = pad_rows(&src, 97, 2);
        assert_eq!(bpr, 256);
        assert_eq!(padded.len(), 256 * 2);
        // First row data preserved
        assert_eq!(&padded[..97], &src[..97]);
        // Padding is zeroes
        assert!(padded[97..256].iter().all(|&b| b == 0));
        // Second row data preserved
        assert_eq!(&padded[256..256 + 97], &src[97..194]);
    }

    #[test]
    fn f16_conversion_zero() {
        assert_eq!(f32_to_f16_bits(0.0), 0x0000);
    }

    #[test]
    fn f16_conversion_one() {
        // f16 1.0 = 0 01111 0000000000 = 0x3C00
        assert_eq!(f32_to_f16_bits(1.0), 0x3C00);
    }

    #[test]
    fn f16_conversion_negative() {
        // f16 -1.0 = 1 01111 0000000000 = 0xBC00
        assert_eq!(f32_to_f16_bits(-1.0), 0xBC00);
    }

    #[test]
    fn f16_conversion_half() {
        // f16 0.5 = 0 01110 0000000000 = 0x3800
        assert_eq!(f32_to_f16_bits(0.5), 0x3800);
    }

    #[test]
    fn f16_conversion_inf() {
        assert_eq!(f32_to_f16_bits(f32::INFINITY), 0x7C00);
        assert_eq!(f32_to_f16_bits(f32::NEG_INFINITY), 0xFC00);
    }

    #[test]
    fn emissive_roundtrip_zeros() {
        let data = vec![[0.0f32; 4]; 4];
        let bytes = emissive_to_f16_bytes(&data);
        assert_eq!(bytes.len(), 4 * 8); // 4 pixels × 8 bytes each
        assert!(bytes.iter().all(|&b| b == 0));
    }

    #[test]
    fn emissive_roundtrip_ones() {
        let data = vec![[1.0, 1.0, 1.0, 1.0]];
        let bytes = emissive_to_f16_bytes(&data);
        assert_eq!(bytes.len(), 8);
        // Each channel should be f16 1.0 = 0x3C00 = [0x00, 0x3C] in little-endian
        for chunk in bytes.chunks(2) {
            assert_eq!(chunk, &[0x00, 0x3C]);
        }
    }
}
