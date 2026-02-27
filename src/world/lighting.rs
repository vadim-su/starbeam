use std::collections::{HashMap, HashSet, VecDeque};

use crate::world::chunk::{tile_to_chunk, WorldMap};
use crate::world::ctx::WorldCtxRef;

pub const SUN_COLOR: [u8; 3] = [255, 250, 230];
pub const LIGHT_FALLOFF: u8 = 17;
pub const OPACITY_SCALE: u16 = 17;
pub const MAX_LIGHT_RADIUS: i32 = 16;

/// Subtract `amount` from each channel, clamping to 0.
fn attenuate(light: [u8; 3], amount: u16) -> [u8; 3] {
    let a = amount.min(255) as u8;
    [
        light[0].saturating_sub(a),
        light[1].saturating_sub(a),
        light[2].saturating_sub(a),
    ]
}

/// Per-channel max of two light values.
fn merge_light(a: [u8; 3], b: [u8; 3]) -> [u8; 3] {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])]
}

/// True if all channels are zero (fully dark).
fn is_dark(light: [u8; 3]) -> bool {
    light[0] == 0 && light[1] == 0 && light[2] == 0
}

/// Merge two light arrays element-wise using per-channel max.
pub fn merge_chunk_lights(a: &[[u8; 3]], b: &[[u8; 3]]) -> Vec<[u8; 3]> {
    a.iter()
        .zip(b.iter())
        .map(|(a, b)| merge_light(*a, *b))
        .collect()
}

/// Compute sunlight for a chunk by tracing rays downward from the sky.
///
/// For each column, starts with `SUN_COLOR` at the top of the world and
/// attenuates through every tile above the chunk, then propagates through
/// the chunk itself top-to-bottom.
pub fn compute_chunk_sunlight(
    world_map: &WorldMap,
    chunk_x: i32,
    chunk_y: i32,
    ctx: &WorldCtxRef,
) -> Vec<[u8; 3]> {
    let cs = ctx.config.chunk_size;
    let height_tiles = ctx.config.height_tiles;
    let base_x = chunk_x * cs as i32;
    let base_y = chunk_y * cs as i32;
    let total = (cs * cs) as usize;
    let mut result = vec![[0, 0, 0]; total];

    let chunk = world_map
        .chunks
        .get(&(chunk_x, chunk_y))
        .expect("chunk must be loaded before computing sunlight");

    for local_x in 0..cs {
        let tile_x = base_x + local_x as i32;
        let mut light = SUN_COLOR;

        // Trace from top of world down to the top of this chunk
        let chunk_top = base_y + cs as i32;
        for y in (chunk_top..height_tiles).rev() {
            if is_dark(light) {
                break;
            }
            if let Some(tile) = world_map.get_tile(tile_x, y, ctx) {
                let opacity = ctx.tile_registry.light_opacity(tile);
                if opacity > 0 {
                    light = attenuate(light, opacity as u16 * OPACITY_SCALE);
                }
            }
            // None = unloaded chunk → light passes through
        }

        // Propagate through this chunk top-to-bottom
        for local_y in (0..cs).rev() {
            if is_dark(light) {
                break;
            }
            let idx = (local_y * cs + local_x) as usize;
            result[idx] = light;

            let tile = chunk.get(local_x, local_y, cs);
            let opacity = ctx.tile_registry.light_opacity(tile);
            if opacity > 0 {
                light = attenuate(light, opacity as u16 * OPACITY_SCALE);
            }
        }
    }

    result
}

/// Compute point light contributions for a chunk via BFS from nearby emitters.
pub fn compute_point_lights(
    world_map: &WorldMap,
    chunk_x: i32,
    chunk_y: i32,
    ctx: &WorldCtxRef,
) -> Vec<[u8; 3]> {
    let cs = ctx.config.chunk_size;
    let cs_i32 = cs as i32;
    let height_tiles = ctx.config.height_tiles;
    let base_x = chunk_x * cs_i32;
    let base_y = chunk_y * cs_i32;
    let total = (cs * cs) as usize;
    let mut result = vec![[0, 0, 0]; total];

    let scan_min_y = (base_y - MAX_LIGHT_RADIUS).max(0);
    let scan_max_y = (base_y + cs_i32 + MAX_LIGHT_RADIUS).min(height_tiles);

    for scan_y in scan_min_y..scan_max_y {
        for scan_dx in -MAX_LIGHT_RADIUS..(cs_i32 + MAX_LIGHT_RADIUS) {
            let scan_x = base_x + scan_dx;
            let wrapped_x = ctx.config.wrap_tile_x(scan_x);
            let tile = world_map.get_tile(wrapped_x, scan_y, ctx);
            if let Some(tile_id) = tile {
                let emission = ctx.tile_registry.light_emission(tile_id);
                if !is_dark(emission) {
                    bfs_from_emitter(
                        world_map,
                        &mut result,
                        scan_x,
                        scan_y,
                        emission,
                        chunk_x,
                        chunk_y,
                        ctx,
                    );
                }
            }
        }
    }

    result
}

