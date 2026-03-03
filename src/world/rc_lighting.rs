use bevy::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::tasks::ComputeTaskPool;

use crate::fluid::cell::FluidId;
use crate::fluid::registry::FluidRegistry;
use crate::object::definition::ObjectId;
use crate::object::registry::ObjectRegistry;
use crate::registry::tile::{TileId, TileRegistry};
use crate::registry::AppState;
use crate::sets::GameSet;
use crate::world::chunk::{world_to_tile, WorldMap};
use crate::world::ctx::WorldCtx;
use crate::world::lit_sprite::LitSpriteMaterial;
use crate::world::rc_pipeline;
use crate::world::tile_renderer::{SharedTileMaterial, TileMaterial};

/// Padding in tiles around the visible viewport for the RC input textures.
/// Must be >= interval_end of the highest useful cascade so that rays from
/// viewport probes don't escape the grid. With 3 cascades the max ray
/// distance is 4^3 = 64, so padding = 64 keeps all viewport rays in-bounds.
const RC_PADDING_TILES: i32 = 64;

/// Warm-white sun color used for sky emitters along the top row.
const SUN_COLOR: [f32; 3] = [1.0, 0.98, 0.90];

/// HDR multiplier for tile-based point lights (torches, lava, etc.).
/// Point sources occupy a single tile, so RC probe rays hit them from
/// far fewer directions than area emitters like the sky band. This boost
/// compensates for the small angular coverage so torches look bright.
const POINT_LIGHT_BOOST: f32 = 4.0;

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
    /// World-space origin of the input grid (min_tx, min_ty).
    /// Passed to the shader so angular jitter can use stable world coordinates.
    pub grid_origin: IVec2,
    /// Previous frame's grid origin, for computing bounce light offset.
    pub prev_grid_origin: IVec2,
    /// Bounce offset in buffer space: how to shift sample_px when reading
    /// lightmap_prev (which was written with prev_grid_origin).
    /// Computed as (dx, -dy) where d = grid_origin - prev_grid_origin.
    pub bounce_offset: IVec2,
    /// Dynamic sun color from day/night cycle.
    pub sun_color: Vec3,
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
            grid_origin: IVec2::ZERO,
            prev_grid_origin: IVec2::ZERO,
            bounce_offset: IVec2::ZERO,
            sun_color: Vec3::new(1.0, 0.98, 0.9),
        }
    }
}

/// CPU-side buffers holding per-tile density, emissive, and albedo data
/// extracted each frame for GPU upload.
#[derive(Resource, Clone, Default, ExtractResource)]
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

// `Default` derived: all Vecs empty, numerics 0, dirty false.

/// Dirty flag: set `true` whenever tiles are modified (block_action, worldgen, etc.)
/// so that the next `extract_lighting_data` rebuilds density/albedo/flat grids.
#[derive(Resource, Default)]
pub struct RcGridDirty(pub bool);

/// Cached flat tile grids for the RC lighting system.
/// Stored in `Local<RcCachedGrid>` to persist between frames without
/// re-extracting tiles from the chunk `HashMap` every frame.
#[derive(Default)]
struct RcCachedGrid {
    fg: Vec<TileId>,
    bg: Vec<TileId>,
    origin: IVec2,
    size: UVec2,
}

