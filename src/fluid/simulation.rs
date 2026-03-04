use std::collections::HashSet;

use bevy::prelude::*;

use super::active::ActiveFluids;
use super::cell::{FluidCell, FluidId};
use super::definition::FluidRegistry;
use crate::world::chunk::{Layer, WorldMap};
use crate::world::ctx::WorldCtx;

/// Fluid simulation fixed timestep: 20 ticks/second.
const FLUID_TICK_INTERVAL: f32 = 1.0 / 20.0;

/// Maximum tiles processed per tick to stay within performance budget.
const MAX_TILES_PER_TICK: usize = 4096;

/// Timer resource to control fixed-timestep fluid ticks.
#[derive(Resource)]
pub struct FluidTickTimer(pub Timer);

impl Default for FluidTickTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(FLUID_TICK_INTERVAL, TimerMode::Repeating))
    }
}

/// A delta to apply to the world after processing all active tiles.
struct FluidDelta {
    pos: (i32, i32),
    new_cell: FluidCell,
}

/// Main fluid simulation system. Runs on fixed timestep in GameSet::Physics.
pub fn fluid_simulation_system(
    time: Res<Time>,
    mut timer: ResMut<FluidTickTimer>,
    mut active_fluids: ResMut<ActiveFluids>,
    mut world_map: ResMut<WorldMap>,
    ctx: WorldCtx,
    fluid_registry: Res<FluidRegistry>,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }

    let ctx_ref = ctx.as_ref();

    // Swap pending → current
    active_fluids.swap_pending();

    if active_fluids.current.is_empty() {
        return;
    }

    // Collect tiles to process (cap at budget)
    let tiles_to_process: Vec<(i32, i32)> = active_fluids
        .current
        .iter()
        .copied()
        .take(MAX_TILES_PER_TICK)
        .collect();

    let mut deltas: Vec<FluidDelta> = Vec::new();
    let mut woken: HashSet<(i32, i32)> = HashSet::new();

    for (tx, ty) in &tiles_to_process {
        let tx = *tx;
        let ty = *ty;

        // Skip solid tiles — no fluid in solids
        if world_map.is_solid(tx, ty, &ctx_ref) {
            continue;
        }

        let Some(cell) = world_map.get_fluid(tx, ty, &ctx_ref) else {
            continue;
        };
        if cell.is_empty() {
            continue;
        }

        let flow_rate = fluid_registry.get(cell.fluid_id).flow_rate as i32;
        let mut my_level = cell.level as i32;

        // 1. Flow down (gravity) — lower Y means downward in screen coords
        //    In this engine, Y increases upward, so "below" = ty - 1
        let below_y = ty - 1;
        if below_y >= 0 {
            if !world_map.is_solid(tx, below_y, &ctx_ref) {
                let below_fluid = world_map
                    .get_fluid(tx, below_y, &ctx_ref)
                    .unwrap_or_default();

                let can_flow = below_fluid.fluid_id == FluidId::NONE
                    || below_fluid.fluid_id == cell.fluid_id;

                if can_flow {
                    let space = 255 - below_fluid.level as i32;
                    if space > 0 {
                        let transfer = my_level.min(space).min(flow_rate);
                        if transfer > 0 {
                            deltas.push(FluidDelta {
                                pos: (tx, below_y),
                                new_cell: FluidCell {
                                    fluid_id: cell.fluid_id,
                                    level: (below_fluid.level as i32 + transfer) as u8,
                                },
                            });
                            my_level -= transfer;
                            woken.insert((tx, below_y));
                        }
                    }
                }
            }
        }

        // 2. Equalize sideways (only if blocked below or remaining fluid)
        if my_level > 0 {
            for dx in [-1i32, 1] {
                let nx = tx + dx;
                if world_map.is_solid(nx, ty, &ctx_ref) {
                    continue;
                }

                let neighbor = world_map
                    .get_fluid(nx, ty, &ctx_ref)
                    .unwrap_or_default();

                let can_flow =
                    neighbor.fluid_id == FluidId::NONE || neighbor.fluid_id == cell.fluid_id;

                if !can_flow {
                    continue;
                }

                let diff = my_level - neighbor.level as i32;
                if diff > 1 {
                    let transfer = (diff / 3).max(1).min(flow_rate);
                    if transfer > 0 {
                        deltas.push(FluidDelta {
                            pos: (nx, ty),
                            new_cell: FluidCell {
                                fluid_id: cell.fluid_id,
                                level: (neighbor.level as i32 + transfer) as u8,
                            },
                        });
                        my_level -= transfer;
                        woken.insert((nx, ty));
                    }
                }
            }
        }

        // Update self if level changed
        if my_level != cell.level as i32 {
            deltas.push(FluidDelta {
                pos: (tx, ty),
                new_cell: FluidCell {
                    fluid_id: if my_level <= 0 {
                        FluidId::NONE
                    } else {
                        cell.fluid_id
                    },
                    level: my_level.max(0) as u8,
                },
            });
            woken.insert((tx, ty));
        }
    }

    // Track which tiles changed for sleep/wake
    let mut changed_tiles: HashSet<(i32, i32)> = HashSet::new();

    // Apply all deltas
    for delta in &deltas {
        world_map.set_fluid(delta.pos.0, delta.pos.1, delta.new_cell, &ctx_ref);
        changed_tiles.insert(delta.pos);
    }

    // Wake neighbors of changed tiles
    for pos in &woken {
        active_fluids.wake_with_neighbors(pos.0, pos.1);
    }

    // Handle sleep/wake for processed tiles
    let mut to_sleep = Vec::new();
    for pos in &tiles_to_process {
        if changed_tiles.contains(pos) {
            active_fluids.reset_settle(*pos);
        } else if active_fluids.tick_settle(*pos) {
            to_sleep.push(*pos);
        }
    }
    for pos in to_sleep {
        active_fluids.sleep(pos);
    }

    // Remove processed tiles from current (they'll be re-added via pending if still active)
    active_fluids.current.clear();
}

