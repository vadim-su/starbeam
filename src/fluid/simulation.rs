use std::collections::HashSet;

use bevy::prelude::*;

use crate::fluid::cell::{FluidCell, FluidId};
use crate::fluid::registry::FluidRegistry;
use crate::registry::tile::{TileId, TileRegistry};
use crate::world::chunk::WorldMap;

/// Normal full-cell mass.
pub const MAX_MASS: f32 = 1.0;
/// Cells with less mass than this are considered empty.
pub const MIN_MASS: f32 = 0.001;
/// Flows smaller than this are damped.
pub const MIN_FLOW: f32 = 0.005;
/// Maximum flow per iteration (before viscosity scaling).
pub const MAX_SPEED: f32 = 1.0;

/// Configuration for the fluid simulation.
#[derive(Debug, Clone, Resource)]
pub struct FluidSimConfig {
    pub iterations_per_tick: u32,
    pub min_mass: f32,
    pub min_flow: f32,
    pub max_speed: f32,
}

impl Default for FluidSimConfig {
    fn default() -> Self {
        Self {
            iterations_per_tick: 2, // was 6 → 3 → 2
            min_mass: MIN_MASS,
            min_flow: MIN_FLOW,
            max_speed: MAX_SPEED, // was *4 → *2 → *1 (base speed)
        }
    }
}

/// Calculate how much mass should be in the bottom cell of two vertically
/// adjacent cells with the given total mass.
///
/// This implements the "slightly compressible liquid" model where bottom cells
/// can hold slightly more mass than top cells, creating implicit pressure.
pub fn get_stable_state(total_mass: f32, max_compress: f32) -> f32 {
    if total_mass <= MAX_MASS {
        // Not enough to fill even one cell — all goes to bottom
        total_mass
    } else if total_mass < 2.0 * MAX_MASS + max_compress {
        // Bottom cell full + proportional compression
        (MAX_MASS * MAX_MASS + total_mass * max_compress) / (MAX_MASS + max_compress)
    } else {
        // Both cells full — bottom has +max_compress more than top
        (total_mass + max_compress) / 2.0
    }
}

/// Run one iteration of the fluid simulation on a flat grid.
///
/// `tiles` is the foreground tile array (same indexing as fluids).
/// `fluids` is the current fluid state (read-only reference).
/// `new_fluids` is the output buffer (write).
/// `width` and `height` define the grid dimensions.
///
/// This function processes a single chunk. For cross-chunk flow,
/// the caller must handle boundary cells separately.
#[allow(clippy::too_many_arguments)]
pub fn simulate_grid(
    tiles: &[TileId],
    fluids: &[FluidCell],
    new_fluids: &mut [FluidCell],
    width: u32,
    height: u32,
    tile_registry: &TileRegistry,
    fluid_registry: &FluidRegistry,
    config: &FluidSimConfig,
) {
    // Copy current state to new_fluids as starting point
    new_fluids.copy_from_slice(fluids);

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let cell = fluids[idx];

            if cell.is_empty() {
                continue;
            }

            let def = fluid_registry.get(cell.fluid_id);
            let max_speed = config.max_speed * (1.0 - def.viscosity).max(0.1);
            let mut remaining = cell.mass;

            if def.is_gas {
                // Gas: flow UP first (primary), then horizontal, then DOWN (decompression)
                remaining = try_flow_vertical(
                    x,
                    y,
                    idx,
                    1, // +1 = up (primary for gas)
                    true,
                    remaining,
                    cell.fluid_id,
                    def.max_compress,
                    max_speed,
                    config.min_flow,
                    tiles,
                    fluids,
                    new_fluids,
                    width,
                    height,
                    tile_registry,
                );
                remaining = try_flow_horizontal(
                    x,
                    y,
                    idx,
                    remaining,
                    cell.fluid_id,
                    cell.mass,
                    max_speed,
                    config.min_flow,
                    tiles,
                    fluids,
                    new_fluids,
                    width,
                    height,
                    tile_registry,
                );
                try_flow_vertical(
                    x,
                    y,
                    idx,
                    -1, // -1 = down (decompression for gas)
                    false,
                    remaining,
                    cell.fluid_id,
                    def.max_compress,
                    max_speed,
                    config.min_flow,
                    tiles,
                    fluids,
                    new_fluids,
                    width,
                    height,
                    tile_registry,
                );
            } else {
                // Liquid: flow DOWN first (primary), then horizontal, then UP (decompression)
                remaining = try_flow_vertical(
                    x,
                    y,
                    idx,
                    -1, // -1 = down (primary for liquid)
                    true,
                    remaining,
                    cell.fluid_id,
                    def.max_compress,
                    max_speed,
                    config.min_flow,
                    tiles,
                    fluids,
                    new_fluids,
                    width,
                    height,
                    tile_registry,
                );
                remaining = try_flow_horizontal(
                    x,
                    y,
                    idx,
                    remaining,
                    cell.fluid_id,
                    cell.mass,
                    max_speed,
                    config.min_flow,
                    tiles,
                    fluids,
                    new_fluids,
                    width,
                    height,
                    tile_registry,
                );
                try_flow_vertical(
                    x,
                    y,
                    idx,
                    1, // +1 = up (decompression for liquid)
                    false,
                    remaining,
                    cell.fluid_id,
                    def.max_compress,
                    max_speed,
                    config.min_flow,
                    tiles,
                    fluids,
                    new_fluids,
                    width,
                    height,
                    tile_registry,
                );
            }
        }
    }

    // Clean up cells with negligible mass
    for cell in new_fluids.iter_mut() {
        if cell.mass < config.min_mass {
            *cell = FluidCell::EMPTY;
        }
    }
}

