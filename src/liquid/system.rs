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
// Constants (matching simulation.rs)
// ---------------------------------------------------------------------------

const GRAVITY_SCALE: f32 = 0.1;
const GRAVITY_BIAS_DOWN: f32 = 2.0;
const GRAVITY_BIAS_UP: f32 = -1.0;

const LIQUID_DT: f32 = 1.0 / 20.0;

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

/// The main liquid simulation system. Runs at ~20 Hz via accumulator.
#[allow(clippy::too_many_arguments)]
pub fn liquid_simulation_system(
    time: Res<Time>,
    config: Res<ActiveWorld>,
    tile_registry: Res<TileRegistry>,
    liquid_registry: Res<LiquidRegistry>,
    mut world_map: ResMut<WorldMap>,
    mut sim_state: ResMut<LiquidSimState>,
    mut dirty_chunks: ResMut<DirtyChunks>,
    mut dirty_liquid: ResMut<DirtyLiquidChunks>,
) {
    if liquid_registry.defs.is_empty() {
        return;
    }

    sim_state.accumulator += time.delta_secs().min(0.1);

    while sim_state.accumulator >= LIQUID_DT {
        sim_state.accumulator -= LIQUID_DT;
        run_liquid_step(
            &config,
            &tile_registry,
            &liquid_registry,
            &mut world_map,
            &mut sim_state.sleep,
            &mut dirty_chunks,
            &mut dirty_liquid,
            LIQUID_DT,
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
    // Below world = bedrock (solid).
    if ty < 0 {
        return true;
    }
    // Above world = sky (not solid).
    if ty >= config.height_tiles {
        return false;
    }
    let wx = config.wrap_tile_x(tx);
    let (cx, cy) = chunk::tile_to_chunk(wx, ty, config.chunk_size);
    let (lx, ly) = chunk::tile_to_local(wx, ty, config.chunk_size);
    match world_map.chunk(cx, cy) {
        Some(chunk) => {
            let tile_id = chunk.fg.get(lx, ly, config.chunk_size);
            tile_registry.is_solid(tile_id)
        }
        // Unloaded chunk = treat as solid wall.
        None => true,
    }
}

/// Scan upward from (tx, ty+1) accumulating levels of the same liquid type.
fn compute_depth_above(
    world_map: &WorldMap,
    tx: i32,
    ty: i32,
    liquid_type: LiquidId,
    config: &ActiveWorld,
) -> f32 {
    let mut depth = 0.0_f32;
    let mut y = ty + 1;
    while y < config.height_tiles {
        let cell = get_liquid_from_map(world_map, tx, y, config);
        if cell.is_empty() || cell.liquid_type != liquid_type {
            break;
        }
        depth += cell.level;
        y += 1;
    }
    depth
}

// ---------------------------------------------------------------------------
// Simulation step
// ---------------------------------------------------------------------------

/// One simulation tick operating directly on WorldMap chunk data.
fn run_liquid_step(
    config: &ActiveWorld,
    tile_registry: &TileRegistry,
    liquid_registry: &LiquidRegistry,
    world_map: &mut WorldMap,
    sleep: &mut SleepTracker,
    dirty_chunks: &mut DirtyChunks,
    dirty_liquid: &mut DirtyLiquidChunks,
    dt: f32,
) {
    // Collect active tiles (snapshot to avoid borrow issues).
    let active: Vec<(i32, i32)> = sleep.active_tiles().collect();

    // Per-tile computed data: (tx, ty, pressure, flows[4]).
    let mut tile_data: Vec<(i32, i32, f32, [f32; 4])> = Vec::with_capacity(active.len());

    // -- Phase 1 & 2: Compute pressure and flows for each active tile -------
    for &(tx, ty) in &active {
        let cell = get_liquid_from_map(world_map, tx, ty, config);

        // Skip empty cells — mark stable so they can sleep.
        if cell.is_empty() {
            sleep.mark_stable(tx, ty);
            continue;
        }

        // Solid tile with liquid inside: clear the liquid.
        if is_solid_at(world_map, tile_registry, tx, ty, config) {
            sleep.mark_changed(tx, ty);
            tile_data.push((tx, ty, 0.0, [0.0; 4]));
            continue;
        }

        // Compute pressure from depth above.
        let density = liquid_registry.density(cell.liquid_type);
        let depth_above = compute_depth_above(world_map, tx, ty, cell.liquid_type, config);
        let pressure = cell.level + density * GRAVITY_SCALE * depth_above;

        let viscosity = liquid_registry.viscosity(cell.liquid_type).max(0.01);

        // Compute flows to 4 neighbors.
        let mut flows = [0.0_f32; 4];
        for face in 0..4 {
            let (dx, dy) = FACE_OFFSET[face];
            let nx = tx + dx;
            let ny = ty + dy;

            // Boundary / solid check.
            if is_solid_at(world_map, tile_registry, nx, ny, config) {
                continue;
            }

            let neighbor = get_liquid_from_map(world_map, nx, ny, config);

            // Block flow into a cell with a denser *different* liquid.
            if !neighbor.is_empty() && neighbor.liquid_type != cell.liquid_type {
                let density_a = liquid_registry.density(cell.liquid_type);
                let density_b = liquid_registry.density(neighbor.liquid_type);
                if density_a < density_b {
                    continue;
                }
            }

            // Neighbor pressure.
            let n_pressure = if neighbor.is_empty() {
                0.0
            } else {
                let n_density = liquid_registry.density(neighbor.liquid_type);
                let n_depth = compute_depth_above(world_map, nx, ny, neighbor.liquid_type, config);
                neighbor.level + n_density * GRAVITY_SCALE * n_depth
            };

            // Gravity bias.
            let gravity_bias = match face {
                FACE_DOWN => GRAVITY_BIAS_DOWN,
                FACE_UP => GRAVITY_BIAS_UP,
                _ => 0.0,
            };

            let flow = dt * (pressure - n_pressure + gravity_bias) / viscosity;
            if flow > 0.0 {
                flows[face] = flow.min(MAX_FLOW);
            }
        }

        // Clamp total outgoing to not exceed cell level.
        let total_out: f32 = flows.iter().copied().sum();
        if total_out > cell.level {
            let scale = cell.level / total_out;
            for f in &mut flows {
                *f *= scale;
            }
        }

        tile_data.push((tx, ty, pressure, flows));
    }

    // -- Phase 3: Collect changes -------------------------------------------
    let mut changes: Vec<(i32, i32, LiquidCell)> = Vec::new();

    // Track level deltas per tile coordinate.
    // Using a Vec of (tx, ty, delta, liquid_type) to avoid HashMap overhead.
    struct Delta {
        tx: i32,
        ty: i32,
        delta: f32,
        liquid_type: LiquidId,
    }
    let mut deltas: Vec<Delta> = Vec::new();

    for &(tx, ty, _pressure, flows) in &tile_data {
        let cell = get_liquid_from_map(world_map, tx, ty, config);

        // Solid tile that had liquid — clear it.
        if is_solid_at(world_map, tile_registry, tx, ty, config) {
            changes.push((tx, ty, LiquidCell::EMPTY));
            continue;
        }

        let has_flow = flows.iter().any(|&f| f > 0.0);
        if !has_flow {
            if !cell.is_empty() {
                sleep.mark_stable(tx, ty);
            }
            continue;
        }

        // Subtract outgoing from source.
        let total_out: f32 = flows.iter().copied().sum();
        deltas.push(Delta {
            tx,
            ty,
            delta: -total_out,
            liquid_type: cell.liquid_type,
        });

        // Add incoming to neighbors.
        for face in 0..4 {
            let out = flows[face];
            if out <= 0.0 {
                continue;
            }
            let (dx, dy) = FACE_OFFSET[face];
            let nx = tx + dx;
            let ny = ty + dy;
            deltas.push(Delta {
                tx: nx,
                ty: ny,
                delta: out,
                liquid_type: cell.liquid_type,
            });
        }
    }

    // Merge deltas by coordinate.
    // Sort by (tx, ty) so we can merge in a single pass.
    deltas.sort_unstable_by(|a, b| (a.tx, a.ty).cmp(&(b.tx, b.ty)));

    let mut i = 0;
    while i < deltas.len() {
        let tx = deltas[i].tx;
        let ty = deltas[i].ty;
        let mut net_delta = 0.0_f32;
        let mut incoming_type = deltas[i].liquid_type;

        // Merge all deltas for the same tile.
        while i < deltas.len() && deltas[i].tx == tx && deltas[i].ty == ty {
            net_delta += deltas[i].delta;
            // Use the liquid type from incoming flow (positive delta).
            if deltas[i].delta > 0.0 {
                incoming_type = deltas[i].liquid_type;
            }
            i += 1;
        }

        if net_delta.abs() < f32::EPSILON {
            continue;
        }

        let current = get_liquid_from_map(world_map, tx, ty, config);
        let mut new_level = current.level + net_delta;
        let new_type = if current.is_empty() {
            incoming_type
        } else {
            current.liquid_type
        };

        let new_cell = if new_level < MIN_LEVEL {
            LiquidCell::EMPTY
        } else {
            new_level = new_level.min(MAX_LEVEL);
            LiquidCell {
                liquid_type: new_type,
                level: new_level,
            }
        };

        changes.push((tx, ty, new_cell));
    }

    // -- Phase 4: Apply changes to WorldMap ---------------------------------
    for &(tx, ty, cell) in &changes {
        set_liquid_in_map(world_map, tx, ty, cell, config);

        // Mark chunk dirty for persistence and liquid mesh rendering.
        let wx = config.wrap_tile_x(tx);
        let (cx, cy) = chunk::tile_to_chunk(wx, ty, config.chunk_size);
        dirty_chunks.0.insert((cx, cy));
        dirty_liquid.0.insert((cx, cy));

        // Wake changed tiles and their neighbors.
        sleep.mark_changed(tx, ty);
    }
}