/// Wake adjacent fluid tiles when a block is broken at (tile_x, tile_y).
pub fn wake_adjacent_fluids(
    tile_x: i32,
    tile_y: i32,
    active_fluids: &mut ActiveFluids,
    world_map: &WorldMap,
    ctx: &crate::world::ctx::WorldCtxRef,
) {
    for (dx, dy) in [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)] {
        let nx = tile_x + dx;
        let ny = tile_y + dy;
        if let Some(cell) = world_map.get_fluid(nx, ny, ctx) {
            if !cell.is_empty() {
                active_fluids.wake(nx, ny);
            }
        }
    }
}

/// Wake fluid tiles at chunk boundaries when a chunk is loaded.
pub fn wake_chunk_boundary_fluids(
    chunk_x: i32,
    chunk_y: i32,
    world_map: &WorldMap,
    active_fluids: &mut ActiveFluids,
    ctx: &crate::world::ctx::WorldCtxRef,
) {
    let cs = ctx.config.chunk_size as i32;
    let base_x = chunk_x * cs;
    let base_y = chunk_y * cs;

    for i in 0..cs {
        // Left edge
        wake_if_fluid(base_x, base_y + i, world_map, active_fluids, ctx);
        // Right edge
        wake_if_fluid(base_x + cs - 1, base_y + i, world_map, active_fluids, ctx);
        // Bottom edge
        wake_if_fluid(base_x + i, base_y, world_map, active_fluids, ctx);
        // Top edge
        wake_if_fluid(base_x + i, base_y + cs - 1, world_map, active_fluids, ctx);
    }
}

fn wake_if_fluid(
    x: i32,
    y: i32,
    world_map: &WorldMap,
    active_fluids: &mut ActiveFluids,
    ctx: &crate::world::ctx::WorldCtxRef,
) {
    if let Some(cell) = world_map.get_fluid(x, y, ctx) {
        if !cell.is_empty() {
            active_fluids.wake_with_neighbors(x, y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fluid_tick_timer_defaults_to_20hz() {
        let timer = FluidTickTimer::default();
        assert!((timer.0.duration().as_secs_f32() - FLUID_TICK_INTERVAL).abs() < 0.001);
    }
}
