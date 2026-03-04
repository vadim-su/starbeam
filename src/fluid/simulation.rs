use std::collections::HashSet;

use bevy::prelude::*;

use crate::fluid::cell::{FluidCell, FluidId};
use crate::fluid::registry::FluidRegistry;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::{tile_to_chunk, tile_to_local, LoadedChunks, WorldMap};

/// Configuration for the fluid CA simulation.
#[derive(Resource)]
pub struct FluidSimConfig {
    /// Simulation ticks per second.
    pub sim_rate: f32,
    /// Cells below this mass are cleared to empty.
    pub min_mass: f32,
    /// Global flow speed multiplier.
    pub flow_speed: f32,
    /// Maximum simulation steps per frame (prevents spiral of death).
    pub max_steps_per_frame: u32,
}

impl Default for FluidSimConfig {
    fn default() -> Self {
        Self {
            sim_rate: 15.0,
            min_mass: 0.01,
            flow_speed: 1.0,
            max_steps_per_frame: 4,
        }
    }
}

/// Runtime state for the fluid simulation.
#[derive(Resource, Default)]
pub struct FluidSimState {
    pub accumulator: f32,
    pub tick: u64,
}

/// Set of data-chunk coordinates whose fluid data changed this frame.
#[derive(Resource, Default)]
pub struct DirtyFluidChunks(pub HashSet<(i32, i32)>);

/// Read a fluid cell from a specific world tile position using direct chunk access.
fn read_fluid(
    world_map: &WorldMap,
    tile_x: i32,
    tile_y: i32,
    chunk_size: u32,
) -> FluidCell {
    let (cx, cy) = tile_to_chunk(tile_x, tile_y, chunk_size);
    let (lx, ly) = tile_to_local(tile_x, tile_y, chunk_size);
    world_map
        .chunks
        .get(&(cx, cy))
        .map(|c| c.fluids[(ly * chunk_size + lx) as usize])
        .unwrap_or(FluidCell::EMPTY)
}

/// Check if a world tile is solid (non-air foreground tile) using direct chunk access.
fn is_tile_solid(
    world_map: &WorldMap,
    tile_x: i32,
    tile_y: i32,
    chunk_size: u32,
) -> bool {
    let (cx, cy) = tile_to_chunk(tile_x, tile_y, chunk_size);
    let (lx, ly) = tile_to_local(tile_x, tile_y, chunk_size);
    world_map
        .chunks
        .get(&(cx, cy))
        .map(|c| {
            let idx = (ly * chunk_size + lx) as usize;
            c.fg.tiles[idx].0 != 0
        })
        .unwrap_or(true) // treat unloaded as solid (barrier)
}

/// Write a fluid cell to a specific world tile position using direct chunk access.
#[allow(dead_code)]
fn write_fluid(
    world_map: &mut WorldMap,
    tile_x: i32,
    tile_y: i32,
    cell: FluidCell,
    chunk_size: u32,
) {
    let (cx, cy) = tile_to_chunk(tile_x, tile_y, chunk_size);
    let (lx, ly) = tile_to_local(tile_x, tile_y, chunk_size);
    if let Some(chunk) = world_map.chunks.get_mut(&(cx, cy)) {
        chunk.fluids[(ly * chunk_size + lx) as usize] = cell;
    }
}

/// Compute how much mass to transfer from a fuller cell to a less-full neighbor.
fn transfer_mass(src_mass: f32, dst_mass: f32, max_flow: f32, dst_max: f32) -> f32 {
    let delta = (src_mass - dst_mass) * 0.5;
    let clamped = delta.clamp(0.0, max_flow);
    let space = (dst_max - dst_mass).max(0.0);
    clamped.min(space)
}

/// Pending fluid transfer to apply after the main pass.
struct Transfer {
    tile_x: i32,
    tile_y: i32,
    fluid_id: FluidId,
    amount: f32,
}