/// Try to flow vertically. `dy` is -1 (down) or +1 (up).
/// `is_primary` indicates whether this is the primary flow direction
/// (down for liquids, up for gases) vs decompression.
/// For primary direction: uses get_stable_state to determine target.
/// For decompression: only flows if mass > MAX_MASS.
/// Returns remaining mass.
#[allow(clippy::too_many_arguments)]
fn try_flow_vertical(
    x: u32,
    y: u32,
    idx: usize,
    dy: i32,
    is_primary: bool,
    remaining: f32,
    fluid_id: FluidId,
    max_compress: f32,
    max_speed: f32,
    min_flow: f32,
    tiles: &[TileId],
    fluids: &[FluidCell],
    new_fluids: &mut [FluidCell],
    width: u32,
    height: u32,
    tile_registry: &TileRegistry,
) -> f32 {
    if remaining <= 0.0 {
        return 0.0;
    }

    let ny = y as i32 + dy;
    if ny < 0 || ny >= height as i32 {
        return remaining;
    }

    let nidx = (ny as u32 * width + x) as usize;

    // Check if neighbor tile is solid
    if tile_registry.is_solid(tiles[nidx]) {
        return remaining;
    }

    // Check if neighbor has different fluid type (can't mix)
    let neighbor = fluids[nidx];
    if !neighbor.is_empty() && neighbor.fluid_id != fluid_id {
        return remaining;
    }

    let neighbor_mass = new_fluids[nidx].mass;
    let total = remaining + neighbor_mass;

    let flow = if is_primary {
        // Primary direction: use get_stable_state to determine how much
        // should be in the "lower" cell (the one fluid flows toward).
        let target_in_neighbor = get_stable_state(total, max_compress);
        target_in_neighbor - neighbor_mass
    } else {
        // Decompression: only compressed fluid flows in this direction
        if remaining <= MAX_MASS {
            return remaining;
        }
        let target_stay = get_stable_state(total, max_compress);
        remaining - target_stay
    };

    if flow <= 0.0 {
        return remaining;
    }

    let mut flow = flow;
    // Smooth small flows
    if flow > min_flow {
        flow *= 0.5;
    }
    // Clamp
    flow = flow.min(max_speed).min(remaining).max(0.0);

    if flow <= 0.0 {
        return remaining;
    }

    new_fluids[idx].mass -= flow;
    new_fluids[nidx].mass += flow;
    if new_fluids[nidx].fluid_id == FluidId::NONE {
        new_fluids[nidx].fluid_id = fluid_id;
    }

    remaining - flow
}

/// Try to flow horizontally (left and right).
/// Returns remaining mass.
#[allow(clippy::too_many_arguments)]
fn try_flow_horizontal(
    x: u32,
    y: u32,
    idx: usize,
    mut remaining: f32,
    fluid_id: FluidId,
    original_mass: f32,
    max_speed: f32,
    min_flow: f32,
    tiles: &[TileId],
    fluids: &[FluidCell],
    new_fluids: &mut [FluidCell],
    width: u32,
    _height: u32,
    tile_registry: &TileRegistry,
) -> f32 {
    // Try left
    if x > 0 {
        remaining = try_flow_side(
            x,
            y,
            idx,
            x - 1,
            remaining,
            fluid_id,
            original_mass,
            max_speed,
            min_flow,
            tiles,
            fluids,
            new_fluids,
            width,
            tile_registry,
        );
    }
    // Try right
    if x + 1 < width {
        remaining = try_flow_side(
            x,
            y,
            idx,
            x + 1,
            remaining,
            fluid_id,
            original_mass,
            max_speed,
            min_flow,
            tiles,
            fluids,
            new_fluids,
            width,
            tile_registry,
        );
    }
    remaining
}

/// Try to flow to a single horizontal neighbor.
/// Uses equalization: flow = (original_mass - neighbor_mass) / 4.
#[allow(clippy::too_many_arguments)]
fn try_flow_side(
    _x: u32,
    y: u32,
    idx: usize,
    nx: u32,
    remaining: f32,
    fluid_id: FluidId,
    original_mass: f32,
    max_speed: f32,
    min_flow: f32,
    tiles: &[TileId],
    fluids: &[FluidCell],
    new_fluids: &mut [FluidCell],
    width: u32,
    tile_registry: &TileRegistry,
) -> f32 {
    if remaining <= 0.0 {
        return 0.0;
    }

    let nidx = (y * width + nx) as usize;

    if tile_registry.is_solid(tiles[nidx]) {
        return remaining;
    }

    let neighbor = fluids[nidx];
    if !neighbor.is_empty() && neighbor.fluid_id != fluid_id {
        return remaining;
    }

    // Equalize: flow = (my_mass - neighbor_mass) / 4
    let mut flow = (original_mass - fluids[nidx].mass) / 4.0;
    if flow <= 0.0 {
        return remaining;
    }
    if flow > min_flow {
        flow *= 0.5;
    }
    flow = flow.min(max_speed).min(remaining).max(0.0);

    if flow <= 0.0 {
        return remaining;
    }

    new_fluids[idx].mass -= flow;
    new_fluids[nidx].mass += flow;
    if new_fluids[nidx].fluid_id == FluidId::NONE {
        new_fluids[nidx].fluid_id = fluid_id;
    }

    remaining - flow
}

// ---------------------------------------------------------------------------
// Cross-chunk boundary reconciliation
// ---------------------------------------------------------------------------

