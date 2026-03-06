use std::collections::HashMap;

use bevy::prelude::*;

use crate::cosmos::persistence::DirtyChunks;
use crate::liquid::data::*;
use crate::liquid::registry::LiquidRegistry;
use crate::liquid::render::DirtyLiquidChunks;
use crate::liquid::sleep::SleepTracker;
use crate::registry::tile::TileRegistry;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::{self, WorldMap};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Fixed timestep for liquid simulation (30 Hz).
const LIQUID_DT: f32 = 1.0 / 30.0;

/// Number of CA iterations per timestep. More iterations = faster
/// convergence but higher CPU cost. 2 gives good results at 30 Hz.
const ITERATIONS_PER_STEP: u32 = 2;

// ---------------------------------------------------------------------------
// Resource
// ---------------------------------------------------------------------------

/// Liquid simulation state resource.
#[derive(Resource, Default)]
pub struct LiquidSimState {
    pub sleep: SleepTracker,
    pub accumulator: f32,
}

// ---------------------------------------------------------------------------
// Bevy system
// ---------------------------------------------------------------------------

/// The main liquid simulation system. Runs at 30 Hz via accumulator.
#[allow(clippy::too_many_arguments)]
pub fn liquid_simulation_system(
    mut commands: Commands,
    time: Res<Time>,
    config: Res<ActiveWorld>,
    tile_registry: Res<TileRegistry>,
    liquid_registry: Res<LiquidRegistry>,
    mut world_map: ResMut<WorldMap>,
    mut sim_state: ResMut<LiquidSimState>,
    dirty_resources: (
        ResMut<DirtyChunks>,
        ResMut<DirtyLiquidChunks>,
        ResMut<crate::world::rc_lighting::RcGridDirty>,
    ),
    _loaded_chunks: Res<chunk::LoadedChunks>,
    chunk_query: Query<(Entity, &chunk::ChunkCoord, &chunk::ChunkLayer)>,
) {
    let (mut dirty_chunks, mut dirty_liquid, mut rc_dirty) = dirty_resources;

    if liquid_registry.defs.is_empty() {
        return;
    }

    sim_state.accumulator += time.delta_secs().min(0.1);

    let mut steps = 0u32;
    let mut all_produced: Vec<(i32, i32)> = Vec::new();
    while sim_state.accumulator >= LIQUID_DT {
        sim_state.accumulator -= LIQUID_DT;
        steps += 1;
        let produced = run_liquid_step(
            &config,
            &tile_registry,
            &liquid_registry,
            &mut world_map,
            &mut sim_state.sleep,
            &mut dirty_chunks,
            &mut dirty_liquid,
        );
        all_produced.extend(produced);
    }

    // Mark tile mesh entities dirty for any reaction-produced solid tiles.
    if !all_produced.is_empty() {
        rc_dirty.0 = true;
        for &(tx, ty) in &all_produced {
            let wtx = config.wrap_tile_x(tx);
            let (cx, cy) = chunk::tile_to_chunk(wtx, ty, config.chunk_size);
            for (entity, coord, _layer) in &chunk_query {
                let dcx = config.wrap_chunk_x(coord.x);
                if dcx == cx && coord.y == cy {
                    commands.entity(entity).insert(chunk::ChunkDirty);
                }
            }
        }
    }

    if steps > 0 && sim_state.sleep.active_count() > 0 {
        debug!(
            "Liquid sim: {} steps, {} active tiles",
            steps,
            sim_state.sleep.active_count()
        );
    }
}

// ---------------------------------------------------------------------------
// Helpers — chunk-level liquid access
// ---------------------------------------------------------------------------

fn get_liquid_from_map(world_map: &WorldMap, tx: i32, ty: i32, config: &ActiveWorld) -> LiquidCell {
    if ty < 0 || ty >= config.height_tiles {
        return LiquidCell::EMPTY;
    }
    let wx = config.wrap_tile_x(tx);
    let (cx, cy) = chunk::tile_to_chunk(wx, ty, config.chunk_size);
    let (lx, ly) = chunk::tile_to_local(wx, ty, config.chunk_size);
    match world_map.chunk(cx, cy) {
        Some(chunk) => chunk.liquid.get(lx, ly, config.chunk_size),
        None => LiquidCell::EMPTY,
    }
}