/// Reset RC lighting state to defaults.
///
/// Registered on `OnEnter(LoadingBiomes)` to ensure that any stale data
/// written by `extract_lighting_data` on the warp frame (race condition:
/// extract may run after `handle_warp` in the same Update) is zeroed before
/// `ExtractResource` copies it to the render world. Without this, the GPU
/// compute node would keep dispatching with old-planet data during loading.
fn reset_rc_on_loading(
    mut config: ResMut<RcLightingConfig>,
    mut input: ResMut<RcInputData>,
    mut rc_dirty: ResMut<RcGridDirty>,
) {
    *config = RcLightingConfig::default();
    *input = RcInputData::default();
    rc_dirty.0 = true; // Force grid rebuild on next frame
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
            .init_resource::<RcGridDirty>()
            .insert_resource(gpu_images)
            .add_plugins((
                ExtractResourcePlugin::<RcLightingConfig>::default(),
                ExtractResourcePlugin::<RcInputData>::default(),
                ExtractResourcePlugin::<rc_pipeline::RcGpuImages>::default(),
            ))
            // Definitive RC state reset: fires before the first Update of the
            // loading phase, guaranteeing the render world sees zeroed config.
            .add_systems(OnEnter(AppState::LoadingBiomes), reset_rc_on_loading)
            .add_systems(
                Update,
                (
                    // Lighting runs AFTER Camera so it sees the current frame's
                    // camera position, not the previous frame's. This prevents
                    // the lightmap from being misaligned with the rendered tiles.
                    //
                    // ALL four systems are gated on InGame to prevent stale data
                    // from corrupting lightmaps during loading after a warp.
                    extract_lighting_data
                        .after(GameSet::Camera)
                        .run_if(in_state(AppState::InGame)),
                    rc_pipeline::resize_gpu_textures
                        .after(extract_lighting_data)
                        .after(GameSet::Camera)
                        .run_if(in_state(AppState::InGame)),
                    rc_pipeline::swap_lightmap_handles
                        .after(rc_pipeline::resize_gpu_textures)
                        .after(GameSet::Camera)
                        .run_if(in_state(AppState::InGame)),
                    update_tile_lightmap
                        .after(rc_pipeline::swap_lightmap_handles)
                        .after(GameSet::Camera)
                        .run_if(in_state(AppState::InGame)),
                ),
            );

        // Set up the render-side pipeline (render app systems + graph node).
        rc_pipeline::setup_render_pipeline(app);
    }
}

/// Count how many of the 4 cardinal neighbors are "open" (both FG and BG air)
/// using direct array indexing into the flat tile grids.
/// Out-of-bounds neighbors (grid edges in padding zone) are treated as open.
fn count_open_neighbors_grid(
    bx: usize,
    by: usize,
    w: usize,
    h: usize,
    fg: &[TileId],
    bg: &[TileId],
    tile_reg: &TileRegistry,
) -> u32 {
    let mut count = 0u32;
    for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
        let nx = bx as i32 + dx;
        let ny = by as i32 + dy;
        if nx < 0 || nx >= w as i32 || ny < 0 || ny >= h as i32 {
            count += 1; // out of grid bounds → treat as open
            continue;
        }
        let nidx = ny as usize * w + nx as usize;
        let fg_air = !tile_reg.is_solid(fg[nidx]);
        let bg_air = !tile_reg.is_solid(bg[nidx]);
        if fg_air && bg_air {
            count += 1;
        }
    }
    count
}