/// After running `simulate_grid` on each chunk in isolation, this function
/// transfers fluid across chunk boundaries.
///
/// For every pair of horizontally or vertically adjacent active chunks, we
/// look at the edge cells and let fluid equalize using the same rules as
/// `try_flow_side` / `try_flow_vertical`.
///
/// `width_chunks` is the number of chunks along the X axis (for wrapping).
/// `height_chunks` is the number of chunks along the Y axis.
/// Pass `width_chunks > 0` to enable horizontal wrapping.
pub fn reconcile_chunk_boundaries(
    world_map: &mut WorldMap,
    active_chunks: &HashSet<(i32, i32)>,
    chunk_size: u32,
    width_chunks: i32,
    height_chunks: i32,
    tile_registry: &TileRegistry,
    fluid_registry: &FluidRegistry,
    config: &FluidSimConfig,
) {
    // Collect transfers: Vec<(chunk_a, idx_a, chunk_b, idx_b, flow, fluid_id)>
    // We accumulate them first, then apply, to avoid borrow conflicts.
    let mut transfers: Vec<((i32, i32), usize, (i32, i32), usize, f32, FluidId)> = Vec::new();

    // Track processed boundary pairs to avoid double-processing when both
    // chunks in a pair are active.
    let mut processed: HashSet<((i32, i32), (i32, i32))> = HashSet::new();

    for &(cx, cy) in active_chunks {
        let Some(chunk) = world_map.chunks.get(&(cx, cy)) else {
            continue;
        };

        // --- Horizontal neighbors (left and right) ---
        for dx in [-1_i32, 1] {
            let ncx = if width_chunks > 0 {
                (cx + dx).rem_euclid(width_chunks)
            } else {
                cx + dx
            };

            // Directed key: (left_chunk, right_chunk) to deduplicate.
            // In a wrapping world, (A-right↔B-left) and (B-right↔A-left)
            // are distinct physical boundaries, so we must NOT canonicalize.
            let (left_cx, right_cx) = if dx == 1 { (cx, ncx) } else { (ncx, cx) };
            let key = ((left_cx, cy), (right_cx, cy));
            if !processed.insert(key) {
                continue;
            }

            if !world_map.chunks.contains_key(&(ncx, cy)) {
                continue;
            }

            // left_cx/right_cx already computed above for the dedup key.
            // Left chunk's right edge (local_x = chunk_size-1) <-> Right chunk's left edge (local_x = 0)
            let left_chunk = world_map.chunks.get(&(left_cx, cy)).unwrap();
            let right_chunk = world_map.chunks.get(&(right_cx, cy)).unwrap();

            for local_y in 0..chunk_size {
                let idx_left = (local_y * chunk_size + (chunk_size - 1)) as usize;
                let idx_right = (local_y * chunk_size) as usize;

                collect_horizontal_transfer(
                    &left_chunk.fluids,
                    &left_chunk.fg.tiles,
                    idx_left,
                    (left_cx, cy),
                    right_chunk,
                    idx_right,
                    (right_cx, cy),
                    tile_registry,
                    fluid_registry,
                    config,
                    &mut transfers,
                );
            }
        }

        // --- Vertical neighbors (bottom and top) ---
        for dy in [-1_i32, 1] {
            let ncy = cy + dy;
            if ncy < 0 || ncy >= height_chunks {
                continue;
            }

            // Directed key: (bottom_chunk, top_chunk) to deduplicate.
            let (bottom_cy, top_cy) = if dy == 1 {
                (cy, ncy) // current is bottom, neighbor is top
            } else {
                (ncy, cy) // neighbor is bottom, current is top
            };
            let key = ((cx, bottom_cy), (cx, top_cy));
            if !processed.insert(key) {
                continue;
            }

            if !world_map.chunks.contains_key(&(cx, ncy)) {
                continue;
            }

            let bottom_chunk = world_map.chunks.get(&(cx, bottom_cy)).unwrap();
            let top_chunk = world_map.chunks.get(&(cx, top_cy)).unwrap();

            for local_x in 0..chunk_size {
                let idx_bottom = ((chunk_size - 1) * chunk_size + local_x) as usize;
                let idx_top = local_x as usize;

                collect_vertical_transfer(
                    &bottom_chunk.fluids,
                    &bottom_chunk.fg.tiles,
                    idx_bottom,
                    (cx, bottom_cy),
                    top_chunk,
                    idx_top,
                    (cx, top_cy),
                    tile_registry,
                    fluid_registry,
                    config,
                    &mut transfers,
                );
            }
        }
    }

    // Apply all transfers
    for (chunk_a, idx_a, chunk_b, idx_b, flow, fluid_id) in transfers {
        if let Some(ca) = world_map.chunks.get_mut(&chunk_a) {
            ca.fluids[idx_a].mass -= flow;
            if ca.fluids[idx_a].mass < config.min_mass {
                ca.fluids[idx_a] = FluidCell::EMPTY;
            }
        }
        if let Some(cb) = world_map.chunks.get_mut(&chunk_b) {
            cb.fluids[idx_b].mass += flow;
            if cb.fluids[idx_b].fluid_id == FluidId::NONE {
                cb.fluids[idx_b].fluid_id = fluid_id;
            }
        }
    }
}

