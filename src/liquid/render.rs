use std::collections::HashSet;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    TextureDimension, TextureFormat,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dKey};

use crate::liquid::data::{LiquidCell, LiquidId};
use crate::liquid::registry::LiquidRegistry;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::{self, ChunkCoord, LoadedChunks, WorldMap};

// ---------------------------------------------------------------------------
// Material
// ---------------------------------------------------------------------------

#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct LiquidMaterial {
    #[uniform(0)]
    pub color: LinearRgba,
    #[texture(1)]
    #[sampler(2)]
    pub lightmap: Handle<Image>,
    #[uniform(3)]
    pub lightmap_uv_rect: Vec4, // (scale_x, scale_y, offset_x, offset_y)
}

impl Material2d for LiquidMaterial {
    fn vertex_shader() -> ShaderRef {
        "engine/shaders/liquid.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "engine/shaders/liquid.wgsl".into()
    }

    fn alpha_mode(&self) -> bevy::sprite_render::AlphaMode2d {
        bevy::sprite_render::AlphaMode2d::Blend
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_COLOR.at_shader_location(1),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        // Enable alpha blending for semi-transparent liquid
        if let Some(target) = descriptor
            .fragment
            .as_mut()
            .and_then(|f| f.targets.get_mut(0))
            .and_then(|t| t.as_mut())
        {
            target.blend = Some(bevy::render::render_resource::BlendState::ALPHA_BLENDING);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Scalar-field material (metaball-style liquid rendering)
// ---------------------------------------------------------------------------

/// Per-channel uniforms for the height-based liquid shader.
///
/// Each liquid type (water, lava, oil) gets its own color. The `threshold` and
/// `smoothing` fields are kept for backward compatibility but are currently
/// unused by the height-based shader (they were used by the old metaball
/// approach).
#[derive(Clone, ShaderType)]
pub struct LiquidFieldUniforms {
    pub water_color: Vec4,
    pub lava_color: Vec4,
    pub oil_color: Vec4,
    pub threshold: f32,
    pub smoothing: f32,
    /// World-space size of one tile in pixels.
    pub tile_size: f32,
    /// Elapsed time in seconds for animated effects (Voronoi caustics).
    pub time: f32,
    /// World-space origin of the field texture (bottom-left corner).
    pub field_origin: Vec2,
    /// Padding to satisfy GPU alignment (16-byte boundary).
    pub _pad: Vec2,
}

/// Material that samples a scalar field texture (R=water, G=lava, B=oil)
/// and renders height-proportional filled tiles with animated Voronoi caustics,
/// composited with the existing lightmap.
#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct LiquidFieldMaterial {
    #[uniform(0)]
    pub uniforms: LiquidFieldUniforms,
    /// The scalar field texture (R=water, G=lava, B=oil).
    #[texture(1)]
    #[sampler(2)]
    pub field_texture: Handle<Image>,
    /// Lightmap
    #[texture(3)]
    #[sampler(4)]
    pub lightmap: Handle<Image>,
    #[uniform(5)]
    pub lightmap_uv_rect: Vec4,
}

impl Material2d for LiquidFieldMaterial {
    fn vertex_shader() -> ShaderRef {
        "engine/shaders/liquid_field.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "engine/shaders/liquid_field.wgsl".into()
    }

    fn alpha_mode(&self) -> bevy::sprite_render::AlphaMode2d {
        bevy::sprite_render::AlphaMode2d::Blend
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        // Enable alpha blending for semi-transparent liquid
        if let Some(target) = descriptor
            .fragment
            .as_mut()
            .and_then(|f| f.targets.get_mut(0))
            .and_then(|t| t.as_mut())
        {
            target.blend = Some(bevy::render::render_resource::BlendState::ALPHA_BLENDING);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Render configuration (tunable via debug panel)
// ---------------------------------------------------------------------------

/// Tunable parameters for the scalar-field liquid renderer.
///
/// Exposed as egui sliders in the F8 liquid debug panel so artists and
/// developers can tweak the metaball look at runtime.
#[derive(Resource)]
pub struct LiquidRenderConfig {
    /// Smoothstep threshold — controls how much liquid level is needed before
    /// a pixel becomes visible. Lower = more visible, higher = tighter blobs.
    pub threshold: f32,
    /// Smoothstep transition width — controls the softness of blob edges.
    pub smoothing: f32,
    /// Box-blur radius applied to the scalar field before upload.
    /// 0 = no blur, 1 = 3×3, 2 = 5×5, 3 = 7×7.
    pub blur_radius: u32,
    /// When true, show the old per-chunk debug meshes (simple colored rectangles).
    pub show_debug_meshes: bool,
}

impl Default for LiquidRenderConfig {
    fn default() -> Self {
        Self {
            threshold: 0.33,
            smoothing: 0.095,
            blur_radius: 0,
            show_debug_meshes: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared material resources
// ---------------------------------------------------------------------------

/// Shared liquid material handle, created once on InGame enter.
#[derive(Resource)]
pub struct SharedLiquidMaterial(pub Handle<LiquidMaterial>);

/// Shared scalar-field liquid material handle, created once on InGame enter.
#[derive(Resource)]
pub struct SharedLiquidFieldMaterial(pub Handle<LiquidFieldMaterial>);

/// Marker component for the full-viewport quad that renders the scalar field.
#[derive(Component)]
pub struct LiquidFieldQuad;

/// Set of data chunk coords whose liquid meshes need rebuilding.
/// Populated by the liquid simulation system, consumed by the rebuild system.
#[derive(Resource, Default)]
pub struct DirtyLiquidChunks(pub HashSet<(i32, i32)>);

// ---------------------------------------------------------------------------
// Scalar-field texture resource
// ---------------------------------------------------------------------------

/// GPU texture storing per-tile liquid levels for the visible area.
///
/// One pixel per tile. R = water level, G = lava level, B = oil level,
/// A = max(R, G, B). Uses `FilterMode::Nearest` because the shader reads
/// exact pixel values via `textureLoad` (no GPU-side interpolation).
///
/// Created in `init_liquid_material` with a 1x1 default; resized each frame
/// by `upload_liquid_field` to match the visible tile rectangle.
#[derive(Resource)]
pub struct LiquidFieldTexture {
    /// Handle to the GPU image asset.
    pub handle: Handle<Image>,
    /// CPU-side RGBA8 pixel buffer, reused across frames.
    pub pixels: Vec<u8>,
    /// Bottom-left tile X coordinate of the texture origin.
    pub origin_tx: i32,
    /// Bottom-left tile Y coordinate of the texture origin.
    pub origin_ty: i32,
    /// Texture width in tiles (pixels).
    pub width: u32,
    /// Texture height in tiles (pixels).
    pub height: u32,
}

// ---------------------------------------------------------------------------
// Marker component
// ---------------------------------------------------------------------------

/// Marker component for liquid mesh entities, linking them to their chunk.
#[derive(Component)]
pub struct LiquidMeshEntity;

// ---------------------------------------------------------------------------
// Mesh builder
// ---------------------------------------------------------------------------

/// Build a mesh for one chunk's liquid layer.
///
/// Each non-empty liquid cell becomes a quad whose height is proportional to
/// the cell's level (0..1). The quad fills the full tile width and sits at the
/// bottom of the tile.
pub fn build_liquid_mesh(
    cells: &[LiquidCell],
    display_chunk_x: i32,
    chunk_y: i32,
    chunk_size: u32,
    tile_size: f32,
    liquid_registry: &LiquidRegistry,
) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let base_x = display_chunk_x as f32 * chunk_size as f32 * tile_size;
    let base_y = chunk_y as f32 * chunk_size as f32 * tile_size;

    for local_y in 0..chunk_size {
        for local_x in 0..chunk_size {
            let idx = (local_y * chunk_size + local_x) as usize;
            if idx >= cells.len() {
                continue;
            }
            let cell = cells[idx];
            if cell.is_empty() {
                continue;
            }

            let color = liquid_registry
                .get(cell.liquid_type)
                .map(|d| d.color)
                .unwrap_or([0.0, 0.0, 1.0, 0.5]);

            let x = base_x + local_x as f32 * tile_size;
            let y = base_y + local_y as f32 * tile_size;
            // Minimum visible height of 2px so small amounts don't disappear.
            let min_height = 2.0_f32.min(tile_size * 0.25);
            let height = (cell.level.clamp(0.0, 1.0) * tile_size).max(min_height);

            let vi = positions.len() as u32;
            // Quad: bottom-left, bottom-right, top-right, top-left
            positions.push([x, y, 0.0]);
            positions.push([x + tile_size, y, 0.0]);
            positions.push([x + tile_size, y + height, 0.0]);
            positions.push([x, y + height, 0.0]);

            colors.push(color);
            colors.push(color);
            colors.push(color);
            colors.push(color);

            indices.push(vi);
            indices.push(vi + 1);
            indices.push(vi + 2);
            indices.push(vi);
            indices.push(vi + 2);
            indices.push(vi + 3);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

// ---------------------------------------------------------------------------
// Init system — create shared material on InGame enter
// ---------------------------------------------------------------------------

pub fn init_liquid_material(
    mut commands: Commands,
    mut materials: ResMut<Assets<LiquidMaterial>>,
    mut field_materials: ResMut<Assets<LiquidFieldMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    config: Res<ActiveWorld>,
) {
    // Create our own 1x1 white fallback lightmap so we don't depend on
    // FallbackLightmap resource ordering during OnEnter(InGame).
    let white_lm = images.add(Image::new_fill(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0x00u8, 0x3C, 0x00, 0x3C, 0x00, 0x3C, 0x00, 0x3C], // f16 1.0
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    ));

    let handle = materials.add(LiquidMaterial {
        color: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
        lightmap: white_lm.clone(),
        lightmap_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
    });
    commands.insert_resource(SharedLiquidMaterial(handle));

    // Create 1×1 default scalar-field texture (resized each frame by
    // upload_liquid_field to match the visible tile rectangle).
    let mut field_image = Image::new_fill(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0u8, 0, 0, 0],
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    // Nearest filtering — the shader uses textureLoad for exact per-tile
    // lookups, so no GPU interpolation is needed.
    field_image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Nearest,
        min_filter: ImageFilterMode::Nearest,
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        ..default()
    });
    let field_handle = images.add(field_image);

    commands.insert_resource(LiquidFieldTexture {
        handle: field_handle.clone(),
        pixels: vec![0u8; 4],
        origin_tx: 0,
        origin_ty: 0,
        width: 1,
        height: 1,
    });

    // --- Scalar-field material + quad entity ---
    let field_mat_handle = field_materials.add(LiquidFieldMaterial {
        uniforms: LiquidFieldUniforms {
            water_color: Vec4::new(0.2, 0.4, 0.8, 0.35),
            lava_color: Vec4::new(1.0, 0.3, 0.0, 1.0),
            oil_color: Vec4::new(0.15, 0.1, 0.05, 0.85),
            threshold: 0.33,
            smoothing: 0.095,
            tile_size: config.tile_size,
            time: 0.0,
            field_origin: Vec2::ZERO,
            _pad: Vec2::ZERO,
        },
        field_texture: field_handle,
        lightmap: white_lm,
        lightmap_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
    });

    commands.insert_resource(SharedLiquidFieldMaterial(field_mat_handle.clone()));

    // Spawn the full-viewport quad. Placeholder size; resized each frame by
    // `update_liquid_field_quad` to match the scalar field coverage area.
    let quad_mesh = meshes.add(Rectangle::new(8.0, 8.0));
    commands.spawn((
        LiquidFieldQuad,
        Mesh2d(quad_mesh),
        MeshMaterial2d(field_mat_handle),
        Transform::from_translation(Vec3::new(0.0, 0.0, 2.0)),
        Visibility::default(),
    ));
}

// ---------------------------------------------------------------------------
// Rebuild system — rebuild liquid meshes for dirty chunks
// ---------------------------------------------------------------------------

/// Rebuild liquid meshes for chunks whose liquid data has changed.
///
/// Uses the `DirtyLiquidChunks` resource (populated by the liquid simulation)
/// to determine which chunks need a mesh rebuild. Clears the dirty set after
/// processing so rebuilds only happen once per change.
#[allow(clippy::too_many_arguments)]
pub fn rebuild_liquid_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    world_map: Res<WorldMap>,
    config: Res<ActiveWorld>,
    liquid_registry: Res<LiquidRegistry>,
    loaded_chunks: Res<LoadedChunks>,
    mut dirty_liquid: ResMut<DirtyLiquidChunks>,
    render_config: Res<LiquidRenderConfig>,
    liquid_query: Query<(Entity, &ChunkCoord, &Visibility), With<LiquidMeshEntity>>,
) {
    let target_vis = if render_config.show_debug_meshes {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };

    // Update visibility for all liquid mesh entities when toggle changes.
    for (entity, _coord, vis) in &liquid_query {
        if *vis != target_vis {
            commands.entity(entity).insert(target_vis);
        }
    }

    if dirty_liquid.0.is_empty() || !render_config.show_debug_meshes {
        dirty_liquid.0.clear();
        return;
    }

    for (entity, coord, _vis) in &liquid_query {
        let data_cx = config.wrap_chunk_x(coord.x);
        if !dirty_liquid.0.contains(&(data_cx, coord.y)) {
            continue;
        }

        // Verify this display chunk is still loaded.
        if !loaded_chunks.map.contains_key(&(coord.x, coord.y)) {
            continue;
        }

        let Some(chunk) = world_map.chunk(data_cx, coord.y) else {
            continue;
        };

        let mesh = build_liquid_mesh(
            &chunk.liquid.cells,
            coord.x,
            coord.y,
            config.chunk_size,
            config.tile_size,
            &liquid_registry,
        );

        commands.entity(entity).insert(Mesh2d(meshes.add(mesh)));
    }

    dirty_liquid.0.clear();
}

// ---------------------------------------------------------------------------
// Scalar-field upload system
// ---------------------------------------------------------------------------

/// Padding (in tiles) added around the visible area to avoid popping at edges.
const FIELD_PADDING: i32 = 4;

/// Each frame, compute the visible tile rectangle from the camera, fill the
/// CPU pixel buffer with per-tile liquid levels, and upload to the GPU image.
///
/// Channel mapping: R = water (LiquidId(1)), G = lava (LiquidId(2)),
/// B = oil (LiquidId(3)), A = max(R, G, B).
#[allow(clippy::too_many_arguments)]
pub fn upload_liquid_field(
    mut field: ResMut<LiquidFieldTexture>,
    mut images: ResMut<Assets<Image>>,
    world_map: Res<WorldMap>,
    config: Res<ActiveWorld>,
    render_config: Res<LiquidRenderConfig>,
    camera_query: Query<(&Camera, &Transform, &Projection), With<Camera2d>>,
) {
    // --- Camera viewport geometry ---
    let Ok((camera, camera_tf, projection)) = camera_query.single() else {
        return;
    };

    let viewport_pixels = camera
        .physical_viewport_size()
        .unwrap_or(UVec2::new(1280, 720));
    let scale = match projection {
        Projection::Orthographic(ortho) => ortho.scale,
        _ => 1.0,
    };

    let tile_size = config.tile_size;
    let vp_world_w = viewport_pixels.x as f32 * scale;
    let vp_world_h = viewport_pixels.y as f32 * scale;

    // Viewport size in tiles (ceiling to cover partial tiles at edges).
    let vp_tiles_w = (vp_world_w / tile_size).ceil() as i32;
    let vp_tiles_h = (vp_world_h / tile_size).ceil() as i32;

    // Camera center in tile coordinates.
    // Use Transform (not GlobalTransform) because GlobalTransform isn't
    // propagated until PostUpdate, and this system runs in Update.
    let camera_pos = camera_tf.translation.truncate();
    let half_w = vp_tiles_w / 2 + FIELD_PADDING;
    let half_h = vp_tiles_h / 2 + FIELD_PADDING;

    let cam_tx = (camera_pos.x / tile_size).floor() as i32;
    let cam_ty = (camera_pos.y / tile_size).floor() as i32;

    let min_tx = cam_tx - half_w;
    let min_ty = (cam_ty - half_h).max(0);
    let max_tx = cam_tx + half_w;
    let max_ty = (cam_ty + half_h).min(config.height_tiles - 1);

    let new_w = (max_tx - min_tx + 1).max(1) as u32;
    let new_h = (max_ty - min_ty + 1).max(1) as u32;

    // --- Resize texture if dimensions changed ---
    if new_w != field.width || new_h != field.height {
        field.width = new_w;
        field.height = new_h;
        let buf_len = (new_w * new_h * 4) as usize;
        field.pixels.resize(buf_len, 0);

        // Replace the GPU image in-place so the Handle stays valid for
        // downstream materials.
        if let Some(image) = images.get_mut(&field.handle) {
            *image = Image::new(
                Extent3d {
                    width: new_w,
                    height: new_h,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                vec![0u8; buf_len],
                TextureFormat::Rgba8Unorm,
                RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
            );
            image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                mag_filter: ImageFilterMode::Nearest,
                min_filter: ImageFilterMode::Nearest,
                address_mode_u: ImageAddressMode::ClampToEdge,
                address_mode_v: ImageAddressMode::ClampToEdge,
                ..default()
            });
        }
    }

    field.origin_tx = min_tx;
    field.origin_ty = min_ty;

    // --- Clear pixel buffer ---
    field.pixels.fill(0);

    // --- Fill pixels from WorldMap liquid data ---
    let chunk_size = config.chunk_size;
    for ty in min_ty..=max_ty {
        // Skip out-of-bounds Y (shouldn't happen after clamping, but be safe).
        if ty < 0 || ty >= config.height_tiles {
            continue;
        }
        let py = (ty - min_ty) as u32;
        for tx in min_tx..=max_tx {
            let px = (tx - min_tx) as u32;

            // Wrap X for world access (world wraps horizontally).
            let wx = config.wrap_tile_x(tx);
            let (cx, cy) = chunk::tile_to_chunk(wx, ty, chunk_size);
            let (lx, ly) = chunk::tile_to_local(wx, ty, chunk_size);

            let cell = match world_map.chunk(cx, cy) {
                Some(c) => c.liquid.get(lx, ly, chunk_size),
                None => LiquidCell::EMPTY,
            };

            if cell.is_empty() {
                continue;
            }

            let level_u8 = (cell.level.clamp(0.0, 1.0) * 255.0) as u8;

            // Map liquid type to channel: water=R, lava=G, oil=B.
            let (r, g, b) = match cell.liquid_type {
                LiquidId(1) => (level_u8, 0u8, 0u8),
                LiquidId(2) => (0u8, level_u8, 0u8),
                LiquidId(3) => (0u8, 0u8, level_u8),
                _ => (level_u8, 0u8, 0u8), // fallback: treat as water
            };
            let a = r.max(g).max(b);

            // Image is stored top-to-bottom in GPU memory, but our tile Y
            // increases upward. Flip Y so row 0 of the image = top of the
            // visible area (highest tile Y).
            let flipped_y = new_h - 1 - py;
            let offset = ((flipped_y * new_w + px) * 4) as usize;
            field.pixels[offset] = r;
            field.pixels[offset + 1] = g;
            field.pixels[offset + 2] = b;
            field.pixels[offset + 3] = a;
        }
    }

    // Wall-wetting dilation is not needed for the height-based shader.
    // The old blob shader required it because bilinear filtering created gaps
    // at walls. The height-based shader fills water up to its level within
    // each tile using textureLoad, so there's no gap.

    // Apply blur for metaball merging effect (radius 0 = no-op by default).
    let (w, h, r) = (field.width, field.height, render_config.blur_radius);
    blur_liquid_field(&mut field.pixels, w, h, r);

    // --- Upload to GPU image ---
    if let Some(data) = images
        .get_mut(&field.handle)
        .and_then(|img| img.data.as_mut())
    {
        let len = field.pixels.len().min(data.len());
        data[..len].copy_from_slice(&field.pixels[..len]);
    }
}

// ---------------------------------------------------------------------------
// CPU-side box blur for metaball merging
// ---------------------------------------------------------------------------

/// Separable box blur on RGBA8 pixel buffer. Radius 1 = 3×3 kernel.
fn blur_liquid_field(pixels: &mut [u8], width: u32, height: u32, radius: u32) {
    if radius == 0 || width == 0 || height == 0 {
        return;
    }
    let w = width as usize;
    let h = height as usize;
    let r = radius as usize;
    let mut temp = vec![0u8; pixels.len()];

    // Horizontal pass: pixels → temp
    for y in 0..h {
        for x in 0..w {
            let mut sums = [0u32; 4];
            let mut count = 0u32;
            let x_lo = x.saturating_sub(r);
            let x_hi = (x + r + 1).min(w);
            for sx in x_lo..x_hi {
                let si = (y * w + sx) * 4;
                for c in 0..4 {
                    sums[c] += pixels[si + c] as u32;
                }
                count += 1;
            }
            let di = (y * w + x) * 4;
            for c in 0..4 {
                temp[di + c] = (sums[c] / count) as u8;
            }
        }
    }

    // Vertical pass: temp → pixels
    for y in 0..h {
        for x in 0..w {
            let mut sums = [0u32; 4];
            let mut count = 0u32;
            let y_lo = y.saturating_sub(r);
            let y_hi = (y + r + 1).min(h);
            for sy in y_lo..y_hi {
                let si = (sy * w + x) * 4;
                for c in 0..4 {
                    sums[c] += temp[si + c] as u32;
                }
                count += 1;
            }
            let di = (y * w + x) * 4;
            for c in 0..4 {
                pixels[di + c] = (sums[c] / count) as u8;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Quad update system — resize/reposition quad to match field coverage
// ---------------------------------------------------------------------------

/// Each frame, update the scalar-field quad mesh and material to match the
/// current field texture coverage area and liquid colors from the registry.
///
/// Runs after `upload_liquid_field` so the field texture dimensions are fresh.
#[allow(clippy::too_many_arguments)]
pub fn update_liquid_field_quad(
    field: Res<LiquidFieldTexture>,
    config: Res<ActiveWorld>,
    liquid_registry: Res<LiquidRegistry>,
    render_config: Res<LiquidRenderConfig>,
    time: Res<Time>,
    shared_mat: Option<Res<SharedLiquidFieldMaterial>>,
    mut materials: ResMut<Assets<LiquidFieldMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut quad_q: Query<(&mut Mesh2d, &mut Transform), With<LiquidFieldQuad>>,
) {
    let Some(shared_mat) = shared_mat else { return };
    let Some(mat) = materials.get_mut(&shared_mat.0) else {
        return;
    };

    // Update field texture handle (may have been recreated).
    mat.field_texture = field.handle.clone();

    // Apply render config to material uniforms.
    mat.uniforms.threshold = render_config.threshold;
    mat.uniforms.smoothing = render_config.smoothing;
    mat.uniforms.tile_size = config.tile_size;
    mat.uniforms.time = time.elapsed_secs();
    mat.uniforms.field_origin = Vec2::new(
        field.origin_tx as f32 * config.tile_size,
        field.origin_ty as f32 * config.tile_size,
    );

    // Update liquid colors from registry.
    if let Some(water) = liquid_registry.get(LiquidId(1)) {
        mat.uniforms.water_color = Vec4::from(water.color);
    }
    if let Some(lava) = liquid_registry.get(LiquidId(2)) {
        mat.uniforms.lava_color = Vec4::from(lava.color);
    }
    if let Some(oil) = liquid_registry.get(LiquidId(3)) {
        mat.uniforms.oil_color = Vec4::from(oil.color);
    }

    // Rebuild quad mesh to match field coverage.
    if field.width == 0 || field.height == 0 {
        return;
    }

    let world_w = field.width as f32 * config.tile_size;
    let world_h = field.height as f32 * config.tile_size;
    let origin_x = field.origin_tx as f32 * config.tile_size;
    let origin_y = field.origin_ty as f32 * config.tile_size;

    let quad_mesh = Rectangle::new(world_w, world_h);

    for (mut mesh_handle, mut transform) in &mut quad_q {
        *mesh_handle = Mesh2d(meshes.add(quad_mesh));
        // Center the quad over the field area.
        transform.translation = Vec3::new(origin_x + world_w * 0.5, origin_y + world_h * 0.5, 2.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::liquid::data::{LiquidCell, LiquidId};

    #[test]
    fn empty_cells_produce_empty_mesh() {
        let cells = vec![LiquidCell::EMPTY; 4];
        let registry = LiquidRegistry::default();
        let mesh = build_liquid_mesh(&cells, 0, 0, 2, 8.0, &registry);
        // No vertices for empty cells
        assert!(mesh.attribute(Mesh::ATTRIBUTE_POSITION).is_some());
        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap();
        assert_eq!(positions.len(), 0);
    }

    #[test]
    fn non_empty_cell_produces_quad() {
        let mut cells = vec![LiquidCell::EMPTY; 4];
        cells[0] = LiquidCell {
            liquid_type: LiquidId(1),
            level: 0.5,
        };
        let registry = LiquidRegistry::from_defs(vec![crate::liquid::registry::LiquidDef {
            name: "water".into(),
            density: 1.0,
            viscosity: 1.0,
            color: [0.0, 0.3, 0.8, 0.6],
            damage_on_contact: 0.0,
            light_emission: [0, 0, 0],
            light_opacity: 0,
            swim_speed_factor: 0.5,
            flicker_speed: 0.0,
            flicker_strength: 0.0,
            flicker_min: 1.0,
            reactions: vec![],
        }]);
        let mesh = build_liquid_mesh(&cells, 0, 0, 2, 8.0, &registry);
        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap();
        // 1 non-empty cell = 1 quad = 4 vertices
        assert_eq!(positions.len(), 4);
        let indices = mesh.indices().unwrap();
        assert_eq!(indices.len(), 6);
    }
}
