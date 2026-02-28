use bevy::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};

use crate::registry::tile::{TileId, TileRegistry};
use crate::registry::world::WorldConfig;
use crate::sets::GameSet;
use crate::world::chunk::{tile_to_chunk, tile_to_local, world_to_tile, WorldMap};
use crate::world::rc_pipeline;
use crate::world::tile_renderer::{SharedTileMaterial, TileMaterial};

/// Padding in tiles around the visible viewport for the RC input textures.
/// Must be >= interval_end of the highest useful cascade so that rays from
/// viewport probes don't escape the grid. With 3 cascades the max ray
/// distance is 4^3 = 64, so padding = 64 keeps all viewport rays in-bounds.
const RC_PADDING_TILES: i32 = 64;

/// Warm-white sun color used for sky emitters along the top row.
const SUN_COLOR: [f32; 3] = [1.0, 0.98, 0.90];

/// Configuration for the radiance cascades lighting pipeline.
#[derive(Resource, Clone, ExtractResource)]
pub struct RcLightingConfig {
    /// Size of RC input textures (viewport + padding) in tiles.
    pub input_size: UVec2,
    /// Viewport size in tiles (before padding).
    pub viewport_size: UVec2,
    /// Offset from input origin to viewport origin (padding amount).
    pub viewport_offset: UVec2,
    /// World-space size of one tile in pixels.
    pub tile_size: f32,
    /// Number of radiance cascade levels.
    pub cascade_count: u32,
    /// Damping factor for bounce light (0.0 = no bounce, 1.0 = full energy).
    pub bounce_damping: f32,
    /// Lightmap output size in tiles (set by `resize_gpu_textures`).
    pub lightmap_size: UVec2,
    /// Viewport size in world units (viewport_pixels * ortho_scale).
    /// Used for correct lightmap UV mapping when ortho scale ≠ 1.
    pub vp_world: Vec2,
}

impl Default for RcLightingConfig {
    fn default() -> Self {
        Self {
            input_size: UVec2::ZERO,
            viewport_size: UVec2::ZERO,
            viewport_offset: UVec2::ZERO,
            tile_size: 32.0,
            cascade_count: 1,
            bounce_damping: 0.4,
            lightmap_size: UVec2::ZERO,
            vp_world: Vec2::ZERO,
        }
    }
}

/// CPU-side buffers holding per-tile density, emissive, and albedo data
/// extracted each frame for GPU upload.
#[derive(Resource, Clone, Default, ExtractResource)]
pub struct RcInputData {
    /// 0 = air, 255 = solid. One byte per tile. (FG layer)
    pub density: Vec<u8>,
    /// 0 = air, 255 = solid. One byte per tile. (BG layer)
    pub density_bg: Vec<u8>,
    /// RGBA float per tile. Emissive light sources.
    pub emissive: Vec<[f32; 4]>,
    /// RGBA u8 per tile. Surface albedo for bounce light.
    pub albedo: Vec<[u8; 4]>,
    /// Width of the input grid in tiles.
    pub width: u32,
    /// Height of the input grid in tiles.
    pub height: u32,
    /// Whether buffers were updated this frame.
    pub dirty: bool,
}

// `Default` derived: all Vecs empty, numerics 0, dirty false.

/// Plugin that registers RC lighting resources and the per-frame extract system.
pub struct RcLightingPlugin;

impl Plugin for RcLightingPlugin {
    fn build(&self, app: &mut App) {
        // Create GPU image handles in the main world (small defaults, resized each frame).
        let gpu_images = rc_pipeline::create_gpu_images(
            app.world_mut().resource_mut::<Assets<Image>>().as_mut(),
        );

        app.init_resource::<RcLightingConfig>()
            .init_resource::<RcInputData>()
            .insert_resource(gpu_images)
            .add_plugins((
                ExtractResourcePlugin::<RcLightingConfig>::default(),
                ExtractResourcePlugin::<RcInputData>::default(),
                ExtractResourcePlugin::<rc_pipeline::RcGpuImages>::default(),
            ))
            .add_systems(
                Update,
                (
                    // Lighting runs AFTER Camera so it sees the current frame's
                    // camera position, not the previous frame's. This prevents
                    // the lightmap from being misaligned with the rendered tiles.
                    extract_lighting_data.after(GameSet::Camera),
                    rc_pipeline::resize_gpu_textures
                        .after(extract_lighting_data)
                        .after(GameSet::Camera),
                    rc_pipeline::swap_lightmap_handles
                        .after(rc_pipeline::resize_gpu_textures)
                        .after(GameSet::Camera),
                    update_tile_lightmap
                        .after(rc_pipeline::swap_lightmap_handles)
                        .after(GameSet::Camera),
                ),
            );

        // Set up the render-side pipeline (render app systems + graph node).
        rc_pipeline::setup_render_pipeline(app);
    }
}