/// Collect a horizontal (left↔right) transfer between two edge cells in adjacent chunks.
#[allow(clippy::too_many_arguments)]
fn collect_horizontal_transfer(
    src_fluids: &[FluidCell],
    src_tiles: &[TileId],
    idx_src: usize,
    chunk_src: (i32, i32),
    dst_chunk: &crate::world::chunk::ChunkData,
    idx_dst: usize,
    chunk_dst: (i32, i32),
    tile_registry: &TileRegistry,
    _fluid_registry: &FluidRegistry,
    config: &FluidSimConfig,
    transfers: &mut Vec<((i32, i32), usize, (i32, i32), usize, f32, FluidId)>,
) {
    let cell_a = src_fluids[idx_src];
    let cell_b = dst_chunk.fluids[idx_dst];

    // Skip if source is empty
    if cell_a.is_empty() && cell_b.is_empty() {
        return;
    }

    // Skip if destination tile is solid
    if !cell_a.is_empty() && tile_registry.is_solid(dst_chunk.fg.tiles[idx_dst]) {
        // a -> b blocked
    } else if !cell_a.is_empty() && (cell_b.is_empty() || cell_b.fluid_id == cell_a.fluid_id) {
        let flow = (cell_a.mass - cell_b.mass) / 4.0;
        if flow > config.min_flow {
            let flow = (flow * 0.5).min(config.max_speed).min(cell_a.mass);
            if flow > 0.0 {
                transfers.push((
                    chunk_src,
                    idx_src,
                    chunk_dst,
                    idx_dst,
                    flow,
                    cell_a.fluid_id,
                ));
            }
        }
    }

    // Reverse direction: b -> a
    if !cell_b.is_empty() && tile_registry.is_solid(src_tiles[idx_src]) {
        // b -> a blocked
    } else if !cell_b.is_empty() && (cell_a.is_empty() || cell_a.fluid_id == cell_b.fluid_id) {
        let flow = (cell_b.mass - cell_a.mass) / 4.0;
        if flow > config.min_flow {
            let flow = (flow * 0.5).min(config.max_speed).min(cell_b.mass);
            if flow > 0.0 {
                transfers.push((
                    chunk_dst,
                    idx_dst,
                    chunk_src,
                    idx_src,
                    flow,
                    cell_b.fluid_id,
                ));
            }
        }
    }
}