/// BFS flood-fill from a single light emitter, writing results into the target chunk.
#[allow(clippy::too_many_arguments)]
fn bfs_from_emitter(
    world_map: &WorldMap,
    result: &mut [[u8; 3]],
    start_x: i32,
    start_y: i32,
    emission: [u8; 3],
    chunk_x: i32,
    chunk_y: i32,
    ctx: &WorldCtxRef,
) {
    let cs = ctx.config.chunk_size;
    let cs_i32 = cs as i32;
    let height_tiles = ctx.config.height_tiles;
    let base_x = chunk_x * cs_i32;
    let base_y = chunk_y * cs_i32;

    let mut queue: VecDeque<(i32, i32, [u8; 3])> = VecDeque::new();
    let mut visited: HashMap<(i32, i32), [u8; 3]> = HashMap::new();

    queue.push_back((start_x, start_y, emission));

    while let Some((x, y, light)) = queue.pop_front() {
        if is_dark(light) {
            continue;
        }

        // Beyond max radius from emitter
        if (x - start_x).abs() > MAX_LIGHT_RADIUS || (y - start_y).abs() > MAX_LIGHT_RADIUS {
            continue;
        }

        let wrapped_x = ctx.config.wrap_tile_x(x);

        // Check visited: skip if all channels <= existing
        if let Some(existing) = visited.get(&(wrapped_x, y)) {
            if light[0] <= existing[0] && light[1] <= existing[1] && light[2] <= existing[2] {
                continue;
            }
            // Merge into visited
            let merged = merge_light(light, *existing);
            visited.insert((wrapped_x, y), merged);
        } else {
            visited.insert((wrapped_x, y), light);
        }

        // Write to result if tile is within target chunk bounds
        let local_x = wrapped_x - base_x;
        let local_y = y - base_y;
        if local_x >= 0 && local_x < cs_i32 && local_y >= 0 && local_y < cs_i32 {
            let idx = (local_y * cs_i32 + local_x) as usize;
            result[idx] = merge_light(result[idx], light);
        }

        // Compute transmitted light through this tile
        let opacity = world_map
            .get_tile(wrapped_x, y, ctx)
            .map(|t| ctx.tile_registry.light_opacity(t))
            .unwrap_or(0);
        let transmitted = attenuate(light, opacity as u16 * OPACITY_SCALE);

        // Spread to 4 neighbors
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let nx = x + dx;
            let ny = y + dy;
            if ny < 0 || ny >= height_tiles {
                continue;
            }
            let neighbor_light = attenuate(transmitted, LIGHT_FALLOFF as u16);
            if !is_dark(neighbor_light) {
                queue.push_back((nx, ny, neighbor_light));
            }
        }
    }
}

/// Compute combined sunlight + point light for a chunk.
pub fn compute_chunk_lighting(
    world_map: &WorldMap,
    chunk_x: i32,
    chunk_y: i32,
    ctx: &WorldCtxRef,
) -> Vec<[u8; 3]> {
    let sun = compute_chunk_sunlight(world_map, chunk_x, chunk_y, ctx);
    let point = compute_point_lights(world_map, chunk_x, chunk_y, ctx);
    merge_chunk_lights(&sun, &point)
}