/// Run one CA tick across all loaded chunks.
fn run_tick(
    world_map: &mut WorldMap,
    loaded_chunks: &LoadedChunks,
    active_world: &ActiveWorld,
    fluid_registry: &FluidRegistry,
    config: &FluidSimConfig,
    dirty: &mut HashSet<(i32, i32)>,
) {
    let chunk_size = active_world.chunk_size;
    let cs = chunk_size as i32;

    // Collect unique data-chunk coords from loaded display chunks
    let data_chunks: Vec<(i32, i32)> = {
        let set: HashSet<(i32, i32)> = loaded_chunks
            .map
            .keys()
            .map(|&(dcx, cy)| (active_world.wrap_chunk_x(dcx), cy))
            .collect();
        set.into_iter().collect()
    };

    // Snapshot prev_mass before this tick
    for &(cx, cy) in &data_chunks {
        if let Some(chunk) = world_map.chunks.get_mut(&(cx, cy)) {
            for cell in &mut chunk.fluids {
                cell.prev_mass = cell.mass;
            }
        }
    }

    // Process each chunk
    let mut transfers: Vec<Transfer> = Vec::new();

    for &(cx, cy) in &data_chunks {
        let base_x = cx * cs;
        let base_y = cy * cs;

        // Take a snapshot of current fluid state for reading
        let fluids: Vec<FluidCell> = match world_map.chunks.get(&(cx, cy)) {
            Some(chunk) => chunk.fluids.clone(),
            None => continue,
        };

        let fg_tiles: Vec<u16> = match world_map.chunks.get(&(cx, cy)) {
            Some(chunk) => chunk.fg.tiles.iter().map(|t| t.0).collect(),
            None => continue,
        };

        let mut new_fluids = fluids.clone();
        let mut chunk_dirty = false;

        // Process bottom-to-top for liquids (gravity settles faster).
        // IMPORTANT: All flow decisions read from `fluids` (snapshot).
        // All writes go to `new_fluids` as deltas. This prevents cascading
        // where mass received from a neighbor in this tick immediately flows onward.
        for local_y in 0..chunk_size {
            for local_x in 0..chunk_size {
                let idx = (local_y * chunk_size + local_x) as usize;
                let cell = fluids[idx]; // Always read from SNAPSHOT

                if cell.is_empty() {
                    continue;
                }

                // Clear fluid in solid tiles
                if fg_tiles[idx] != 0 {
                    new_fluids[idx] = FluidCell::EMPTY;
                    chunk_dirty = true;
                    continue;
                }

                let def = fluid_registry.get(cell.fluid_id);
                let flow_rate = (1.0 - def.viscosity) * config.flow_speed;
                let max_mass = 1.0 + def.max_compress;

                let world_x = base_x + local_x as i32;
                let world_y = base_y + local_y as i32;

                // Track total outflow from this cell (applied as delta at the end)
                let mut outflow = 0.0f32;

                // === Primary direction: down for liquids, up for gases ===
                let primary_dy: i32 = if def.is_gas { 1 } else { -1 };
                let target_y = world_y + primary_dy;

                if cell.mass > config.min_mass
                    && target_y >= 0
                    && target_y < active_world.height_tiles
                {
                    let wrapped_x = active_world.wrap_tile_x(world_x);
                    let (tcx, tcy) = tile_to_chunk(wrapped_x, target_y, chunk_size);
                    let (tlx, tly) = tile_to_local(wrapped_x, target_y, chunk_size);
                    let tidx = (tly * chunk_size + tlx) as usize;
                    let same_chunk = tcx == cx && tcy == cy;

                    let dst_solid = if same_chunk {
                        fg_tiles[tidx] != 0
                    } else {
                        is_tile_solid(world_map, wrapped_x, target_y, chunk_size)
                    };

                    if !dst_solid {
                        // Read destination from SNAPSHOT
                        let dst = if same_chunk {
                            fluids[tidx]
                        } else {
                            read_fluid(world_map, wrapped_x, target_y, chunk_size)
                        };

                        if dst.is_empty() || dst.fluid_id == cell.fluid_id {
                            // For primary direction (gravity/buoyancy), move aggressively
                            let remaining = cell.mass;
                            let flow = (remaining - dst.mass)
                                .max(0.0)
                                .min(remaining)
                                .min((max_mass - dst.mass).max(0.0))
                                * flow_rate;

                            if flow > config.min_mass {
                                outflow += flow;
                                chunk_dirty = true;

                                if same_chunk {
                                    new_fluids[tidx].fluid_id = cell.fluid_id;
                                    new_fluids[tidx].mass += flow;
                                } else {
                                    transfers.push(Transfer {
                                        tile_x: wrapped_x,
                                        tile_y: target_y,
                                        fluid_id: cell.fluid_id,
                                        amount: flow,
                                    });
                                    dirty.insert((tcx, tcy));
                                }
                            }
                        }
                    }
                }

                // === Horizontal equalization ===
                let remaining_for_horiz = cell.mass - outflow;
                for dx in [-1i32, 1] {
                    if remaining_for_horiz <= config.min_mass {
                        break;
                    }

                    let nx = active_world.wrap_tile_x(world_x + dx);
                    let ny = world_y;

                    let (ncx, ncy) = tile_to_chunk(nx, ny, chunk_size);
                    let (nlx, nly) = tile_to_local(nx, ny, chunk_size);
                    let nidx = (nly * chunk_size + nlx) as usize;
                    let same_chunk = ncx == cx && ncy == cy;

                    let dst_solid = if same_chunk {
                        fg_tiles[nidx] != 0
                    } else {
                        is_tile_solid(world_map, nx, ny, chunk_size)
                    };

                    if dst_solid {
                        continue;
                    }

                    // Read destination from SNAPSHOT
                    let dst = if same_chunk {
                        fluids[nidx]
                    } else {
                        read_fluid(world_map, nx, ny, chunk_size)
                    };

                    if !dst.is_empty() && dst.fluid_id != cell.fluid_id {
                        continue;
                    }

                    let flow =
                        transfer_mass(remaining_for_horiz, dst.mass, flow_rate * 0.5, max_mass);

                    if flow > config.min_mass {
                        outflow += flow;
                        chunk_dirty = true;

                        if same_chunk {
                            new_fluids[nidx].fluid_id = cell.fluid_id;
                            new_fluids[nidx].mass += flow;
                        } else {
                            transfers.push(Transfer {
                                tile_x: nx,
                                tile_y: ny,
                                fluid_id: cell.fluid_id,
                                amount: flow,
                            });
                            dirty.insert((ncx, ncy));
                        }
                    }
                }

                // === Pressure: push upward for liquids if overfull ===
                let remaining_after_all = cell.mass - outflow;
                if !def.is_gas && remaining_after_all > 1.0 {
                    let up_y = world_y + 1;
                    if up_y < active_world.height_tiles {
                        let wrapped_x = active_world.wrap_tile_x(world_x);
                        let (ucx, ucy) = tile_to_chunk(wrapped_x, up_y, chunk_size);
                        let (ulx, uly) = tile_to_local(wrapped_x, up_y, chunk_size);
                        let uidx = (uly * chunk_size + ulx) as usize;
                        let same_chunk = ucx == cx && ucy == cy;

                        let dst_solid = if same_chunk {
                            fg_tiles[uidx] != 0
                        } else {
                            is_tile_solid(world_map, wrapped_x, up_y, chunk_size)
                        };

                        if !dst_solid {
                            // Read destination from SNAPSHOT
                            let dst = if same_chunk {
                                fluids[uidx]
                            } else {
                                read_fluid(world_map, wrapped_x, up_y, chunk_size)
                            };

                            if dst.is_empty() || dst.fluid_id == cell.fluid_id {
                                let excess = remaining_after_all - 1.0;
                                let space = (max_mass - dst.mass).max(0.0);
                                let flow = excess.min(space) * flow_rate;

                                if flow > config.min_mass {
                                    outflow += flow;
                                    chunk_dirty = true;

                                    if same_chunk {
                                        new_fluids[uidx].fluid_id = cell.fluid_id;
                                        new_fluids[uidx].mass += flow;
                                    } else {
                                        transfers.push(Transfer {
                                            tile_x: wrapped_x,
                                            tile_y: up_y,
                                            fluid_id: cell.fluid_id,
                                            amount: flow,
                                        });
                                        dirty.insert((ucx, ucy));
                                    }
                                }
                            }
                        }
                    }
                }

                // Apply total outflow as delta to source cell
                new_fluids[idx].mass -= outflow;
                if new_fluids[idx].mass <= config.min_mass {
                    new_fluids[idx] = FluidCell::EMPTY;
                    chunk_dirty = true;
                } else if outflow > 0.0 {
                    chunk_dirty = true;
                }
            }
        }

        // Write back modified fluids
        if chunk_dirty {
            if let Some(chunk) = world_map.chunks.get_mut(&(cx, cy)) {
                chunk.fluids = new_fluids;
            }
            dirty.insert((cx, cy));
        }
    }

    // Apply cross-chunk transfers
    for t in transfers {
        let (cx, cy) = tile_to_chunk(t.tile_x, t.tile_y, chunk_size);
        let (lx, ly) = tile_to_local(t.tile_x, t.tile_y, chunk_size);
        if let Some(chunk) = world_map.chunks.get_mut(&(cx, cy)) {
            let idx = (ly * chunk_size + lx) as usize;
            let cell = &mut chunk.fluids[idx];
            if cell.is_empty() {
                cell.fluid_id = t.fluid_id;
                cell.mass = t.amount;
                cell.prev_mass = 0.0;
            } else if cell.fluid_id == t.fluid_id {
                cell.mass += t.amount;
            }
        }
    }
}

