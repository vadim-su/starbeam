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

const GRAVITY_SCALE: f32 = 0.15;
const GRAVITY_BIAS_DOWN: f32 = 6.0;
/// Must be > -1.0 so that a full cell (level=1.0) can push upward.
/// At -0.8, upward flow only starts when cell level > ~0.8 — water must be
/// nearly full before it pushes up, preventing tower formation.
const GRAVITY_BIAS_UP: f32 = -0.8;

const LIQUID_DT: f32 = 1.0 / 30.0;

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
            LIQUID_DT,
        );
        all_produced.extend(produced);
    }

    // Mark tile mesh entities dirty for any reaction-produced solid tiles.
    if !all_produced.is_empty() {
        rc_dirty.0 = true;
        for &(tx, ty) in &all_produced {
            let wtx = config.wrap_tile_x(tx);
            let (cx, cy) = chunk::tile_to_chunk(wtx, ty, config.chunk_size);
            // Mark fg+bg chunk entities as ChunkDirty for mesh rebuild.
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
/// Returns list of tile coords where reactions produced solid tiles (need mesh rebuild).
#[allow(clippy::too_many_arguments)]
fn run_liquid_step(
    config: &ActiveWorld,
    tile_registry: &TileRegistry,
    liquid_registry: &LiquidRegistry,
    world_map: &mut WorldMap,
    sleep: &mut SleepTracker,
    dirty_chunks: &mut DirtyChunks,
    dirty_liquid: &mut DirtyLiquidChunks,
    dt: f32,
) -> Vec<(i32, i32)> {
    // Tiles where reactions produced solid blocks (need mesh rebuild).
    let mut produced_tiles: Vec<(i32, i32)> = Vec::new();

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

        // Suppress horizontal flow if the cell below can actually accept THIS liquid.
        // This forces liquid to fall first before spreading sideways.
        // But don't suppress if the cell below has a denser *different* liquid
        // that would block downward flow anyway (e.g. oil sitting on water).
        let below_open = {
            let bx = tx;
            let by = ty - 1;
            if is_solid_at(world_map, tile_registry, bx, by, config) {
                false
            } else {
                let below = get_liquid_from_map(world_map, bx, by, config);
                if below.is_empty() {
                    true // empty cell below — liquid should fall first
                } else if below.liquid_type == cell.liquid_type {
                    below.level < 0.95 // same type — fall if there's room
                } else {
                    // Different liquid below: only "open" if we're denser
                    // (can displace it). If we're lighter, we can't go down,
                    // so horizontal flow should NOT be suppressed.
                    let density_me = liquid_registry.density(cell.liquid_type);
                    let density_below = liquid_registry.density(below.liquid_type);
                    density_me > density_below
                }
            }
        };

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

            // Gravity bias + horizontal damping.
            let (gravity_bias, damping) = match face {
                FACE_DOWN => (GRAVITY_BIAS_DOWN, 1.0),
                FACE_UP => (GRAVITY_BIAS_UP, 1.0),
                _ => {
                    // Ledge detection: is the cell below the neighbor open?
                    let below_neighbor_open =
                        !is_solid_at(world_map, tile_registry, nx, ny - 1, config) && {
                            let bn = get_liquid_from_map(world_map, nx, ny - 1, config);
                            bn.is_empty() || bn.level < 0.8
                        };
                    let ledge_bonus = if below_neighbor_open { 2.0 } else { 0.0 };

                    // When cell below can still accept liquid, dampen (but
                    // don't suppress) horizontal flow to prioritize falling.
                    let h_damp = if below_open { 0.15 } else { 1.0 };
                    (ledge_bonus, h_damp)
                }
            };

            let flow = dt * (pressure - n_pressure + gravity_bias) / viscosity * damping;
            if flow > 0.0 {
                // Limit by available room in destination to prevent mass destruction.
                let room = if neighbor.is_empty() {
                    MAX_LEVEL
                } else if neighbor.liquid_type == cell.liquid_type {
                    (MAX_LEVEL - neighbor.level).max(0.0)
                } else {
                    MAX_LEVEL // different type — displacement handles it
                };
                flows[face] = flow.min(MAX_FLOW).min(room);
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
    // Displacement: liquid pushed to another cell by density sorting.
    let mut displacements: Vec<(i32, i32, LiquidCell)> = Vec::new();

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

        // --- Different liquid types meeting: reactions & displacement ---
        if !current.is_empty()
            && incoming_type != LiquidId::NONE
            && incoming_type != current.liquid_type
            && net_delta > 0.0
        {
            // Check for reactions first (e.g., water + lava = stone).
            if let Some(reaction) = liquid_registry.get_reaction(current.liquid_type, incoming_type)
            {
                if let Some(tile_name) = &reaction.produce_tile {
                    let produced_tile = tile_registry
                        .try_by_name(tile_name)
                        .unwrap_or(crate::registry::tile::TileId::AIR);
                    if produced_tile != crate::registry::tile::TileId::AIR {
                        let wtx = config.wrap_tile_x(tx);
                        let (cx, cy) = chunk::tile_to_chunk(wtx, ty, config.chunk_size);
                        let (lx, ly) = chunk::tile_to_local(wtx, ty, config.chunk_size);
                        if let Some(chunk_data) = world_map.chunk_mut(cx, cy) {
                            chunk_data.fg.set(lx, ly, produced_tile, config.chunk_size);
                        }
                        dirty_chunks.0.insert((cx, cy));
                        produced_tiles.push((tx, ty));
                    }
                }
                if reaction.consume_both {
                    changes.push((tx, ty, LiquidCell::EMPTY));
                } else {
                    changes.push((tx, ty, current));
                }
                sleep.mark_changed(tx, ty);
                continue;
            }

            // No reaction — handle density displacement.
            // Denser liquid stays in this cell, lighter one is displaced UP.
            let density_incoming = liquid_registry.density(incoming_type);
            let density_current = liquid_registry.density(current.liquid_type);

            if density_incoming > density_current {
                // Incoming is denser: it takes over this cell, existing displaced UP.
                let incoming_amount = net_delta.min(MAX_LEVEL);
                // Displace existing liquid upward.
                displacements.push((tx, ty + 1, current));
                changes.push((
                    tx,
                    ty,
                    LiquidCell {
                        liquid_type: incoming_type,
                        level: incoming_amount,
                    },
                ));
            } else {
                // Incoming is lighter: it gets pushed UP, cell stays as-is.
                let displaced = LiquidCell {
                    liquid_type: incoming_type,
                    level: net_delta.min(MAX_LEVEL),
                };
                displacements.push((tx, ty + 1, displaced));
                // Cell keeps its current state (may have outgoing flow deltas
                // handled via same-type delta merge below if also present).
            }
            sleep.mark_changed(tx, ty);
            continue;
        }

        // --- Same liquid type or flowing into empty cell ---
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

    // -- Phase 4b: Apply displacement (lighter liquid pushed upward) --------
    for (tx, ty, displaced) in displacements {
        if ty < 0 || ty >= config.height_tiles {
            continue;
        }
        if is_solid_at(world_map, tile_registry, tx, ty, config) {
            continue; // can't displace into solid — liquid is lost
        }

        let existing = get_liquid_from_map(world_map, tx, ty, config);
        let new_cell = if existing.is_empty() {
            displaced
        } else if existing.liquid_type == displaced.liquid_type {
            // Same type — merge levels.
            LiquidCell {
                liquid_type: displaced.liquid_type,
                level: (existing.level + displaced.level).min(MAX_LEVEL),
            }
        } else {
            // Different type again — denser stays, lighter pushed further up.
            // For simplicity, place displaced here; the density swap pass
            // below will sort it in the next tick.
            let d_disp = liquid_registry.density(displaced.liquid_type);
            let d_exist = liquid_registry.density(existing.liquid_type);
            if d_disp <= d_exist {
                // Displaced is lighter or equal — it goes here, existing
                // stays below (already correct ordering).
                LiquidCell {
                    liquid_type: displaced.liquid_type,
                    level: displaced.level.min(MAX_LEVEL),
                }
            } else {
                // Displaced is denser than what's here — shouldn't happen
                // normally, but keep existing for safety.
                existing
            }
        };

        set_liquid_in_map(world_map, tx, ty, new_cell, config);

        let wx = config.wrap_tile_x(tx);
        let (cx, cy) = chunk::tile_to_chunk(wx, ty, config.chunk_size);
        dirty_chunks.0.insert((cx, cy));
        dirty_liquid.0.insert((cx, cy));
        sleep.mark_changed(tx, ty);
    }

    // -- Phase 5: Column density sorting ------------------------------------
    // For each active column, swap adjacent cells where lighter liquid is
    // below denser liquid. This ensures correct vertical stratification.
    {
        // Collect unique columns from active tiles.
        let mut col_ranges: std::collections::HashMap<i32, (i32, i32)> =
            std::collections::HashMap::new();
        for &(tx, ty) in &active {
            let entry = col_ranges.entry(tx).or_insert((ty, ty));
            entry.0 = entry.0.min(ty);
            entry.1 = entry.1.max(ty);
        }

        for (&tx, &(y_min, y_max)) in &col_ranges {
            // Scan upward through the column.
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
                        let wx = config.wrap_tile_x(tx);
                        let (cx, cy) = chunk::tile_to_chunk(wx, sy, config.chunk_size);
                        dirty_chunks.0.insert((cx, cy));
                        dirty_liquid.0.insert((cx, cy));
                        sleep.mark_changed(tx, sy);
                    }
                }
            }
        }
    }

    // -- Phase 6: Surface tension — consolidate tiny droplets ----------------
    // Only absorb a droplet if a neighbor has *significantly* more liquid
    // (at least 3× the droplet's level). This prevents chain-reaction
    // evaporation on staircases where every cell is small.
    // Truly isolated tiny drops (no same-type neighbor at all) evaporate.
    {
        const TENSION_THRESHOLD: f32 = 0.08;
        const NEIGHBOR_RATIO: f32 = 3.0;

        for &(tx, ty) in &active {
            let cell = get_liquid_from_map(world_map, tx, ty, config);
            if cell.is_empty() || cell.level >= TENSION_THRESHOLD {
                continue;
            }

            // Count same-type neighbors and find the largest one.
            let mut best_neighbor: Option<(i32, i32, f32)> = None;
            let mut same_type_neighbors = 0u32;
            for &(dx, dy) in &[(0i32, -1i32), (0, 1), (-1, 0), (1, 0)] {
                let nx = tx + dx;
                let ny = ty + dy;
                if is_solid_at(world_map, tile_registry, nx, ny, config) {
                    continue;
                }
                let neighbor = get_liquid_from_map(world_map, nx, ny, config);
                if neighbor.is_empty() || neighbor.liquid_type != cell.liquid_type {
                    continue;
                }
                same_type_neighbors += 1;
                if neighbor.level > best_neighbor.map_or(0.0, |b| b.2) {
                    best_neighbor = Some((nx, ny, neighbor.level));
                }
            }

            match best_neighbor {
                Some((nx, ny, n_level)) if n_level >= cell.level * NEIGHBOR_RATIO => {
                    // Transfer this cell's liquid to the much-larger neighbor.
                    let new_n_level = (n_level + cell.level).min(MAX_LEVEL);
                    set_liquid_in_map(
                        world_map,
                        nx,
                        ny,
                        LiquidCell {
                            liquid_type: cell.liquid_type,
                            level: new_n_level,
                        },
                        config,
                    );
                    set_liquid_in_map(world_map, tx, ty, LiquidCell::EMPTY, config);

                    for &(sx, sy) in &[(tx, ty), (nx, ny)] {
                        let wx = config.wrap_tile_x(sx);
                        let (cx, cy) = chunk::tile_to_chunk(wx, sy, config.chunk_size);
                        dirty_chunks.0.insert((cx, cy));
                        dirty_liquid.0.insert((cx, cy));
                        sleep.mark_changed(sx, sy);
                    }
                }
                _ if same_type_neighbors == 0 && cell.level < 0.03 => {
                    // Completely isolated tiny drop — evaporate.
                    set_liquid_in_map(world_map, tx, ty, LiquidCell::EMPTY, config);
                    let wx = config.wrap_tile_x(tx);
                    let (cx, cy) = chunk::tile_to_chunk(wx, ty, config.chunk_size);
                    dirty_chunks.0.insert((cx, cy));
                    dirty_liquid.0.insert((cx, cy));
                    sleep.mark_changed(tx, ty);
                }
                _ => {}
            }
        }
    }

    produced_tiles
}