/// Recompute lighting for a 3×3 chunk area around a changed tile.
///
/// Returns the set of chunk coordinates whose light data was updated.
#[allow(dead_code)] // Called from block_action on tile break/place (Task 7)
pub fn relight_around(
    world_map: &mut WorldMap,
    center_x: i32,
    center_y: i32,
    ctx: &WorldCtxRef,
) -> HashSet<(i32, i32)> {
    let wrapped = ctx.config.wrap_tile_x(center_x);
    let (center_cx, center_cy) = tile_to_chunk(wrapped, center_y, ctx.config.chunk_size);

    // Phase 1: compute lighting (immutable borrow)
    #[allow(clippy::type_complexity)]
    let updates: Vec<((i32, i32), Vec<[u8; 3]>)> = {
        let wm: &WorldMap = &*world_map;
        let mut results = Vec::new();
        for dy in -1..=1 {
            for dx in -1..=1 {
                let cy = center_cy + dy;
                if cy < 0 || cy >= ctx.config.height_chunks() {
                    continue;
                }
                let cx = ctx.config.wrap_chunk_x(center_cx + dx);
                if wm.chunks.contains_key(&(cx, cy)) {
                    let light = compute_chunk_lighting(wm, cx, cy, ctx);
                    results.push(((cx, cy), light));
                }
            }
        }
        results
    };

    // Phase 2: write back (mutable borrow)
    let mut dirty = HashSet::new();
    for ((cx, cy), light) in updates {
        if let Some(chunk) = world_map.chunks.get_mut(&(cx, cy)) {
            chunk.light_levels = light;
            dirty.insert((cx, cy));
        }
    }

    dirty
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::tile::TileId;
    use crate::test_helpers::fixtures;
    use crate::world::chunk::WorldMap;

    #[test]
    fn merge_light_takes_max_per_channel() {
        assert_eq!(
            merge_light([100, 200, 50], [200, 100, 100]),
            [200, 200, 100]
        );
    }

    #[test]
    fn attenuate_clamps_to_zero() {
        assert_eq!(attenuate([100, 50, 10], 200), [0, 0, 0]);
        assert_eq!(attenuate([255, 255, 255], 0), [255, 255, 255]);
    }

    #[test]
    fn sunlight_open_sky() {
        // A chunk near the top of the world with all air tiles should receive full sunlight
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let cs = wc.chunk_size;

        // Place chunk at the very top of the world
        let top_chunk_y = wc.height_chunks() - 1;
        let mut map = WorldMap::default();

        // Generate chunk and fill with air
        map.get_or_generate_chunk(0, top_chunk_y, &ctx);
        let chunk = map.chunks.get_mut(&(0, top_chunk_y)).unwrap();
        for tile in chunk.tiles.iter_mut() {
            *tile = TileId::AIR;
        }

        let result = compute_chunk_sunlight(&map, 0, top_chunk_y, &ctx);

        // Top row of the chunk (local_y = cs-1) should have SUN_COLOR
        let top_row_start = ((cs - 1) * cs) as usize;
        assert_eq!(result[top_row_start], SUN_COLOR);
    }

    #[test]
    fn sunlight_blocked_by_solid() {
        // Chunk with top half air, bottom half stone → air tiles lit, below stone is dark
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let cs = wc.chunk_size;
        let stone = tr.by_name("stone");

        let top_chunk_y = wc.height_chunks() - 1;
        let mut map = WorldMap::default();

        // Generate chunk and set up: top half air, bottom half stone
        map.get_or_generate_chunk(0, top_chunk_y, &ctx);
        let chunk = map.chunks.get_mut(&(0, top_chunk_y)).unwrap();
        let half = cs / 2;
        for local_y in 0..cs {
            for local_x in 0..cs {
                let idx = (local_y * cs + local_x) as usize;
                if local_y >= half {
                    chunk.tiles[idx] = TileId::AIR;
                } else {
                    chunk.tiles[idx] = stone;
                }
            }
        }

        let result = compute_chunk_sunlight(&map, 0, top_chunk_y, &ctx);

        // Top row (air) should have SUN_COLOR
        let top_idx = ((cs - 1) * cs) as usize;
        assert_eq!(result[top_idx], SUN_COLOR);

        // Bottom row (below many stone tiles) should be dark
        // Stone has opacity 15, so attenuation per stone = 15 * 17 = 255
        // After just one stone tile, light is fully attenuated
        let bottom_idx = 0; // local_y=0, local_x=0
        assert!(
            is_dark(result[bottom_idx]),
            "Bottom row should be dark, got {:?}",
            result[bottom_idx]
        );
    }

    #[test]
    fn compute_lighting_merges_sun_and_point() {
        let a = vec![[100, 0, 50], [0, 200, 0]];
        let b = vec![[50, 100, 100], [100, 0, 200]];
        let merged = merge_chunk_lights(&a, &b);
        assert_eq!(merged, vec![[100, 100, 100], [100, 200, 200]]);
    }
}