/// Bevy system: runs fixed-timestep fluid simulation.
pub fn fluid_simulation_step(
    time: Res<Time>,
    mut world_map: ResMut<WorldMap>,
    loaded_chunks: Res<LoadedChunks>,
    active_world: Res<ActiveWorld>,
    fluid_registry: Option<Res<FluidRegistry>>,
    config: Res<FluidSimConfig>,
    mut state: ResMut<FluidSimState>,
    mut dirty: ResMut<DirtyFluidChunks>,
) {
    let Some(fluid_registry) = fluid_registry else {
        return;
    };
    if fluid_registry.is_empty() {
        return;
    }

    dirty.0.clear();

    let dt = time.delta_secs().min(crate::physics::MAX_DELTA_SECS);
    state.accumulator += dt;

    let step_dt = 1.0 / config.sim_rate;
    let mut steps = 0u32;

    while state.accumulator >= step_dt && steps < config.max_steps_per_frame {
        state.accumulator -= step_dt;
        steps += 1;
        state.tick += 1;

        run_tick(
            &mut world_map,
            &loaded_chunks,
            &active_world,
            &fluid_registry,
            &config,
            &mut dirty.0,
        );
    }

    // Clamp accumulator to prevent spiral of death
    if state.accumulator > step_dt * 2.0 {
        state.accumulator = step_dt;
    }
}