/// Look up a foreground tile without requiring `WorldCtxRef`.
/// Returns stone (bedrock) for `tile_y < 0`, `None` for above-world or unloaded chunks.
fn get_fg_tile(
    world_map: &WorldMap,
    tile_x: i32,
    tile_y: i32,
    world_config: &WorldConfig,
    tile_registry: &TileRegistry,
) -> Option<TileId> {
    if tile_y < 0 {
        return Some(tile_registry.by_name("stone")); // bedrock below world
    }
    if tile_y >= world_config.height_tiles {
        return None; // above world, treat as air
    }
    let wrapped_x = world_config.wrap_tile_x(tile_x);
    let (cx, cy) = tile_to_chunk(wrapped_x, tile_y, world_config.chunk_size);
    let (lx, ly) = tile_to_local(wrapped_x, tile_y, world_config.chunk_size);
    world_map
        .chunk(cx, cy)
        .map(|chunk| chunk.fg.get(lx, ly, world_config.chunk_size))
}

/// Look up a background tile without requiring `WorldCtxRef`.
/// Returns stone (bedrock) for `tile_y < 0`, `None` for above-world or unloaded chunks.
fn get_bg_tile(
    world_map: &WorldMap,
    tile_x: i32,
    tile_y: i32,
    world_config: &WorldConfig,
    tile_registry: &TileRegistry,
) -> Option<TileId> {
    if tile_y < 0 {
        return Some(tile_registry.by_name("stone")); // bedrock below world
    }
    if tile_y >= world_config.height_tiles {
        return None; // above world, treat as air
    }
    let wrapped_x = world_config.wrap_tile_x(tile_x);
    let (cx, cy) = tile_to_chunk(wrapped_x, tile_y, world_config.chunk_size);
    let (lx, ly) = tile_to_local(wrapped_x, tile_y, world_config.chunk_size);
    world_map
        .chunk(cx, cy)
        .map(|chunk| chunk.bg.get(lx, ly, world_config.chunk_size))
}

/// Compute cascade count so the highest cascade's interval_end fits within
/// the padding. Each cascade N has interval_end = 4^(N+1). We keep adding
/// cascades while 4^(count+1) <= padding, ensuring rays from viewport probes
/// (which are at least `padding` tiles from the grid edge) stay in-bounds.
/// Capped at 8 cascades.
fn compute_cascade_count(padding: u32) -> u32 {
    let mut count = 1u32;
    // interval_end for cascade `count` would be 4^(count+1).
    // Keep adding cascades while the NEXT cascade's interval_end still fits.
    while 4u32.saturating_pow(count + 1) <= padding && count < 8 {
        count += 1;
    }
    count
}

