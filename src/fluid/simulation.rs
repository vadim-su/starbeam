use bevy::prelude::*;

use crate::fluid::cell::{FluidCell, FluidId, FluidSlot};
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
                let snap = world.read(gx, gy);
                if snap.is_empty() {
                    continue;
                }

                // Process each non-empty slot independently
                let slots: [(FluidId, f32); 2] = [
                    (snap.primary.fluid_id, snap.primary.mass),
                    (snap.secondary.fluid_id, snap.secondary.mass),
                ];

                for &(fid, snap_mass) in &slots {
                    if fid == FluidId::NONE || snap_mass <= 0.0 {
                        continue;
                    }

                    let def = world.fluid_registry.get(fid);
                    let max_speed = config.max_speed * (1.0 - def.viscosity).max(0.1);
                    // Read remaining from current (live) state for this slot
                    let current_cell = world.read_current(gx, gy);
                    let remaining = current_cell
                        .slot_for(fid)
                        .map(|s| s.mass)
                        .unwrap_or(0.0);

                    if def.is_gas {
                        // Gas: flow UP first (primary), then horizontal, then DOWN (decompression)
                        let remaining = flow_vertical(
                            world,
                            gx,
                            gy,
                            1,
                            true,
                            remaining,
                            fid,
                            def.max_compress,
                            max_speed,
                            config.min_flow,
                        );
                        let remaining = flow_horizontal(
                            world,
                            gx,
                            gy,
                            remaining,
                            fid,
                            snap_mass,
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
                            fid,
                            def.max_compress,
                            max_speed,
                            config.min_flow,
                        );
                    } else {
                        // Liquid: flow DOWN first (primary), then horizontal, then UP (decompression)
                        let remaining = flow_vertical(
                            world,
                            gx,
                            gy,
                            -1,
                            true,
                            remaining,
                            fid,
                            def.max_compress,
                            max_speed,
                            config.min_flow,
                        );
                        let remaining = flow_horizontal(
                            world,
                            gx,
                            gy,
                            remaining,
                            fid,
                            snap_mass,
                            max_speed,
                            config.min_flow,
                        );
                        flow_vertical(
                            world,
                            gx,
                            gy,
                            1,
                            false,
                            remaining,
                            fid,
                            def.max_compress,
                            max_speed,
                            config.min_flow,
                        );
                    }
                }
            }
        }
    }

    // Cleanup: remove slots with negligible mass and normalize
    for &(cx, cy) in active_chunks {
        if let Some(chunk) = world.world_map.chunks.get_mut(&(cx, cy)) {
            for cell in chunk.fluids.iter_mut() {
                if cell.primary.mass > 0.0 && cell.primary.mass < config.min_mass {
                    cell.primary = FluidSlot::EMPTY;
                }
                if cell.secondary.mass > 0.0 && cell.secondary.mass < config.min_mass {
                    cell.secondary = FluidSlot::EMPTY;
                }
                cell.normalize();
            }
        }
    }
}

/// Check whether a cell can accept fluid of the given type.
/// Returns true if the cell is empty, already contains this fluid, or has a free secondary slot.
fn can_accept_fluid(cell: &FluidCell, fluid_id: FluidId) -> bool {
    if cell.is_empty() {
        return true;
    }
    if cell.has_fluid(fluid_id) {
        return true;
    }
    if cell.secondary.is_empty() {
        return true;
    }
    false
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

    // Snapshot check: can the neighbor accept this fluid?
    let neighbor = world.read(gx, ny);
    if !can_accept_fluid(&neighbor, fluid_id) {
        return remaining;
    }

    // Live-state check: can the neighbor still accept this fluid?
    let current_neighbor = world.read_current(gx, ny);
    if !can_accept_fluid(&current_neighbor, fluid_id) {
        return remaining;
    }

    let neighbor_mass = current_neighbor
        .slot_for(fluid_id)
        .map(|s| s.mass)
        .unwrap_or(0.0);
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
    // Clamp: don't exceed available capacity in neighbor
    let capacity = (1.0 - current_neighbor.total_mass()).max(0.0);
    flow = flow.min(max_speed).min(remaining).min(capacity).max(0.0);

    if flow <= 0.0 {
        return remaining;
    }

    world.sub_mass(gx, gy, fluid_id, flow);
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

    // Snapshot check: can the neighbor accept this fluid?
    let neighbor = world.read(ngx, gy);
    if !can_accept_fluid(&neighbor, fluid_id) {
        return remaining;
    }

    // Live-state check
    let current_neighbor = world.read_current(ngx, gy);
    if !can_accept_fluid(&current_neighbor, fluid_id) {
        return remaining;
    }

    let neighbor_slot_mass = neighbor
        .slot_for(fluid_id)
        .map(|s| s.mass)
        .unwrap_or(0.0);
    let mut flow = (original_mass - neighbor_slot_mass) / 4.0;
    if flow <= 0.0 {
        return remaining;
    }
    if flow > min_flow {
        flow *= 0.5;
    }
    // Clamp: don't exceed available capacity in neighbor
    let capacity = (1.0 - current_neighbor.total_mass()).max(0.0);
    flow = flow.min(max_speed).min(remaining).min(capacity).max(0.0);

    if flow <= 0.0 {
        return remaining;
    }

    world.sub_mass(gx, gy, fluid_id, flow);
    world.add_mass(ngx, gy, fluid_id, flow);
    remaining - flow
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // NOTE: Integration tests for simulate_grid and reconcile_chunk_boundaries
    // were removed because those functions were replaced by the global
    // FluidWorld-based simulate_tick. New integration tests should use
    // FluidWorld directly.
}