/// Fraction of time elapsed since last sim tick (0.0..1.0), for visual interpolation.
pub fn sim_interpolation_frac(state: &FluidSimState, config: &FluidSimConfig) -> f32 {
    let step_dt = 1.0 / config.sim_rate;
    (state.accumulator / step_dt).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::cell::FluidCell;

    #[test]
    fn transfer_mass_equalizes() {
        let t = transfer_mass(1.0, 0.0, 0.5, 1.02);
        assert!(t > 0.0);
        assert!(t <= 0.5);
    }

    #[test]
    fn transfer_mass_no_flow_when_equal() {
        let t = transfer_mass(0.5, 0.5, 0.5, 1.02);
        assert_eq!(t, 0.0);
    }

    #[test]
    fn transfer_mass_respects_dst_capacity() {
        // dst nearly full, src has more
        let t = transfer_mass(1.0, 0.99, 0.5, 1.0);
        assert!(t <= 0.01 + f32::EPSILON);
        assert!(t >= 0.0);
    }

    #[test]
    fn transfer_mass_no_negative() {
        // dst has more than src
        let t = transfer_mass(0.3, 0.8, 0.5, 1.0);
        assert_eq!(t, 0.0);
    }

    #[test]
    fn sim_frac_clamped() {
        let state = FluidSimState {
            accumulator: 0.05,
            tick: 10,
        };
        let config = FluidSimConfig::default();
        let frac = sim_interpolation_frac(&state, &config);
        assert!(frac >= 0.0 && frac <= 1.0);
    }

    #[test]
    fn empty_cell_not_processed() {
        let cell = FluidCell::EMPTY;
        assert!(cell.is_empty());
    }

    #[test]
    fn new_cell_has_matching_prev_mass() {
        let cell = FluidCell::new(FluidId(1), 0.75);
        assert_eq!(cell.mass, 0.75);
        assert_eq!(cell.prev_mass, 0.75);
    }
}
