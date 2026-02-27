use bevy::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};

use crate::registry::tile::{TileId, TileRegistry};
use crate::registry::world::WorldConfig;
use crate::sets::GameSet;
use crate::world::chunk::{tile_to_chunk, tile_to_local, world_to_tile, WorldMap};
use crate::world::rc_pipeline;

/// Padding in tiles around the visible viewport for the RC input textures.
/// Ensures cascades have enough data beyond screen edges.
const RC_PADDING_TILES: i32 = 32;

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
}

impl Default for RcLightingConfig {
    fn default() -> Self {
        Self {
            input_size: UVec2::ZERO,
            viewport_size: UVec2::ZERO,
            viewport_offset: UVec2::ZERO,
            tile_size: 32.0,
            cascade_count: 1,
        }
    }
}

/// CPU-side buffers holding per-tile density, emissive, and albedo data
/// extracted each frame for GPU upload.
#[derive(Resource, Clone, ExtractResource)]
pub struct RcInputData {
    /// 0 = air, 255 = solid. One byte per tile.
    pub density: Vec<u8>,
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

impl Default for RcInputData {
    fn default() -> Self {
        Self {
            density: Vec::new(),
            emissive: Vec::new(),
            albedo: Vec::new(),
            width: 0,
            height: 0,
            dirty: false,
        }
    }
}

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
                    extract_lighting_data.in_set(GameSet::WorldUpdate),
                    rc_pipeline::resize_gpu_textures
                        .after(extract_lighting_data)
                        .in_set(GameSet::WorldUpdate),
                    rc_pipeline::swap_lightmap_handles
                        .after(rc_pipeline::resize_gpu_textures)
                        .in_set(GameSet::WorldUpdate),
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

/// Compute how many cascade levels are needed for the given maximum dimension.
/// Each cascade covers 4× the area of the previous one, starting at size 4.
/// Capped at 8 cascades.
fn compute_cascade_count(max_dim: u32) -> u32 {
    let mut count = 1u32;
    let mut size = 4u32;
    while size < max_dim && count < 8 {
        size = size.saturating_mul(4);
        count += 1;
    }
    count
}

/// Per-frame system: reads camera viewport and visible tiles, fills
/// density/emissive/albedo buffers for the GPU radiance cascades pipeline.
#[allow(clippy::too_many_arguments)]
fn extract_lighting_data(
    camera_query: Query<(&Camera, &GlobalTransform, &Projection), With<Camera2d>>,
    world_map: Res<WorldMap>,
    tile_registry: Res<TileRegistry>,
    world_config: Res<WorldConfig>,
    mut input: ResMut<RcInputData>,
    mut config: ResMut<RcLightingConfig>,
) {
    // Reset dirty flag; will be set true if we produce new data
    input.dirty = false;

    let Ok((camera, camera_gt, projection)) = camera_query.single() else {
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

    // Camera center in tile coordinates
    let camera_pos = camera_gt.translation().truncate();
    let (cam_tile_x, cam_tile_y) = world_to_tile(camera_pos.x, camera_pos.y, tile_size);

    // Tile range with padding
    let half_w = vp_tiles_w / 2;
    let half_h = vp_tiles_h / 2;
    let min_tx = cam_tile_x - half_w - RC_PADDING_TILES;
    let max_tx = cam_tile_x + half_w + RC_PADDING_TILES;
    let min_ty = cam_tile_y - half_h - RC_PADDING_TILES;
    let max_ty = cam_tile_y + half_h + RC_PADDING_TILES;

    let input_w = (max_tx - min_tx + 1) as u32;
    let input_h = (max_ty - min_ty + 1) as u32;
    let total = (input_w * input_h) as usize;

    // --- Update config ---
    config.input_size = UVec2::new(input_w, input_h);
    config.viewport_size = UVec2::new(vp_tiles_w as u32, vp_tiles_h as u32);
    config.viewport_offset = UVec2::new(RC_PADDING_TILES as u32, RC_PADDING_TILES as u32);
    config.tile_size = tile_size;
    config.cascade_count = compute_cascade_count(input_w.max(input_h));

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
            let buf_y = (ty - min_ty) as u32;
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

    // --- Sun emitters along top row ---
    // Place sun light on the topmost row of the input range where sky is visible
    // (tile_y < 0 or tile is air and above the world surface)
    let sun_ty = min_ty;
    let sun_buf_y = 0u32;
    for tx in min_tx..=max_tx {
        let buf_x = (tx - min_tx) as u32;
        let idx = (sun_buf_y * input_w + buf_x) as usize;

        // Only emit sun where there's no solid tile (sky)
        let is_sky = get_fg_tile(&world_map, tx, sun_ty, &world_config, &tile_registry)
            .map_or(true, |id| !tile_registry.is_solid(id));

        if is_sky {
            input.emissive[idx] = [SUN_COLOR[0], SUN_COLOR[1], SUN_COLOR[2], 1.0];
        }
    }

    input.dirty = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cascade_count_small() {
        // max_dim=4 → size starts at 4, already >= 4, so count=1
        assert_eq!(compute_cascade_count(4), 1);
    }

    #[test]
    fn cascade_count_medium() {
        // max_dim=16 → size=4 (count=1), 16 (count=2) → 16 >= 16 → 2
        assert_eq!(compute_cascade_count(16), 2);
        // max_dim=17 → size=4,16,64 → count=3
        assert_eq!(compute_cascade_count(17), 3);
    }

    #[test]
    fn cascade_count_large() {
        // max_dim=64 → 4,16,64 → count=3
        assert_eq!(compute_cascade_count(64), 3);
        // max_dim=256 → 4,16,64,256 → count=4
        assert_eq!(compute_cascade_count(256), 4);
    }

    #[test]
    fn cascade_count_capped_at_8() {
        // Very large dimension should cap at 8
        assert_eq!(compute_cascade_count(1_000_000), 8);
    }

    #[test]
    fn cascade_count_zero_and_one() {
        // Edge cases: 0 and 1 should return 1 (size=4 already >= them)
        assert_eq!(compute_cascade_count(0), 1);
        assert_eq!(compute_cascade_count(1), 1);
    }

    #[test]
    fn cascade_count_boundary_values() {
        // 4^1=4, 4^2=16, 4^3=64, 4^4=256, 4^5=1024, 4^6=4096, 4^7=16384, 4^8=65536
        assert_eq!(compute_cascade_count(5), 2); // needs > 4
        assert_eq!(compute_cascade_count(65), 4); // needs > 64
        assert_eq!(compute_cascade_count(1024), 5);
        assert_eq!(compute_cascade_count(1025), 6);
        assert_eq!(compute_cascade_count(4096), 6);
        assert_eq!(compute_cascade_count(4097), 7);
        assert_eq!(compute_cascade_count(16384), 7);
        assert_eq!(compute_cascade_count(16385), 8);
    }
}