fn set_liquid_in_map(
    world_map: &mut WorldMap,
    tx: i32,
    ty: i32,
    cell: LiquidCell,
    config: &ActiveWorld,
) {
    if ty < 0 || ty >= config.height_tiles {
        return;
    }
    let wx = config.wrap_tile_x(tx);
    let (cx, cy) = chunk::tile_to_chunk(wx, ty, config.chunk_size);
    let (lx, ly) = chunk::tile_to_local(wx, ty, config.chunk_size);
    if let Some(chunk) = world_map.chunk_mut(cx, cy) {
        chunk.liquid.set(lx, ly, cell, config.chunk_size);
    }
}

fn is_solid_at(
    world_map: &WorldMap,
    tile_registry: &TileRegistry,
    tx: i32,
    ty: i32,
    config: &ActiveWorld,
) -> bool {
    if ty < 0 {
        return true; // bedrock
    }
    if ty >= config.height_tiles {
        return false; // sky
    }
    let wx = config.wrap_tile_x(tx);
    let (cx, cy) = chunk::tile_to_chunk(wx, ty, config.chunk_size);
    let (lx, ly) = chunk::tile_to_local(wx, ty, config.chunk_size);
    match world_map.chunk(cx, cy) {
        Some(chunk) => {
            let tile_id = chunk.fg.get(lx, ly, config.chunk_size);
            tile_registry.is_solid(tile_id)
        }
        None => true, // unloaded = solid
    }
}

/// Mark a tile and its chunk as dirty for persistence and rendering.
fn mark_dirty(
    tx: i32,
    ty: i32,
    config: &ActiveWorld,
    dirty_chunks: &mut DirtyChunks,
    dirty_liquid: &mut DirtyLiquidChunks,
    sleep: &mut SleepTracker,
) {
    let wx = config.wrap_tile_x(tx);
    let (cx, cy) = chunk::tile_to_chunk(wx, ty, config.chunk_size);
    dirty_chunks.0.insert((cx, cy));
    dirty_liquid.0.insert((cx, cy));
    sleep.mark_changed(tx, ty);
}

// ---------------------------------------------------------------------------
// Simulation step — jgallant / Starbound cellular automata
// ---------------------------------------------------------------------------

/// One simulation tick operating directly on WorldMap chunk data.
/// Returns list of tile coords where reactions produced solid tiles.
#[allow(clippy::too_many_arguments)]
fn run_liquid_step(
    config: &ActiveWorld,
    tile_registry: &TileRegistry,
    liquid_registry: &LiquidRegistry,
    world_map: &mut WorldMap,
    sleep: &mut SleepTracker,
    dirty_chunks: &mut DirtyChunks,
    dirty_liquid: &mut DirtyLiquidChunks,
) -> Vec<(i32, i32)> {
    let mut produced_tiles: Vec<(i32, i32)> = Vec::new();

    // Run the core flow algorithm multiple times for faster convergence.
    for _ in 0..ITERATIONS_PER_STEP {
        run_flow_iteration(
            config,
            tile_registry,
            liquid_registry,
            world_map,
            sleep,
            dirty_chunks,
            dirty_liquid,
        );
    }

    // --- Reactions: adjacent cells of different types ---
    run_reactions(
        config,
        tile_registry,
        liquid_registry,
        world_map,
        sleep,
        dirty_chunks,
        dirty_liquid,
        &mut produced_tiles,
    );

    // --- Density sorting: lighter-below-denser swaps ---
    run_density_sort(
        config,
        tile_registry,
        liquid_registry,
        world_map,
        sleep,
        dirty_chunks,
        dirty_liquid,
    );

    produced_tiles
}