/// Per-frame system: reads camera viewport and visible tiles, fills
/// density/emissive/albedo buffers for the GPU radiance cascades pipeline.
#[allow(clippy::too_many_arguments)]
fn extract_lighting_data(
    camera_query: Query<(&Camera, &Transform, &Projection), With<Camera2d>>,
    world_map: Res<WorldMap>,
    tile_registry: Res<TileRegistry>,
    world_config: Res<WorldConfig>,
    mut input: ResMut<RcInputData>,
    mut config: ResMut<RcLightingConfig>,
) {
    // Reset dirty flag; will be set true if we produce new data
    input.dirty = false;

    let Ok((camera, camera_tf, projection)) = camera_query.single() else {
        return;
    };

    // --- Viewport geometry ---
    let viewport_pixels = camera
        .physical_viewport_size()
        .unwrap_or(UVec2::new(1280, 720));
    let scale = match projection {
        Projection::Orthographic(ortho) => ortho.scale,
        _ => 1.0,
    };

    let tile_size = world_config.tile_size;
    let vp_world_w = viewport_pixels.x as f32 * scale;
    let vp_world_h = viewport_pixels.y as f32 * scale;

    // Viewport size in tiles (ceiling to cover partial tiles at edges)
    let vp_tiles_w = (vp_world_w / tile_size).ceil() as i32;
    let vp_tiles_h = (vp_world_h / tile_size).ceil() as i32;

    // Camera center in tile coordinates.
    // Read Transform (not GlobalTransform) because GlobalTransform isn't
    // propagated until PostUpdate, and this system runs in Update. The
    // shader reads view.world_position (from propagated GlobalTransform),
    // so both must agree on the camera position to avoid 1-frame lag.
    let camera_pos = camera_tf.translation.truncate();
    let (cam_tile_x, cam_tile_y) = world_to_tile(camera_pos.x, camera_pos.y, tile_size);

    // Tile range with padding, SNAPPED to the largest cascade probe spacing.
    // This ensures cascade probes always land on the same world tiles
    // regardless of camera position, eliminating view-dependent shadows.
    let cascade_count = compute_cascade_count(RC_PADDING_TILES as u32);
    let max_spacing = 1i32 << (cascade_count - 1); // 2^(n-1): 4 for 3 cascades

    let half_w = vp_tiles_w / 2;
    let half_h = vp_tiles_h / 2;

    // Snap min down to a multiple of max_spacing, then round the width UP
    // to a multiple of max_spacing. This guarantees:
    //   1. Probes land on the same world tiles regardless of camera position.
    //   2. input_w/input_h are exact multiples of every cascade's probe_spacing.
    let raw_min_tx = cam_tile_x - half_w - RC_PADDING_TILES;
    let raw_min_ty = cam_tile_y - half_h - RC_PADDING_TILES;
    let raw_w = (vp_tiles_w + 2 * RC_PADDING_TILES) as u32;
    let raw_h = (vp_tiles_h + 2 * RC_PADDING_TILES) as u32;

    // Snap min down to multiple of max_spacing (floor towards -∞)
    let min_tx = raw_min_tx - raw_min_tx.rem_euclid(max_spacing);
    let min_ty = raw_min_ty - raw_min_ty.rem_euclid(max_spacing);

    // Round width/height UP to next multiple of max_spacing
    let ms = max_spacing as u32;
    let input_w = raw_w.div_ceil(ms) * ms;
    let input_h = raw_h.div_ceil(ms) * ms;

    let max_tx = min_tx + input_w as i32 - 1;
    let max_ty = min_ty + input_h as i32 - 1;
    let total = (input_w * input_h) as usize;

    // Viewport offset: distance from input origin to viewport origin.
    // Dynamic because the snapped grid may extend further than RC_PADDING_TILES.
    let vp_offset_x = (cam_tile_x - half_w - min_tx) as u32;
    let vp_offset_y = (max_ty - cam_tile_y - half_h) as u32; // Y-flipped

    // --- Update config ---
    config.input_size = UVec2::new(input_w, input_h);
    config.viewport_size = UVec2::new(vp_tiles_w as u32, vp_tiles_h as u32);
    config.viewport_offset = UVec2::new(vp_offset_x, vp_offset_y);
    config.tile_size = tile_size;
    config.vp_world = Vec2::new(vp_world_w, vp_world_h);
    config.cascade_count = cascade_count;

    // --- Resize buffers if needed ---
    if input.width != input_w || input.height != input_h {
        input.density.resize(total, 0);
        input.emissive.resize(total, [0.0; 4]);
        input.albedo.resize(total, [0, 0, 0, 0]);
        input.width = input_w;
        input.height = input_h;
    }

    // Clear buffers
    input.density.fill(0);
    input.emissive.fill([0.0; 4]);
    input.albedo.fill([0, 0, 0, 0]);

    // --- Fill tile data ---
    for ty in min_ty..=max_ty {
        for tx in min_tx..=max_tx {
            let buf_x = (tx - min_tx) as u32;
            // GPU textures have Y=0 at top; world Y increases upward.
            // Flip so that max_ty (top of world view) maps to texel row 0.
            let buf_y = (max_ty - ty) as u32;
            let idx = (buf_y * input_w + buf_x) as usize;

            let Some(tile_id) = get_fg_tile(&world_map, tx, ty, &world_config, &tile_registry)
            else {
                // Above world or unloaded chunk — leave as 0 (air)
                continue;
            };

            // Density
            if tile_registry.is_solid(tile_id) {
                input.density[idx] = 255;
            }

            // Emissive
            let emission = tile_registry.light_emission(tile_id);
            if emission != [0, 0, 0] {
                input.emissive[idx] = [
                    emission[0] as f32 / 255.0,
                    emission[1] as f32 / 255.0,
                    emission[2] as f32 / 255.0,
                    1.0,
                ];
            }

            // Albedo
            let albedo = tile_registry.albedo(tile_id);
            input.albedo[idx] = [albedo[0], albedo[1], albedo[2], 255];
        }
    }

    // --- Sun emitters: fill sky columns from top down ---
    // For each column, scan from the top of the input texture (buf_y=0 = max_ty)
    // downward. Mark every air tile as a sun emitter until we hit the first solid
    // tile. This creates a thick emitter band that diagonal rays can reliably hit,
    // while keeping the emitter boundary at the actual terrain surface.
    for tx in min_tx..=max_tx {
        let buf_x = (tx - min_tx) as u32;
        for buf_y in 0..input_h {
            // buf_y=0 is max_ty (top of world view), buf_y increases downward
            let ty = max_ty - buf_y as i32;
            let is_sky = get_fg_tile(&world_map, tx, ty, &world_config, &tile_registry)
                .is_none_or(|id| !tile_registry.is_solid(id));
            if !is_sky {
                break; // hit terrain surface, stop filling this column
            }
            let idx = (buf_y * input_w + buf_x) as usize;
            input.emissive[idx] = [SUN_COLOR[0], SUN_COLOR[1], SUN_COLOR[2], 1.0];
        }
    }

    input.dirty = true;
}

