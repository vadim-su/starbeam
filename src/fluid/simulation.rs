use bevy::prelude::*;

use crate::fluid::cell::{FluidCell, FluidId};
use crate::fluid::fluid_world::FluidWorld;

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
    /// Simulation ticks per second (default 20 = like Minecraft).
    pub tick_rate: f32,
    /// Max ticks per frame to prevent death spiral (default 3).
    pub max_ticks_per_frame: u32,
    pub min_mass: f32,
    pub min_flow: f32,
    pub max_speed: f32,
}

impl Default for FluidSimConfig {
    fn default() -> Self {
        Self {
            tick_rate: 60.0,
            max_ticks_per_frame: 4,
            min_mass: MIN_MASS,
            min_flow: MIN_FLOW,
            max_speed: 2.0,
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

// ---------------------------------------------------------------------------
// Global simulation using FluidWorld (replaces per-chunk simulate_grid)
// ---------------------------------------------------------------------------

/// Run one tick of the fluid simulation on all active cells globally.
///
/// Uses `FluidWorld` for seamless cross-chunk addressing. The snapshot taken
/// at `FluidWorld::new` provides consistent reads while writes go to live data.
///
/// `tick_parity` alternates scan direction: even=L→R, odd=R→L to reduce
/// directional bias.
pub fn simulate_tick(
    world: &mut FluidWorld,
    active_chunks: &[(i32, i32)],
    config: &FluidSimConfig,
    tick_parity: u32,
) {
    let cs = world.chunk_size as i32;

    for &(cx, cy) in active_chunks {
        let base_gx = cx * cs;
        let base_gy = cy * cs;

        for ly in 0..cs {
            let gy = base_gy + ly;
            let x_iter: Box<dyn Iterator<Item = i32>> = if tick_parity % 2 == 0 {
                Box::new(0..cs)
            } else {
                Box::new((0..cs).rev())
            };

            for lx in x_iter {
                let gx = base_gx + lx;
                let cell = world.read(gx, gy);
                if cell.is_empty() {
                    continue;
                }

                let def = world.fluid_registry.get(cell.fluid_id);
                let max_speed = config.max_speed * (1.0 - def.viscosity).max(0.1);
                let remaining = world.read_current(gx, gy).mass;

                if def.is_gas {
                    // Gas: flow UP first (primary), then horizontal, then DOWN (decompression)
                    let remaining = flow_vertical(
                        world,
                        gx,
                        gy,
                        1,
                        true,
                        remaining,
                        cell.fluid_id,
                        def.max_compress,
                        max_speed,
                        config.min_flow,
                    );
                    let remaining = flow_horizontal(
                        world,
                        gx,
                        gy,
                        remaining,
                        cell.fluid_id,
                        cell.mass,
                        max_speed,
                        config.min_flow,
                    );
                    flow_vertical(
                        world,
                        gx,
                        gy,
                        -1,
                        false,
                        remaining,
                        cell.fluid_id,
                        def.max_compress,
                        max_speed,
                        config.min_flow,
                    );
                } else {
                    // Liquid: flow DOWN first (primary), then horizontal (if supported), then UP
                    let remaining = flow_vertical(
                        world,
                        gx,
                        gy,
                        -1,
                        true,
                        remaining,
                        cell.fluid_id,
                        def.max_compress,
                        max_speed,
                        config.min_flow,
                    );
                    // Only spread horizontally when supported from below (solid
                    // tile or existing fluid). In freefall the liquid drops
                    // straight down instead of flattening into a pancake.
                    let supported_below = world.is_solid(gx, gy - 1) || {
                        let below = world.read_current(gx, gy - 1);
                        !below.is_empty()
                    };
                    let remaining = if supported_below {
                        flow_horizontal(
                            world,
                            gx,
                            gy,
                            remaining,
                            cell.fluid_id,
                            cell.mass,
                            max_speed,
                            config.min_flow,
                        )
                    } else {
                        remaining
                    };
                    flow_vertical(
                        world,
                        gx,
                        gy,
                        1,
                        false,
                        remaining,
                        cell.fluid_id,
                        def.max_compress,
                        max_speed,
                        config.min_flow,
                    );
                }
            }
        }
    }

    // Cleanup: remove cells with negligible mass
    for &(cx, cy) in active_chunks {
        if let Some(chunk) = world.world_map.chunks.get_mut(&(cx, cy)) {
            for cell in chunk.fluids.iter_mut() {
                if cell.mass > 0.0 && cell.mass < config.min_mass {
                    *cell = FluidCell::EMPTY;
                }
            }
        }
    }
}

/// Try to flow vertically using global coordinates.
///
/// `dy` is -1 (down) or +1 (up). `is_primary` indicates whether this is the
/// primary flow direction (down for liquids, up for gases) vs decompression.
/// Returns remaining mass.
#[allow(clippy::too_many_arguments)]
fn flow_vertical(
    world: &mut FluidWorld,
    gx: i32,
    gy: i32,
    dy: i32,
    is_primary: bool,
    remaining: f32,
    fluid_id: FluidId,
    max_compress: f32,
    max_speed: f32,
    min_flow: f32,
) -> f32 {
    if remaining <= 0.0 {
        return 0.0;
    }

    let ny = gy + dy;
    if world.is_solid(gx, ny) {
        return remaining;
    }

    // Snapshot check: can't mix different fluid types
    let neighbor = world.read(gx, ny);
    if !neighbor.is_empty() && neighbor.fluid_id != fluid_id {
        return remaining;
    }

    // Live-state check: another cell may have already claimed this target
    let current_neighbor = world.read_current(gx, ny);
    if current_neighbor.fluid_id != FluidId::NONE && current_neighbor.fluid_id != fluid_id {
        return remaining;
    }

    let neighbor_mass = current_neighbor.mass;
    let total = remaining + neighbor_mass;

    let flow = if is_primary {
        get_stable_state(total, max_compress) - neighbor_mass
    } else {
        // Decompression: only compressed fluid flows in this direction
        if remaining <= MAX_MASS {
            return remaining;
        }
        remaining - get_stable_state(total, max_compress)
    };

    if flow <= 0.0 {
        return remaining;
    }

    let mut flow = flow;
    // Smooth small flows
    if flow > min_flow {
        flow *= 0.5;
    }
    flow = flow.min(max_speed).min(remaining).max(0.0);

    if flow <= 0.0 {
        return remaining;
    }

    world.sub_mass(gx, gy, flow);
    world.add_mass(gx, ny, fluid_id, flow);
    remaining - flow
}

/// Try to flow horizontally (left and right) using global coordinates.
/// Returns remaining mass.
#[allow(clippy::too_many_arguments)]
fn flow_horizontal(
    world: &mut FluidWorld,
    gx: i32,
    gy: i32,
    mut remaining: f32,
    fluid_id: FluidId,
    original_mass: f32,
    max_speed: f32,
    min_flow: f32,
) -> f32 {
    remaining = flow_side(
        world,
        gx,
        gy,
        gx - 1,
        remaining,
        fluid_id,
        original_mass,
        max_speed,
        min_flow,
    );
    remaining = flow_side(
        world,
        gx,
        gy,
        gx + 1,
        remaining,
        fluid_id,
        original_mass,
        max_speed,
        min_flow,
    );
    remaining
}

/// Try to flow to a single horizontal neighbor using global coordinates.
/// Uses equalization: flow = (original_mass - neighbor_mass) / 4.
#[allow(clippy::too_many_arguments)]
fn flow_side(
    world: &mut FluidWorld,
    gx: i32,
    gy: i32,
    ngx: i32,
    remaining: f32,
    fluid_id: FluidId,
    original_mass: f32,
    max_speed: f32,
    min_flow: f32,
) -> f32 {
    if remaining <= 0.0 {
        return 0.0;
    }

    if world.is_solid(ngx, gy) {
        return remaining;
    }

    let neighbor = world.read(ngx, gy);
    if !neighbor.is_empty() && neighbor.fluid_id != fluid_id {
        return remaining;
    }

    let current_neighbor = world.read_current(ngx, gy);
    if current_neighbor.fluid_id != FluidId::NONE && current_neighbor.fluid_id != fluid_id {
        return remaining;
    }

    let mut flow = (original_mass - world.read(ngx, gy).mass) / 4.0;
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

    world.sub_mass(gx, gy, flow);
    world.add_mass(ngx, gy, fluid_id, flow);
    remaining - flow
}