// ---------------------------------------------------------------------------
// Pass 1: jgallant flow iteration
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn run_flow_iteration(
    config: &ActiveWorld,
    tile_registry: &TileRegistry,
    liquid_registry: &LiquidRegistry,
    world_map: &mut WorldMap,
    sleep: &mut SleepTracker,
    dirty_chunks: &mut DirtyChunks,
    dirty_liquid: &mut DirtyLiquidChunks,
) {
    // Snapshot active tiles and sort bottom-to-top, left-to-right for
    // consistent iteration order (the /4 vs /3 left/right asymmetry
    // compensates for this sweep direction).
    let mut active: Vec<(i32, i32)> = sleep.active_tiles().collect();
    active.sort_unstable_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

    // Accumulate level deltas. Applied to WorldMap after all cells processed.
    // Value: (delta, source_liquid_type for type propagation).
    let mut diffs: HashMap<(i32, i32), (f32, LiquidId)> = HashMap::new();

    for &(tx, ty) in &active {
        let cell = get_liquid_from_map(world_map, tx, ty, config);

        if cell.is_empty() {
            sleep.mark_stable(tx, ty);
            continue;
        }

        // Solid tile with liquid inside: clear immediately.
        if is_solid_at(world_map, tile_registry, tx, ty, config) {
            set_liquid_in_map(world_map, tx, ty, LiquidCell::EMPTY, config);
            mark_dirty(tx, ty, config, dirty_chunks, dirty_liquid, sleep);
            continue;
        }

        let viscosity_factor = liquid_registry.viscosity(cell.liquid_type).recip().min(1.0);

        let mut remaining = cell.level;
        let mut flowed = false;

        // Helper: can we flow into this neighbor?
        let can_flow = |neighbor: LiquidCell, lt: LiquidId| -> bool {
            neighbor.is_empty() || neighbor.liquid_type == lt
        };

        let dest_level = |neighbor: LiquidCell, lt: LiquidId| -> f32 {
            if neighbor.liquid_type == lt {
                neighbor.level
            } else {
                0.0
            }
        };

        // ---- 1. Flow Down ----
        let (bx, by) = (tx, ty - 1);
        if !is_solid_at(world_map, tile_registry, bx, by, config) {
            let below = get_liquid_from_map(world_map, bx, by, config);
            if can_flow(below, cell.liquid_type) {
                let dest = dest_level(below, cell.liquid_type);
                let mut flow = vertical_flow_target(remaining, dest) - dest;
                flow = constrain_flow(flow, remaining);
                if flow > MIN_FLOW {
                    flow *= FLOW_SPEED * viscosity_factor;
                }
                if flow > 0.0 {
                    add_diff(&mut diffs, tx, ty, -flow, cell.liquid_type);
                    add_diff(&mut diffs, bx, by, flow, cell.liquid_type);
                    remaining -= flow;
                    flowed = true;
                }
                if remaining < MIN_LEVEL {
                    if flowed {
                        sleep.mark_changed(tx, ty);
                    }
                    continue;
                }
            }
        }

        // ---- 2. Flow Left ----
        let (lx, ly) = (tx - 1, ty);
        if !is_solid_at(world_map, tile_registry, lx, ly, config) {
            let left = get_liquid_from_map(world_map, lx, ly, config);
            if can_flow(left, cell.liquid_type) {
                let dest = dest_level(left, cell.liquid_type);
                let mut flow = (remaining - dest) / 4.0;
                flow = constrain_flow(flow, remaining);
                if flow > MIN_FLOW {
                    flow *= FLOW_SPEED * viscosity_factor;
                }
                if flow > 0.0 {
                    add_diff(&mut diffs, tx, ty, -flow, cell.liquid_type);
                    add_diff(&mut diffs, lx, ly, flow, cell.liquid_type);
                    remaining -= flow;
                    flowed = true;
                }
                if remaining < MIN_LEVEL {
                    if flowed {
                        sleep.mark_changed(tx, ty);
                    }
                    continue;
                }
            }
        }

        // ---- 3. Flow Right ----
        let (rx, ry) = (tx + 1, ty);
        if !is_solid_at(world_map, tile_registry, rx, ry, config) {
            let right = get_liquid_from_map(world_map, rx, ry, config);
            if can_flow(right, cell.liquid_type) {
                let dest = dest_level(right, cell.liquid_type);
                // /3 (not /4) to compensate for left-to-right sweep bias.
                let mut flow = (remaining - dest) / 3.0;
                flow = constrain_flow(flow, remaining);
                if flow > MIN_FLOW {
                    flow *= FLOW_SPEED * viscosity_factor;
                }
                if flow > 0.0 {
                    add_diff(&mut diffs, tx, ty, -flow, cell.liquid_type);
                    add_diff(&mut diffs, rx, ry, flow, cell.liquid_type);
                    remaining -= flow;
                    flowed = true;
                }
                if remaining < MIN_LEVEL {
                    if flowed {
                        sleep.mark_changed(tx, ty);
                    }
                    continue;
                }
            }
        }

        // ---- 4. Flow Up ----
        // Only happens when cell is over-full (compression pushes excess up).
        let (ux, uy) = (tx, ty + 1);
        if !is_solid_at(world_map, tile_registry, ux, uy, config) {
            let top = get_liquid_from_map(world_map, ux, uy, config);
            if can_flow(top, cell.liquid_type) {
                let dest = dest_level(top, cell.liquid_type);
                let mut flow = remaining - vertical_flow_target(remaining, dest);
                flow = constrain_flow(flow, remaining);
                if flow > MIN_FLOW {
                    flow *= FLOW_SPEED * viscosity_factor;
                }
                if flow > 0.0 {
                    add_diff(&mut diffs, tx, ty, -flow, cell.liquid_type);
                    add_diff(&mut diffs, ux, uy, flow, cell.liquid_type);
                    // remaining -= flow; // last direction, not needed
                    flowed = true;
                }
            }
        }

        if flowed {
            sleep.mark_changed(tx, ty);
        } else {
            sleep.mark_stable(tx, ty);
        }
    }

    // --- Apply diffs to WorldMap ---
    for (&(tx, ty), &(delta, ltype)) in &diffs {
        if delta.abs() < f32::EPSILON {
            continue;
        }
        let current = get_liquid_from_map(world_map, tx, ty, config);
        let new_level = current.level + delta;

        let new_cell = if new_level < MIN_LEVEL {
            LiquidCell::EMPTY
        } else {
            let new_type = if current.is_empty() {
                ltype
            } else {
                current.liquid_type
            };
            LiquidCell {
                liquid_type: new_type,
                level: new_level,
            }
        };

        set_liquid_in_map(world_map, tx, ty, new_cell, config);
        mark_dirty(tx, ty, config, dirty_chunks, dirty_liquid, sleep);
    }
}