/// Deterministic hash of a tile position for per-tile flicker phase.
/// Uses a simple mixing function — quality doesn't need to be cryptographic,
/// just enough that adjacent tiles get visually different phases.
fn tile_phase(tx: i32, ty: i32) -> f32 {
    let mut h = (tx as u32).wrapping_mul(73856093) ^ (ty as u32).wrapping_mul(19349663);
    h = h.wrapping_mul(0x45d9f3b).wrapping_add(0x238e1f29);
    h ^= h >> 16;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Compute flicker brightness multiplier for an emissive tile.
/// Returns a value in `[flicker_min, flicker_min + flicker_strength]` based on
/// three summed sine harmonics keyed by tile position and elapsed time.
fn flicker_multiplier(tx: i32, ty: i32, elapsed: f32, speed: f32, strength: f32, min: f32) -> f32 {
    if speed <= 0.0 || strength <= 0.0 {
        return 1.0;
    }
    let phase = tile_phase(tx, ty) * std::f32::consts::TAU;
    let t = elapsed * speed + phase;
    // Three harmonics for organic feel
    let noise = t.sin() * 0.5 + (t * 2.3).sin() * 0.3 + (t * 4.1).sin() * 0.2;
    let normalized = noise * 0.5 + 0.5; // [0, 1]
    min + normalized * strength
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
///
/// **Optimizations over the naive per-tile approach:**
/// 1. Flat `Vec<TileId>` grids built by iterating chunks (1 HashMap lookup
///    per chunk, row-wise `copy_from_slice`) instead of ~600K per-tile lookups.
/// 2. Density/albedo only rebuilt when the grid moves or tiles change
///    (`RcGridDirty`); cached flat grids persist in `Local<RcCachedGrid>`.
/// 3. Fast-paths: sky tiles (`ty >= height`) → full sun row; bedrock
///    (`ty < 0`) → skip emissive entirely.
/// 4. `count_open_neighbors_grid` uses 4 array reads instead of 8 HashMap
///    lookups.
#[allow(clippy::too_many_arguments)]
fn extract_lighting_data(
    camera_query: Query<(&Camera, &Transform, &Projection), With<Camera2d>>,
    mut world_map: ResMut<WorldMap>,
    ctx: WorldCtx,
    mut input: ResMut<RcInputData>,
    mut config: ResMut<RcLightingConfig>,
    world_time: Option<Res<crate::world::day_night::WorldTime>>,
    time: Res<Time>,
    object_registry: Option<Res<ObjectRegistry>>,
    fluid_registry: Option<Res<FluidRegistry>>,
    mut rc_dirty: ResMut<RcGridDirty>,
    mut cache: Local<RcCachedGrid>,
) {
    let world_config = &*ctx.config;
    let tile_registry = &*ctx.tile_registry;
    let height_tiles = world_config.height_tiles;
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
    let new_grid_origin = IVec2::new(min_tx, min_ty);

    // Bounce offset: correct lightmap_prev reads when grid origin changes.
    // In buffer X: old_buf_x = new_buf_x + dx (buf_x = tx - min_tx)
    // In buffer Y: old_buf_y = new_buf_y - dy (buf_y = max_ty - ty, Y-flipped)
    let d = new_grid_origin - config.prev_grid_origin;
    config.bounce_offset = IVec2::new(d.x, -d.y);
    config.prev_grid_origin = new_grid_origin;

    config.input_size = UVec2::new(input_w, input_h);
    config.viewport_size = UVec2::new(vp_tiles_w as u32, vp_tiles_h as u32);
    config.viewport_offset = UVec2::new(vp_offset_x, vp_offset_y);
    config.tile_size = tile_size;
    config.cascade_count = cascade_count;
    config.grid_origin = new_grid_origin;

    // --- Resize buffers if needed ---
    if input.width != input_w || input.height != input_h {
        input.density.resize(total, 0);
        input.emissive.resize(total, [0.0; 4]);
        input.albedo.resize(total, [0, 0, 0, 0]);
        input.width = input_w;
        input.height = input_h;
    }

    // --- Pre-generate chunk data for the RC grid ---
    // Ensures tile lookups never return None for in-bounds tiles, so unloaded
    // chunks at the edge of the lighting radius have correct tile data instead
    // of being incorrectly treated as sky emitters.
    {
        let ctx_ref = ctx.as_ref();
        let cs = world_config.chunk_size as i32;
        let clamp_min_ty = min_ty.max(0);
        let clamp_max_ty = max_ty.min(height_tiles - 1);
        if clamp_min_ty <= clamp_max_ty {
            let gen_min_cy = clamp_min_ty.div_euclid(cs);
            let gen_max_cy = clamp_max_ty.div_euclid(cs);
            let gen_min_cx = min_tx.div_euclid(cs);
            let gen_max_cx = max_tx.div_euclid(cs);
            for cy in gen_min_cy..=gen_max_cy {
                for cx in gen_min_cx..=gen_max_cx {
                    let data_cx = world_config.wrap_chunk_x(cx);
                    world_map.get_or_generate_chunk(data_cx, cy, &ctx_ref);
                }
            }
        }
    }

    // --- Determine whether to rebuild flat grids + density/albedo ---
    let new_size = UVec2::new(input_w, input_h);
    let need_rebuild = new_grid_origin != cache.origin || new_size != cache.size || rc_dirty.0;

    // --- Rebuild flat tile grids + density/albedo when needed ---
    // Instead of ~63K×2 HashMap lookups (get_fg_tile + get_bg_tile per tile),
    // iterate ~70 chunks with row-wise copy_from_slice (~2K memcpy calls).
    if need_rebuild {
        let cs = world_config.chunk_size as i32;
        let cs_usize = world_config.chunk_size as usize;
        let w_usize = input_w as usize;
        let stone = tile_registry.by_name("stone");

        // Resize and default to AIR (sky tiles above world stay AIR)
        cache.fg.resize(total, TileId::AIR);
        cache.bg.resize(total, TileId::AIR);
        cache.fg.fill(TileId::AIR);
        cache.bg.fill(TileId::AIR);

        // Fill bedrock rows (ty < 0) with stone
        for ty in min_ty..0_i32.min(max_ty + 1) {
            let buf_y = (max_ty - ty) as usize;
            let row_start = buf_y * w_usize;
            cache.fg[row_start..row_start + w_usize].fill(stone);
            cache.bg[row_start..row_start + w_usize].fill(stone);
        }

        // Fill from chunks using row-wise copy_from_slice
        let clamp_min_ty = min_ty.max(0);
        let clamp_max_ty = max_ty.min(height_tiles - 1);
        if clamp_min_ty <= clamp_max_ty {
            let grid_min_cy = clamp_min_ty.div_euclid(cs);
            let grid_max_cy = clamp_max_ty.div_euclid(cs);
            let grid_min_cx = min_tx.div_euclid(cs);
            let grid_max_cx = max_tx.div_euclid(cs);

            for cy in grid_min_cy..=grid_max_cy {
                for cx in grid_min_cx..=grid_max_cx {
                    let data_cx = world_config.wrap_chunk_x(cx);
                    let Some(chunk) = world_map.chunk(data_cx, cy) else {
                        continue;
                    };

                    let chunk_tx0 = cx * cs;
                    let chunk_ty0 = cy * cs;

                    // Intersect chunk tile range with RC grid and valid world Y
                    let tx0 = chunk_tx0.max(min_tx);
                    let tx1 = (chunk_tx0 + cs).min(max_tx + 1);
                    let ty0 = chunk_ty0.max(clamp_min_ty);
                    let ty1 = (chunk_ty0 + cs).min(clamp_max_ty + 1);

                    if tx0 >= tx1 || ty0 >= ty1 {
                        continue;
                    }

                    let lx0 = (tx0 - chunk_tx0) as usize;
                    let row_len = (tx1 - tx0) as usize;
                    let buf_x0 = (tx0 - min_tx) as usize;

                    for ty in ty0..ty1 {
                        let ly = (ty - chunk_ty0) as usize;
                        let buf_y = (max_ty - ty) as usize;

                        let src_start = ly * cs_usize + lx0;
                        let dst_start = buf_y * w_usize + buf_x0;

                        cache.fg[dst_start..dst_start + row_len]
                            .copy_from_slice(&chunk.fg.tiles[src_start..src_start + row_len]);
                        cache.bg[dst_start..dst_start + row_len]
                            .copy_from_slice(&chunk.bg.tiles[src_start..src_start + row_len]);
                    }
                }
            }
        }

        // Rebuild density + albedo from flat grids (single pass, no fill).
        // Every element is written — solid tiles get opacity/albedo,
        // air tiles get explicit zeros.
        for idx in 0..total {
            let fg_id = cache.fg[idx];
            if tile_registry.is_solid(fg_id) {
                let opacity = tile_registry.light_opacity(fg_id);
                input.density[idx] = (opacity as f32 / 15.0 * 255.0) as u8;
                let albedo = tile_registry.albedo(fg_id);
                input.albedo[idx] = [albedo[0], albedo[1], albedo[2], 255];
            } else {
                input.density[idx] = 0;
                input.albedo[idx] = [0, 0, 0, 0];
            }
        }

        cache.origin = new_grid_origin;
        cache.size = new_size;
    }

    // --- Overlay fluid density on top of tile density (every frame) ---
    // Fluids move each tick so this cannot be cached inside `need_rebuild`.
    // Only increases density for air tiles (fluids don't make solid tiles darker).
    if let Some(ref fluid_reg) = fluid_registry {
        let cs = world_config.chunk_size as i32;
        let cs_u = world_config.chunk_size;
        let clamp_min_ty = min_ty.max(0);
        let clamp_max_ty = max_ty.min(height_tiles - 1);
        if clamp_min_ty <= clamp_max_ty {
            let fl_min_cy = clamp_min_ty.div_euclid(cs);
            let fl_max_cy = clamp_max_ty.div_euclid(cs);
            let fl_min_cx = min_tx.div_euclid(cs);
            let fl_max_cx = max_tx.div_euclid(cs);

            for cy in fl_min_cy..=fl_max_cy {
                for cx in fl_min_cx..=fl_max_cx {
                    let data_cx = world_config.wrap_chunk_x(cx);
                    let Some(chunk) = world_map.chunk(data_cx, cy) else {
                        continue;
                    };

                    let chunk_tx0 = cx * cs;
                    let chunk_ty0 = cy * cs;
                    let tx0 = chunk_tx0.max(min_tx);
                    let tx1 = (chunk_tx0 + cs).min(max_tx + 1);
                    let ty0 = chunk_ty0.max(clamp_min_ty);
                    let ty1 = (chunk_ty0 + cs).min(clamp_max_ty + 1);

                    for ty in ty0..ty1 {
                        let ly = (ty - chunk_ty0) as u32;
                        for tx in tx0..tx1 {
                            let lx = (tx - chunk_tx0) as u32;
                            let fidx = (ly * cs_u + lx) as usize;
                            let cell = chunk.fluids[fidx];
                            if cell.is_empty() {
                                continue;
                            }
                            let def = fluid_reg.get(cell.fluid_id);
                            if def.light_absorption <= 0.0 {
                                continue;
                            }

                            let buf_x = (tx - min_tx) as u32;
                            let buf_y = (max_ty - ty) as u32;
                            let idx = (buf_y * input_w + buf_x) as usize;

                            // Skip solid tiles — fluids don't make them more opaque
                            if input.density[idx] >= 255 {
                                continue;
                            }

                            let absorption =
                                cell.mass.min(1.0) * def.light_absorption;
                            let fluid_density = (absorption * 255.0) as u8;
                            input.density[idx] = input.density[idx].max(fluid_density);
                        }
                    }
                }
            }
        }
    }

    // --- Compute effective sun color from day/night cycle ---
    // Bake ambient_min into sun emitters: each channel is at least ambient_min.
    // This ensures sky-visible tiles always emit some light (even at night),
    // while underground tiles (no sky access) stay pitch black.
    let (sun, ambient_min) = if let Some(ref wt) = world_time {
        let amb = wt.ambient_min;
        (
            [
                (wt.sun_color.x * wt.sun_intensity).max(amb),
                (wt.sun_color.y * wt.sun_intensity).max(amb),
                (wt.sun_color.z * wt.sun_intensity).max(amb),
            ],
            amb,
        )
    } else {
        (SUN_COLOR, 0.0)
    };

    // --- Rebuild emissive every frame (parallel across CPU cores) ---
    // Split into horizontal strips, one per thread. Each strip writes only
    // to its own slice of the emissive buffer — no synchronization needed.
    input.emissive.fill([0.0; 4]);
    let w_usize = input_w as usize;
    let h_usize = input_h as usize;
    let elapsed = time.elapsed_secs();

    {
        let pool = ComputeTaskPool::get();
        let num_strips = (pool.thread_num() + 1).max(1);
        let rows_per_strip = h_usize.div_ceil(num_strips);
        let fg = cache.fg.as_slice();
        let bg = cache.bg.as_slice();
        let tr = tile_registry;

        let emissive = input.emissive.as_mut_slice();
        pool.scope(|s| {
            for (strip_idx, strip) in emissive
                .chunks_mut(rows_per_strip * w_usize)
                .enumerate()
            {
                s.spawn(async move {
                    let strip_start = strip_idx * rows_per_strip;
                    let strip_rows = strip.len() / w_usize;

                    for local_row in 0..strip_rows {
                        let buf_y = strip_start + local_row;
                        let ty = max_ty - buf_y as i32;
                        let row_start = local_row * w_usize;

                        // Fast path: sky above world — full sun row
                        if ty >= height_tiles {
                            for i in 0..w_usize {
                                strip[row_start + i] = [sun[0], sun[1], sun[2], 1.0];
                            }
                            continue;
                        }

                        // Fast path: bedrock below world — stays zeroed
                        if ty < 0 {
                            continue;
                        }

                        // In-world tiles: sun emitters + tile-specific emissive
                        for buf_x in 0..w_usize {
                            let local_idx = row_start + buf_x;
                            let global_idx = buf_y * w_usize + buf_x;
                            let fg_id = fg[global_idx];

                            if tr.is_solid(fg_id) {
                                // Tile emissive (torches, lava, etc.)
                                let emission = tr.light_emission(fg_id);
                                if emission != [0, 0, 0] {
                                    let tx = min_tx + buf_x as i32;
                                    let def = tr.get(fg_id);
                                    let flicker = flicker_multiplier(
                                        tx,
                                        ty,
                                        elapsed,
                                        def.flicker_speed,
                                        def.flicker_strength,
                                        def.flicker_min,
                                    );
                                    strip[local_idx] = [
                                        emission[0] as f32 / 255.0 * POINT_LIGHT_BOOST * flicker,
                                        emission[1] as f32 / 255.0 * POINT_LIGHT_BOOST * flicker,
                                        emission[2] as f32 / 255.0 * POINT_LIGHT_BOOST * flicker,
                                        1.0,
                                    ];
                                }
                            } else {
                                // FG is air: sun emitter if BG is also air
                                let bg_id = bg[global_idx];
                                let bg_air = !tr.is_solid(bg_id);
                                if bg_air {
                                    let open = count_open_neighbors_grid(
                                        buf_x, buf_y, w_usize, h_usize, fg, bg, tr,
                                    );
                                    let intensity = (1 + open) as f32 / 5.0;
                                    strip[local_idx] = [
                                        sun[0] * intensity,
                                        sun[1] * intensity,
                                        sun[2] * intensity,
                                        1.0,
                                    ];
                                }
                            }
                        }
                    }
                });
            }
        });
    }

    // --- Object emissive (iterate by chunk, not per-tile HashMap) ---
    // Checked after tile emission so an object light on an air tile
    // fills the emissive slot that tile emission left at zero.
    if let Some(ref obj_reg) = object_registry {
        let cs = world_config.chunk_size as i32;
        let cs_u = world_config.chunk_size;
        let clamp_min_ty = min_ty.max(0);
        let clamp_max_ty = max_ty.min(height_tiles - 1);
        if clamp_min_ty <= clamp_max_ty {
            let obj_min_cy = clamp_min_ty.div_euclid(cs);
            let obj_max_cy = clamp_max_ty.div_euclid(cs);
            let obj_min_cx = min_tx.div_euclid(cs);
            let obj_max_cx = max_tx.div_euclid(cs);

            for cy in obj_min_cy..=obj_max_cy {
                for cx in obj_min_cx..=obj_max_cx {
                    let data_cx = world_config.wrap_chunk_x(cx);
                    let Some(chunk) = world_map.chunk(data_cx, cy) else {
                        continue;
                    };

                    let chunk_tx0 = cx * cs;
                    let chunk_ty0 = cy * cs;
                    let tx0 = chunk_tx0.max(min_tx);
                    let tx1 = (chunk_tx0 + cs).min(max_tx + 1);
                    let ty0 = chunk_ty0.max(clamp_min_ty);
                    let ty1 = (chunk_ty0 + cs).min(clamp_max_ty + 1);

                    for ty in ty0..ty1 {
                        let ly = (ty - chunk_ty0) as u32;
                        for tx in tx0..tx1 {
                            let lx = (tx - chunk_tx0) as u32;
                            let occ_idx = (ly * cs_u + lx) as usize;
                            let Some(occ) = &chunk.occupancy[occ_idx] else {
                                continue;
                            };

                            let (dcx, dcy) = occ.data_chunk;
                            let Some(data_chunk) = world_map.chunk(dcx, dcy) else {
                                continue;
                            };
                            let Some(obj) = data_chunk.objects.get(occ.object_index as usize)
                            else {
                                continue;
                            };
                            if obj.object_id == ObjectId::NONE {
                                continue;
                            }
                            let Some(def) = obj_reg.try_get(obj.object_id) else {
                                continue;
                            };
                            let oe = def.light_emission;
                            if oe == [0, 0, 0] {
                                continue;
                            }

                            let buf_x = (tx - min_tx) as u32;
                            let buf_y = (max_ty - ty) as u32;
                            let idx = (buf_y * input_w + buf_x) as usize;

                            let flicker = flicker_multiplier(
                                tx,
                                ty,
                                elapsed,
                                def.flicker_speed,
                                def.flicker_strength,
                                def.flicker_min,
                            );
                            input.emissive[idx] = [
                                oe[0] as f32 / 255.0 * POINT_LIGHT_BOOST * flicker,
                                oe[1] as f32 / 255.0 * POINT_LIGHT_BOOST * flicker,
                                oe[2] as f32 / 255.0 * POINT_LIGHT_BOOST * flicker,
                                1.0,
                            ];
                        }
                    }
                }
            }
        }
    }

    // --- Fluid emissive (lava, etc.) ---
    // Emissive fluids inject light directly into the RC emissive buffer.
    // Only overwrites if the new emission is brighter than what's already there.
    if let Some(ref fluid_reg) = fluid_registry {
        let cs = world_config.chunk_size as i32;
        let cs_u = world_config.chunk_size;
        let clamp_min_ty = min_ty.max(0);
        let clamp_max_ty = max_ty.min(height_tiles - 1);
        if clamp_min_ty <= clamp_max_ty {
            let fl_min_cy = clamp_min_ty.div_euclid(cs);
            let fl_max_cy = clamp_max_ty.div_euclid(cs);
            let fl_min_cx = min_tx.div_euclid(cs);
            let fl_max_cx = max_tx.div_euclid(cs);

            for cy in fl_min_cy..=fl_max_cy {
                for cx in fl_min_cx..=fl_max_cx {
                    let data_cx = world_config.wrap_chunk_x(cx);
                    let Some(chunk) = world_map.chunk(data_cx, cy) else {
                        continue;
                    };

                    let chunk_tx0 = cx * cs;
                    let chunk_ty0 = cy * cs;
                    let tx0 = chunk_tx0.max(min_tx);
                    let tx1 = (chunk_tx0 + cs).min(max_tx + 1);
                    let ty0 = chunk_ty0.max(clamp_min_ty);
                    let ty1 = (chunk_ty0 + cs).min(clamp_max_ty + 1);

                    // Pre-compute per-cell emission coverage for this chunk.
                    // A cell is "covered" when a contiguous column of fluid
                    // above it contains a different fluid type (e.g. water
                    // over lava).  Covered emissive cells are skipped so
                    // their light doesn't bleed into the lightmap through
                    // the covering fluid.
                    let total = (cs_u * cs_u) as usize;
                    let mut emission_covered = vec![false; total];
                    {
                        // Seed top_fluid from the chunk above (cross-boundary).
                        let above_chunk = world_map.chunk(
                            world_config.wrap_chunk_x(cx),
                            cy + 1,
                        );
                        for lx in 0..cs_u {
                            let mut top_fluid = above_chunk
                                .map(|c| {
                                    let aidx = lx as usize; // bottom row of chunk above = ly=0
                                    c.fluids[aidx].fluid_id
                                })
                                .filter(|id| *id != FluidId::NONE)
                                .unwrap_or(FluidId::NONE);
                            let mut cover_active = false;

                            for ly in (0..cs_u).rev() {
                                let cidx = (ly * cs_u + lx) as usize;
                                let cell = chunk.fluids[cidx];
                                if cell.is_empty() {
                                    top_fluid = FluidId::NONE;
                                    cover_active = false;
                                } else {
                                    if top_fluid == FluidId::NONE {
                                        top_fluid = cell.fluid_id;
                                    } else if cell.fluid_id != top_fluid {
                                        cover_active = true;
                                    }
                                    if cover_active {
                                        emission_covered[cidx] = true;
                                    }
                                }
                            }
                        }
                    }

                    for ty in ty0..ty1 {
                        let ly = (ty - chunk_ty0) as u32;
                        for tx in tx0..tx1 {
                            let lx = (tx - chunk_tx0) as u32;
                            let fidx = (ly * cs_u + lx) as usize;
                            let cell = chunk.fluids[fidx];
                            if cell.is_empty() {
                                continue;
                            }
                            let def = fluid_reg.get(cell.fluid_id);
                            if def.light_emission == [0, 0, 0] {
                                continue;
                            }
                            if cell.mass < 0.1 {
                                continue;
                            }
                            // Skip emission for cells covered by a different
                            // fluid above (e.g. lava under water).
                            if emission_covered[fidx] {
                                continue;
                            }

                            let buf_x = (tx - min_tx) as u32;
                            let buf_y = (max_ty - ty) as u32;
                            let idx = (buf_y * input_w + buf_x) as usize;
                            let intensity = cell.mass.min(1.0);
                            let e = def.light_emission;
                            let new_emission = [
                                e[0] as f32 / 255.0 * POINT_LIGHT_BOOST * intensity,
                                e[1] as f32 / 255.0 * POINT_LIGHT_BOOST * intensity,
                                e[2] as f32 / 255.0 * POINT_LIGHT_BOOST * intensity,
                                1.0,
                            ];
                            let existing = input.emissive[idx];
                            if new_emission[0] > existing[0]
                                || new_emission[1] > existing[1]
                                || new_emission[2] > existing[2]
                            {
                                input.emissive[idx] = new_emission;
                            }
                        }
                    }
                }
            }
        }
    }

    rc_dirty.0 = false;
    input.dirty = true;

    // Update config with day/night values for the GPU pipeline.
    // Bake ambient_min into sun_color so sky escape in radiance_cascades.wgsl
    // also returns at least ambient_min per channel.
    if let Some(ref wt) = world_time {
        let base = wt.sun_color * wt.sun_intensity;
        config.sun_color = base.max(Vec3::splat(ambient_min));
    }
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
    mut tile_materials: ResMut<Assets<TileMaterial>>,
    mut lit_sprite_materials: ResMut<Assets<LitSpriteMaterial>>,
) {
    let (Some(gpu_images), Some(config), Some(shared_material)) =
        (gpu_images, config, shared_material)
    else {
        return;
    };

    // Pre-compute affine transform: world_pos → lightmap UV.
    // lightmap_uv = world_pos * scale + offset
    // Lightmap is input-sized, covering the full RC grid in world-space.
    // This transform is stable (changes only on grid snap, not every frame).
    let ts = config.tile_size;
    let iw = config.input_size.x as f32;
    let ih = config.input_size.y as f32;
    if iw == 0.0 || ih == 0.0 {
        return;
    }
    let gx = config.grid_origin.x as f32;
    let gy = config.grid_origin.y as f32;

    let lm_params = Vec4::new(
        1.0 / (ts * iw),  // scale_x
        -1.0 / (ts * ih), // scale_y (negated: world Y up, texel Y down)
        -gx / iw,         // offset_x
        1.0 + gy / ih,    // offset_y
    );

    // Update tile materials (shared FG/BG handles)
    for handle in [&shared_material.fg, &shared_material.bg] {
        if let Some(mat) = tile_materials.get_mut(handle) {
            mat.lightmap = gpu_images.lightmap.clone();
            mat.lightmap_uv_rect = lm_params;
        }
    }

    // Update ALL lit sprite materials (player, dropped items, etc.)
    // with the current lightmap and UV transform.
    for (_id, mat) in lit_sprite_materials.iter_mut() {
        mat.lightmap = gpu_images.lightmap.clone();
        mat.lightmap_uv_rect = lm_params;
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

    #[test]
    fn tile_phase_deterministic() {
        // Same position always gives same phase
        assert_eq!(tile_phase(10, 20), tile_phase(10, 20));
    }

    #[test]
    fn tile_phase_varies_by_position() {
        // Different positions give different phases
        assert_ne!(tile_phase(0, 0), tile_phase(1, 0));
        assert_ne!(tile_phase(0, 0), tile_phase(0, 1));
    }

    #[test]
    fn tile_phase_in_unit_range() {
        for x in -10..10 {
            for y in -10..10 {
                let p = tile_phase(x, y);
                assert!(p >= 0.0 && p <= 1.0, "phase({x},{y}) = {p}");
            }
        }
    }

    #[test]
    fn flicker_no_flicker_returns_one() {
        // speed=0 or strength=0 → multiplier is 1.0
        assert_eq!(flicker_multiplier(0, 0, 1.0, 0.0, 0.3, 0.7), 1.0);
        assert_eq!(flicker_multiplier(0, 0, 1.0, 3.0, 0.0, 0.7), 1.0);
    }

    #[test]
    fn flicker_within_bounds() {
        // For any time, result should be in [flicker_min, flicker_min + strength]
        let min = 0.7;
        let strength = 0.3;
        for t in 0..100 {
            let m = flicker_multiplier(5, 10, t as f32 * 0.1, 3.0, strength, min);
            assert!(
                m >= min - 0.01 && m <= min + strength + 0.01,
                "flicker at t={}: {m} not in [{min}, {}]",
                t as f32 * 0.1,
                min + strength
            );
        }
    }

    #[test]
    fn flicker_different_tiles_differ() {
        // Two adjacent tiles at the same time should have different multipliers
        let a = flicker_multiplier(0, 0, 1.0, 3.0, 0.3, 0.7);
        let b = flicker_multiplier(1, 0, 1.0, 3.0, 0.3, 0.7);
        assert!(
            (a - b).abs() > 0.001,
            "adjacent tiles shouldn't sync: a={a}, b={b}"
        );
    }
}