/// Update the tile material lightmap handles to point to the current RC lightmap
/// and compute the UV correction rect that compensates for sub-tile camera offset.
///
/// Runs each frame after `swap_lightmap_handles`. The RC compute node runs
/// before camera rendering in the render graph, so the lightmap is fully
/// written before the tile fragment shader samples it.
fn update_tile_lightmap(
    gpu_images: Option<Res<rc_pipeline::RcGpuImages>>,
    config: Option<Res<RcLightingConfig>>,
    shared_material: Option<Res<SharedTileMaterial>>,
    mut materials: ResMut<Assets<TileMaterial>>,
) {
    let (Some(gpu_images), Some(config), Some(shared_material)) =
        (gpu_images, config, shared_material)
    else {
        return;
    };

    // Pass constant lightmap parameters to the shader.
    // The shader computes the actual lightmap UV from world position using
    // view.world_from_clip (guaranteed current-frame camera data) — no lag.
    let lm_params = Vec4::new(
        config.viewport_size.x as f32, // vp_tiles_w
        config.viewport_size.y as f32, // vp_tiles_h
        config.tile_size,              // tile_size
        0.0,                           // unused
    );

    for handle in [&shared_material.fg, &shared_material.bg] {
        if let Some(mat) = materials.get_mut(handle) {
            mat.lightmap = gpu_images.lightmap.clone();
            mat.lightmap_uv_rect = lm_params;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // compute_cascade_count(padding) returns the number of cascades such that
    // the highest cascade's interval_end = 4^(count+1) fits within `padding`.
    // We add cascades while 4^(count+1) <= padding.

    #[test]
    fn cascade_count_small_padding() {
        // padding=0..3: 4^2=16 > 0..3 → never enter loop → count=1
        assert_eq!(compute_cascade_count(0), 1);
        assert_eq!(compute_cascade_count(1), 1);
        assert_eq!(compute_cascade_count(3), 1);
    }

    #[test]
    fn cascade_count_padding_16() {
        // padding=16: 4^2=16 <= 16 → count=2, then 4^3=64 > 16 → stop
        assert_eq!(compute_cascade_count(16), 2);
        // padding=15: 4^2=16 > 15 → count=1
        assert_eq!(compute_cascade_count(15), 1);
    }

    #[test]
    fn cascade_count_padding_64() {
        // padding=64: 4^2=16 <=64 → 2, 4^3=64 <=64 → 3, 4^4=256 >64 → stop
        assert_eq!(compute_cascade_count(64), 3);
        // padding=63: 4^3=64 > 63 → count=2
        assert_eq!(compute_cascade_count(63), 2);
    }

    #[test]
    fn cascade_count_padding_256() {
        // padding=256: 4^2=16, 4^3=64, 4^4=256 all <= 256 → count=4
        assert_eq!(compute_cascade_count(256), 4);
    }

    #[test]
    fn cascade_count_capped_at_8() {
        // Very large padding should cap at 8
        assert_eq!(compute_cascade_count(u32::MAX), 8);
    }

    #[test]
    fn cascade_count_current_padding() {
        // RC_PADDING_TILES = 64 → should give 3 cascades
        assert_eq!(compute_cascade_count(RC_PADDING_TILES as u32), 3);
    }
}