/// Accumulate a delta into the diffs map, tracking liquid type for
/// propagation to empty cells.
fn add_diff(
    diffs: &mut HashMap<(i32, i32), (f32, LiquidId)>,
    tx: i32,
    ty: i32,
    delta: f32,
    ltype: LiquidId,
) {
    let entry = diffs.entry((tx, ty)).or_insert((0.0, ltype));
    entry.0 += delta;
    // Keep the type from positive (incoming) deltas for type propagation.
    if delta > 0.0 {
        entry.1 = ltype;
    }
}

// ---------------------------------------------------------------------------
// Pass 2: Reactions
// ---------------------------------------------------------------------------

/// Check for reactions between adjacent cells of different liquid types.
#[allow(clippy::too_many_arguments)]
fn run_reactions(
    config: &ActiveWorld,
    tile_registry: &TileRegistry,
    liquid_registry: &LiquidRegistry,
    world_map: &mut WorldMap,
    sleep: &mut SleepTracker,
    dirty_chunks: &mut DirtyChunks,
    dirty_liquid: &mut DirtyLiquidChunks,
    produced_tiles: &mut Vec<(i32, i32)>,
) {
    let active: Vec<(i32, i32)> = sleep.active_tiles().collect();
    let mut consumed: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();

    for &(tx, ty) in &active {
        if consumed.contains(&(tx, ty)) {
            continue;
        }
        let cell = get_liquid_from_map(world_map, tx, ty, config);
        if cell.is_empty() {
            continue;
        }

        // Check 4 neighbors for different-type adjacency.
        for &(dx, dy) in &[(0i32, -1i32), (1, 0), (0, 1), (-1, 0)] {
            let (nx, ny) = (tx + dx, ty + dy);
            if consumed.contains(&(nx, ny)) {
                continue;
            }

            let neighbor = get_liquid_from_map(world_map, nx, ny, config);
            if neighbor.is_empty() || neighbor.liquid_type == cell.liquid_type {
                continue;
            }

            if let Some(reaction) =
                liquid_registry.get_reaction(cell.liquid_type, neighbor.liquid_type)
            {
                // Produce tile at the reaction site (the neighbor cell).
                if let Some(tile_name) = &reaction.produce_tile {
                    let produced_tile = tile_registry
                        .try_by_name(tile_name)
                        .unwrap_or(crate::registry::tile::TileId::AIR);
                    if produced_tile != crate::registry::tile::TileId::AIR {
                        let wtx = config.wrap_tile_x(nx);
                        let (cx, cy) = chunk::tile_to_chunk(wtx, ny, config.chunk_size);
                        let (lx, ly) = chunk::tile_to_local(wtx, ny, config.chunk_size);
                        if let Some(chunk_data) = world_map.chunk_mut(cx, cy) {
                            chunk_data.fg.set(lx, ly, produced_tile, config.chunk_size);
                        }
                        dirty_chunks.0.insert((cx, cy));
                        produced_tiles.push((nx, ny));
                    }
                }

                if reaction.consume_both {
                    set_liquid_in_map(world_map, tx, ty, LiquidCell::EMPTY, config);
                    set_liquid_in_map(world_map, nx, ny, LiquidCell::EMPTY, config);
                    consumed.insert((tx, ty));
                    consumed.insert((nx, ny));
                    mark_dirty(tx, ty, config, dirty_chunks, dirty_liquid, sleep);
                    mark_dirty(nx, ny, config, dirty_chunks, dirty_liquid, sleep);
                    break; // cell consumed, stop checking neighbors
                } else {
                    // Only consume the neighbor.
                    set_liquid_in_map(world_map, nx, ny, LiquidCell::EMPTY, config);
                    consumed.insert((nx, ny));
                    mark_dirty(nx, ny, config, dirty_chunks, dirty_liquid, sleep);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pass 3: Density sorting
// ---------------------------------------------------------------------------

/// Swap adjacent cells where a lighter liquid sits below a denser one.
/// This ensures correct vertical stratification (e.g., oil floats on water).
#[allow(clippy::too_many_arguments)]
fn run_density_sort(
    config: &ActiveWorld,
    tile_registry: &TileRegistry,
    liquid_registry: &LiquidRegistry,
    world_map: &mut WorldMap,
    sleep: &mut SleepTracker,
    dirty_chunks: &mut DirtyChunks,
    dirty_liquid: &mut DirtyLiquidChunks,
) {
    let active: Vec<(i32, i32)> = sleep.active_tiles().collect();

    // Collect unique columns from active tiles.
    let mut col_ranges: HashMap<i32, (i32, i32)> = HashMap::new();
    for &(tx, ty) in &active {
        let entry = col_ranges.entry(tx).or_insert((ty, ty));
        entry.0 = entry.0.min(ty);
        entry.1 = entry.1.max(ty);
    }

    for (&tx, &(y_min, y_max)) in &col_ranges {
        for y in y_min..y_max {
            if is_solid_at(world_map, tile_registry, tx, y, config)
                || is_solid_at(world_map, tile_registry, tx, y + 1, config)
            {
                continue;
            }

            let bottom = get_liquid_from_map(world_map, tx, y, config);
            let top = get_liquid_from_map(world_map, tx, y + 1, config);

            if bottom.is_empty() || top.is_empty() {
                continue;
            }
            if bottom.liquid_type == top.liquid_type {
                continue;
            }

            let d_bottom = liquid_registry.density(bottom.liquid_type);
            let d_top = liquid_registry.density(top.liquid_type);

            if d_bottom < d_top {
                // Lighter below denser — swap cells.
                set_liquid_in_map(world_map, tx, y, top, config);
                set_liquid_in_map(world_map, tx, y + 1, bottom, config);

                for sy in [y, y + 1] {
                    mark_dirty(tx, sy, config, dirty_chunks, dirty_liquid, sleep);
                }
            }
        }
    }
}