/// Collect a vertical (bottom↔top) transfer between two edge cells in adjacent chunks.
/// `chunk_src` is the bottom chunk, `chunk_dst` is the top chunk.
#[allow(clippy::too_many_arguments)]
fn collect_vertical_transfer(
    src_fluids: &[FluidCell],
    src_tiles: &[TileId],
    idx_src: usize,
    chunk_src: (i32, i32),
    dst_chunk: &crate::world::chunk::ChunkData,
    idx_dst: usize,
    chunk_dst: (i32, i32),
    tile_registry: &TileRegistry,
    fluid_registry: &FluidRegistry,
    config: &FluidSimConfig,
    transfers: &mut Vec<((i32, i32), usize, (i32, i32), usize, f32, FluidId)>,
) {
    let cell_bottom = src_fluids[idx_src]; // top row of bottom chunk
    let cell_top = dst_chunk.fluids[idx_dst]; // bottom row of top chunk

    // Liquid falls down: top -> bottom (primary)
    if !cell_top.is_empty() && !tile_registry.is_solid(src_tiles[idx_src]) {
        let def = fluid_registry.get(cell_top.fluid_id);
        if !def.is_gas {
            // Liquid in top chunk wants to fall into bottom chunk
            if cell_bottom.is_empty() || cell_bottom.fluid_id == cell_top.fluid_id {
                let neighbor_mass = cell_bottom.mass;
                let total = cell_top.mass + neighbor_mass;
                let target_bottom = get_stable_state(total, def.max_compress);
                let flow = (target_bottom - neighbor_mass).max(0.0);
                if flow > config.min_flow {
                    let flow = (flow * 0.5).min(config.max_speed).min(cell_top.mass);
                    if flow > 0.0 {
                        transfers.push((
                            chunk_dst,
                            idx_dst,
                            chunk_src,
                            idx_src,
                            flow,
                            cell_top.fluid_id,
                        ));
                    }
                }
            }
        } else {
            // Gas in top chunk: decompression downward (only if compressed)
            if cell_top.mass > MAX_MASS
                && (cell_bottom.is_empty() || cell_bottom.fluid_id == cell_top.fluid_id)
            {
                let neighbor_mass = cell_bottom.mass;
                let total = cell_top.mass + neighbor_mass;
                let target_stay = get_stable_state(total, def.max_compress);
                let flow = (cell_top.mass - target_stay).max(0.0);
                if flow > config.min_flow {
                    let flow = (flow * 0.5).min(config.max_speed).min(cell_top.mass);
                    if flow > 0.0 {
                        transfers.push((
                            chunk_dst,
                            idx_dst,
                            chunk_src,
                            idx_src,
                            flow,
                            cell_top.fluid_id,
                        ));
                    }
                }
            }
        }
    }

    // Gas rises up: bottom -> top (primary)
    if !cell_bottom.is_empty() && !tile_registry.is_solid(dst_chunk.fg.tiles[idx_dst]) {
        let def = fluid_registry.get(cell_bottom.fluid_id);
        if def.is_gas {
            // Gas in bottom chunk wants to rise into top chunk
            if cell_top.is_empty() || cell_top.fluid_id == cell_bottom.fluid_id {
                let neighbor_mass = cell_top.mass;
                let total = cell_bottom.mass + neighbor_mass;
                let target_top = get_stable_state(total, def.max_compress);
                let flow = (target_top - neighbor_mass).max(0.0);
                if flow > config.min_flow {
                    let flow = (flow * 0.5).min(config.max_speed).min(cell_bottom.mass);
                    if flow > 0.0 {
                        transfers.push((
                            chunk_src,
                            idx_src,
                            chunk_dst,
                            idx_dst,
                            flow,
                            cell_bottom.fluid_id,
                        ));
                    }
                }
            }
        } else {
            // Liquid decompression upward (only if compressed)
            if cell_bottom.mass > MAX_MASS
                && (cell_top.is_empty() || cell_top.fluid_id == cell_bottom.fluid_id)
            {
                let neighbor_mass = cell_top.mass;
                let total = cell_bottom.mass + neighbor_mass;
                let target_stay = get_stable_state(total, def.max_compress);
                let flow = (cell_bottom.mass - target_stay).max(0.0);
                if flow > config.min_flow {
                    let flow = (flow * 0.5).min(config.max_speed).min(cell_bottom.mass);
                    if flow > 0.0 {
                        transfers.push((
                            chunk_src,
                            idx_src,
                            chunk_dst,
                            idx_dst,
                            flow,
                            cell_bottom.fluid_id,
                        ));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::registry::FluidDef;

    // --- get_stable_state tests ---

    #[test]
    fn stable_state_empty() {
        // No water at all
        let bottom = get_stable_state(0.0, 0.02);
        assert!((bottom - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn stable_state_half_cell() {
        // Half a cell — all goes to bottom
        let bottom = get_stable_state(0.5, 0.02);
        assert!((bottom - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn stable_state_one_cell() {
        // Exactly one cell — all in bottom
        let bottom = get_stable_state(1.0, 0.02);
        assert!((bottom - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn stable_state_two_cells() {
        // Two full cells — bottom should be slightly compressed
        // Formula: (1.0 + 2.0*0.02) / (1.0 + 0.02) = 1.04/1.02 ≈ 1.0196
        let bottom = get_stable_state(2.0, 0.02);
        assert!(bottom > 1.0, "Bottom should be > 1.0, got {bottom}");
        assert!(bottom < 1.02, "Bottom should be < 1.02, got {bottom}");
        // top = 2.0 - bottom ≈ 0.9804
        let top = 2.0 - bottom;
        assert!(
            top < 1.0,
            "Top should be < 1.0 (bottom gets more), got {top}"
        );
        assert!(bottom > top, "Bottom ({bottom}) should be > top ({top})");
    }

    #[test]
    fn stable_state_three_cells() {
        // Well above 2*MAX + compress
        let bottom = get_stable_state(3.0, 0.02);
        let top = 3.0 - bottom;
        let diff = bottom - top;
        assert!(
            (diff - 0.02).abs() < f32::EPSILON,
            "Difference should be exactly 0.02, got {diff}"
        );
    }

    #[test]
    fn stable_state_always_positive() {
        for i in 0..100 {
            let total = i as f32 * 0.1;
            let bottom = get_stable_state(total, 0.02);
            assert!(bottom >= 0.0, "Bottom should be >= 0 for total={total}");
            assert!(
                bottom <= total,
                "Bottom ({bottom}) should be <= total ({total})"
            );
        }
    }

    // --- Simulation integration tests ---

    fn test_tile_registry() -> TileRegistry {
        crate::test_helpers::fixtures::test_tile_registry()
    }

    fn test_fluid_registry() -> FluidRegistry {
        FluidRegistry::from_defs(vec![
            FluidDef {
                id: "water".to_string(),
                density: 1000.0,
                viscosity: 0.0, // no viscosity for tests
                max_compress: 0.02,
                is_gas: false,
                color: [64, 128, 255, 180],
                damage_on_contact: 0.0,
                light_emission: [0, 0, 0],
                effects: vec![],
                wave_amplitude: 1.0,
                wave_speed: 1.0,
                light_absorption: 0.3,
            },
            FluidDef {
                id: "gas".to_string(),
                density: 0.5,
                viscosity: 0.0,
                max_compress: 0.01,
                is_gas: true,
                color: [200, 200, 200, 100],
                damage_on_contact: 0.0,
                light_emission: [0, 0, 0],
                effects: vec![],
                wave_amplitude: 1.0,
                wave_speed: 1.0,
                light_absorption: 0.05,
            },
        ])
    }

    fn make_grid(width: u32, height: u32) -> (Vec<TileId>, Vec<FluidCell>) {
        let len = (width * height) as usize;
        (vec![TileId::AIR; len], vec![FluidCell::EMPTY; len])
    }

    fn idx(x: u32, y: u32, width: u32) -> usize {
        (y * width + x) as usize
    }

    #[test]
    fn water_falls_down() {
        let w = 3;
        let h = 3;
        let (tiles, mut fluids) = make_grid(w, h);
        let mut new_fluids = fluids.clone();
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig {
            iterations_per_tick: 1,
            ..Default::default()
        };

        let water_id = fr.by_name("water");
        // Place water at top-center (x=1, y=2)
        fluids[idx(1, 2, w)] = FluidCell::new(water_id, 1.0);

        simulate_grid(&tiles, &fluids, &mut new_fluids, w, h, &tr, &fr, &config);

        // Water should have moved down (y=2 -> y=1)
        assert!(
            new_fluids[idx(1, 1, w)].mass > 0.0,
            "Water should flow to cell below"
        );
        assert!(
            new_fluids[idx(1, 2, w)].mass < 1.0,
            "Source cell should have less water"
        );
    }

    #[test]
    fn water_spreads_horizontally_on_floor() {
        let w = 5;
        let h = 3;
        let (mut tiles, mut fluids) = make_grid(w, h);
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig::default();

        let water_id = fr.by_name("water");
        // Solid floor at y=0
        for x in 0..w {
            tiles[idx(x, 0, w)] = TileId(3); // stone = solid
        }
        // Water at center, resting on floor
        fluids[idx(2, 1, w)] = FluidCell::new(water_id, 1.0);

        // Run several iterations to let water spread
        let mut current = fluids.clone();
        for _ in 0..10 {
            let mut new = current.clone();
            simulate_grid(&tiles, &current, &mut new, w, h, &tr, &fr, &config);
            current = new;
        }

        // Water should have spread left and right
        assert!(current[idx(1, 1, w)].mass > 0.0, "Water should spread left");
        assert!(
            current[idx(3, 1, w)].mass > 0.0,
            "Water should spread right"
        );
    }

    #[test]
    fn water_blocked_by_solid() {
        let w = 3;
        let h = 3;
        let (mut tiles, mut fluids) = make_grid(w, h);
        let mut new_fluids = fluids.clone();
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig::default();

        let water_id = fr.by_name("water");
        // Solid block below water
        tiles[idx(1, 0, w)] = TileId(3); // stone
        tiles[idx(1, 1, w)] = TileId(3); // stone
                                         // Water above
        fluids[idx(1, 2, w)] = FluidCell::new(water_id, 1.0);

        simulate_grid(&tiles, &fluids, &mut new_fluids, w, h, &tr, &fr, &config);

        // Water should NOT be in the solid cell
        assert!(
            new_fluids[idx(1, 1, w)].mass <= 0.0,
            "Water should not enter solid cell"
        );
    }

    #[test]
    fn pressure_pushes_water_up() {
        // Stack 3 water cells in a 1-wide column
        let w = 1;
        let h = 5;
        let (tiles, mut fluids) = make_grid(w, h);
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig {
            iterations_per_tick: 1,
            ..Default::default()
        };
        let water_id = fr.by_name("water");

        // Stack 3 water cells
        fluids[idx(0, 0, w)] = FluidCell::new(water_id, 1.0);
        fluids[idx(0, 1, w)] = FluidCell::new(water_id, 1.0);
        fluids[idx(0, 2, w)] = FluidCell::new(water_id, 1.0);

        // Run iterations so pressure builds
        let mut current = fluids.clone();
        for _ in 0..20 {
            let mut new = current.clone();
            simulate_grid(&tiles, &current, &mut new, w, h, &tr, &fr, &config);
            current = new;
        }

        // Bottom cell should be compressed (mass > 1.0)
        assert!(
            current[idx(0, 0, w)].mass > 1.0,
            "Bottom cell should be compressed, got {}",
            current[idx(0, 0, w)].mass
        );
    }

    #[test]
    fn gas_flows_up() {
        let w = 3;
        let h = 3;
        let (tiles, mut fluids) = make_grid(w, h);
        let mut new_fluids = fluids.clone();
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig::default();

        let gas_id = fr.by_name("gas");
        // Place gas at bottom-center
        fluids[idx(1, 0, w)] = FluidCell::new(gas_id, 1.0);

        simulate_grid(&tiles, &fluids, &mut new_fluids, w, h, &tr, &fr, &config);

        // Gas should have moved up
        assert!(
            new_fluids[idx(1, 1, w)].mass > 0.0,
            "Gas should flow upward"
        );
        assert!(
            new_fluids[idx(1, 0, w)].mass < 1.0,
            "Source cell should have less gas"
        );
    }

    #[test]
    fn mass_is_conserved() {
        let w = 5;
        let h = 5;
        let (mut tiles, mut fluids) = make_grid(w, h);
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig::default();
        let water_id = fr.by_name("water");

        // Floor
        for x in 0..w {
            tiles[idx(x, 0, w)] = TileId(3);
        }

        // Add water
        fluids[idx(2, 3, w)] = FluidCell::new(water_id, 1.0);
        fluids[idx(2, 2, w)] = FluidCell::new(water_id, 0.7);

        let initial_mass: f32 = fluids.iter().map(|c| c.mass).sum();

        let mut current = fluids;
        for _ in 0..50 {
            let mut new = current.clone();
            simulate_grid(&tiles, &current, &mut new, w, h, &tr, &fr, &config);
            current = new;
        }

        let final_mass: f32 = current.iter().map(|c| c.mass).sum();
        assert!(
            (initial_mass - final_mass).abs() < 0.01,
            "Mass should be conserved: initial={initial_mass}, final={final_mass}"
        );
    }

    // ---------------------------------------------------------------
    // Cross-chunk boundary tests
    // ---------------------------------------------------------------

    use crate::world::chunk::{ChunkData, TileLayer};

    /// Helper: create a minimal ChunkData with all-air tiles and empty fluids.
    fn make_chunk(chunk_size: u32) -> ChunkData {
        let len = (chunk_size * chunk_size) as usize;
        ChunkData {
            fg: TileLayer::new_air(len),
            bg: TileLayer::new_air(len),
            fluids: vec![FluidCell::EMPTY; len],
            objects: Vec::new(),
            occupancy: vec![None; len],
            damage: vec![0; len],
        }
    }

    #[test]
    fn water_flows_horizontally_across_chunk_boundary() {
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig::default();
        let water_id = fr.by_name("water");
        let cs: u32 = 4; // small chunk for testing

        // Two horizontally adjacent chunks: (0,0) and (1,0)
        let mut world_map = WorldMap::default();
        let mut chunk_a = make_chunk(cs);
        let chunk_b = make_chunk(cs);

        // Place water at right edge of chunk A (local_x = 3, local_y = 1)
        chunk_a.fluids[idx(cs - 1, 1, cs)] = FluidCell::new(water_id, 1.0);

        world_map.chunks.insert((0, 0), chunk_a);
        world_map.chunks.insert((1, 0), chunk_b);

        let mut active = HashSet::new();
        active.insert((0, 0));
        active.insert((1, 0));

        // Run reconciliation several times
        for _ in 0..20 {
            reconcile_chunk_boundaries(
                &mut world_map,
                &active,
                cs,
                2, // width_chunks
                1, // height_chunks
                &tr,
                &fr,
                &config,
            );
        }

        // Water should have flowed to the left edge of chunk B (local_x = 0, local_y = 1)
        let chunk_b = world_map.chunks.get(&(1, 0)).unwrap();
        assert!(
            chunk_b.fluids[idx(0, 1, cs)].mass > 0.0,
            "Water should flow from right edge of chunk A to left edge of chunk B, got mass={}",
            chunk_b.fluids[idx(0, 1, cs)].mass,
        );
        assert_eq!(
            chunk_b.fluids[idx(0, 1, cs)].fluid_id,
            water_id,
            "Transferred fluid should be water"
        );
    }

    #[test]
    fn water_falls_down_across_chunk_boundary() {
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig::default();
        let water_id = fr.by_name("water");
        let cs: u32 = 4;

        // Two vertically adjacent chunks: (0,0) bottom, (0,1) top
        let mut world_map = WorldMap::default();
        let chunk_bottom = make_chunk(cs);
        let mut chunk_top = make_chunk(cs);

        // Place water at bottom edge of top chunk (local_y = 0, local_x = 2)
        chunk_top.fluids[idx(2, 0, cs)] = FluidCell::new(water_id, 1.0);

        world_map.chunks.insert((0, 0), chunk_bottom);
        world_map.chunks.insert((0, 1), chunk_top);

        let mut active = HashSet::new();
        active.insert((0, 0));
        active.insert((0, 1));

        for _ in 0..20 {
            reconcile_chunk_boundaries(&mut world_map, &active, cs, 1, 2, &tr, &fr, &config);
        }

        // Water should have fallen to top edge of bottom chunk (local_y = chunk_size-1, local_x = 2)
        let chunk_bottom = world_map.chunks.get(&(0, 0)).unwrap();
        assert!(
            chunk_bottom.fluids[idx(2, cs - 1, cs)].mass > 0.0,
            "Water should fall from bottom of top chunk to top of bottom chunk, got mass={}",
            chunk_bottom.fluids[idx(2, cs - 1, cs)].mass,
        );
    }

    #[test]
    fn gas_rises_across_chunk_boundary() {
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig::default();
        let gas_id = fr.by_name("gas");
        let cs: u32 = 4;

        let mut world_map = WorldMap::default();
        let mut chunk_bottom = make_chunk(cs);
        let chunk_top = make_chunk(cs);

        // Place gas at top edge of bottom chunk (local_y = chunk_size-1, local_x = 2)
        chunk_bottom.fluids[idx(2, cs - 1, cs)] = FluidCell::new(gas_id, 1.0);

        world_map.chunks.insert((0, 0), chunk_bottom);
        world_map.chunks.insert((0, 1), chunk_top);

        let mut active = HashSet::new();
        active.insert((0, 0));
        active.insert((0, 1));

        for _ in 0..20 {
            reconcile_chunk_boundaries(&mut world_map, &active, cs, 1, 2, &tr, &fr, &config);
        }

        // Gas should have risen to bottom edge of top chunk (local_y = 0, local_x = 2)
        let chunk_top = world_map.chunks.get(&(0, 1)).unwrap();
        assert!(
            chunk_top.fluids[idx(2, 0, cs)].mass > 0.0,
            "Gas should rise from top of bottom chunk to bottom of top chunk, got mass={}",
            chunk_top.fluids[idx(2, 0, cs)].mass,
        );
    }

    #[test]
    fn cross_chunk_mass_is_conserved() {
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig::default();
        let water_id = fr.by_name("water");
        let cs: u32 = 4;

        let mut world_map = WorldMap::default();
        let mut chunk_a = make_chunk(cs);
        let chunk_b = make_chunk(cs);

        // Place water at boundary
        chunk_a.fluids[idx(cs - 1, 1, cs)] = FluidCell::new(water_id, 1.0);
        chunk_a.fluids[idx(cs - 1, 2, cs)] = FluidCell::new(water_id, 0.7);

        let initial_mass = 1.0 + 0.7;

        world_map.chunks.insert((0, 0), chunk_a);
        world_map.chunks.insert((1, 0), chunk_b);

        let mut active = HashSet::new();
        active.insert((0, 0));
        active.insert((1, 0));

        for _ in 0..50 {
            reconcile_chunk_boundaries(&mut world_map, &active, cs, 2, 1, &tr, &fr, &config);
        }

        let final_mass: f32 = world_map
            .chunks
            .values()
            .flat_map(|c| c.fluids.iter())
            .map(|c| c.mass)
            .sum();

        assert!(
            (initial_mass - final_mass).abs() < 0.01,
            "Cross-chunk mass should be conserved: initial={initial_mass}, final={final_mass}"
        );
    }

    #[test]
    fn cross_chunk_water_blocked_by_solid() {
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig::default();
        let water_id = fr.by_name("water");
        let cs: u32 = 4;

        let mut world_map = WorldMap::default();
        let mut chunk_a = make_chunk(cs);
        let mut chunk_b = make_chunk(cs);

        // Water at right edge of chunk A
        chunk_a.fluids[idx(cs - 1, 1, cs)] = FluidCell::new(water_id, 1.0);
        // Solid wall at left edge of chunk B
        chunk_b.fg.tiles[idx(0, 1, cs)] = TileId(3); // stone

        world_map.chunks.insert((0, 0), chunk_a);
        world_map.chunks.insert((1, 0), chunk_b);

        let mut active = HashSet::new();
        active.insert((0, 0));
        active.insert((1, 0));

        for _ in 0..20 {
            reconcile_chunk_boundaries(&mut world_map, &active, cs, 2, 1, &tr, &fr, &config);
        }

        // Water should NOT enter solid cell
        let chunk_b = world_map.chunks.get(&(1, 0)).unwrap();
        assert!(
            chunk_b.fluids[idx(0, 1, cs)].is_empty(),
            "Water should not flow into solid tile across chunk boundary"
        );
        // Original water should remain
        let chunk_a = world_map.chunks.get(&(0, 0)).unwrap();
        assert!(
            chunk_a.fluids[idx(cs - 1, 1, cs)].mass > 0.9,
            "Water should remain at source since it's blocked"
        );
    }

    #[test]
    fn horizontal_wrap_around_chunks() {
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig::default();
        let water_id = fr.by_name("water");
        let cs: u32 = 4;

        // World is 2 chunks wide — chunk 0 and chunk 1 wrap around
        let mut world_map = WorldMap::default();
        let mut chunk_last = make_chunk(cs);
        let chunk_first = make_chunk(cs);

        // Water at right edge of last chunk (chunk 1)
        chunk_last.fluids[idx(cs - 1, 1, cs)] = FluidCell::new(water_id, 1.0);

        world_map.chunks.insert((0, 0), chunk_first);
        world_map.chunks.insert((1, 0), chunk_last);

        let mut active = HashSet::new();
        active.insert((0, 0));
        active.insert((1, 0));

        for _ in 0..20 {
            reconcile_chunk_boundaries(
                &mut world_map,
                &active,
                cs,
                2, // width_chunks = 2, so chunk 1's right neighbor wraps to chunk 0
                1,
                &tr,
                &fr,
                &config,
            );
        }

        // Water should have wrapped around to left edge of chunk 0
        let chunk_first = world_map.chunks.get(&(0, 0)).unwrap();
        assert!(
            chunk_first.fluids[idx(0, 1, cs)].mass > 0.0,
            "Water should wrap from right edge of last chunk to left edge of first chunk, got mass={}",
            chunk_first.fluids[idx(0, 1, cs)].mass,
        );
    }

    #[test]
    fn water_on_left_edge_flows_left() {
        // Bug regression: only right+top neighbors were checked.
        // Water on the LEFT edge of an active chunk must also flow to its left neighbor.
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig::default();
        let water_id = fr.by_name("water");
        let cs: u32 = 4;

        let mut world_map = WorldMap::default();
        let chunk_left = make_chunk(cs);
        let mut chunk_right = make_chunk(cs);

        // Water at LEFT edge of chunk (1,0), only chunk (1,0) is active
        chunk_right.fluids[idx(0, 1, cs)] = FluidCell::new(water_id, 1.0);

        world_map.chunks.insert((0, 0), chunk_left);
        world_map.chunks.insert((1, 0), chunk_right);

        // Only the chunk with water is active
        let mut active = HashSet::new();
        active.insert((1, 0));

        for _ in 0..20 {
            reconcile_chunk_boundaries(&mut world_map, &active, cs, 2, 1, &tr, &fr, &config);
        }

        // Water should have flowed to right edge of chunk (0,0)
        let chunk_left = world_map.chunks.get(&(0, 0)).unwrap();
        assert!(
            chunk_left.fluids[idx(cs - 1, 1, cs)].mass > 0.0,
            "Water on left edge should flow to right edge of left neighbor, got mass={}",
            chunk_left.fluids[idx(cs - 1, 1, cs)].mass,
        );
    }

    #[test]
    fn water_on_bottom_edge_falls_to_chunk_below() {
        // Bug regression: only top neighbor was checked.
        // Water on the BOTTOM edge of an active chunk must fall into the chunk below.
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig::default();
        let water_id = fr.by_name("water");
        let cs: u32 = 4;

        let mut world_map = WorldMap::default();
        let chunk_below = make_chunk(cs);
        let mut chunk_above = make_chunk(cs);

        // Water at bottom edge of chunk (0,1)
        chunk_above.fluids[idx(2, 0, cs)] = FluidCell::new(water_id, 1.0);

        world_map.chunks.insert((0, 0), chunk_below);
        world_map.chunks.insert((0, 1), chunk_above);

        // Only the chunk with water is active
        let mut active = HashSet::new();
        active.insert((0, 1));

        for _ in 0..20 {
            reconcile_chunk_boundaries(&mut world_map, &active, cs, 1, 2, &tr, &fr, &config);
        }

        // Water should have fallen to top edge of chunk (0,0)
        let chunk_below = world_map.chunks.get(&(0, 0)).unwrap();
        assert!(
            chunk_below.fluids[idx(2, cs - 1, cs)].mass > 0.0,
            "Water on bottom edge should fall to top edge of chunk below, got mass={}",
            chunk_below.fluids[idx(2, cs - 1, cs)].mass,
        );
    }
}
