use super::active::ActiveFluids;
use super::cell::{FluidCell, FluidId};
use crate::world::chunk::WorldMap;
use crate::world::ctx::WorldCtxRef;

/// Redistribute displaced fluid to neighboring tiles when a block is placed
/// on a tile containing fluid.
pub fn displace_fluid(
    tile_x: i32,
    tile_y: i32,
    displaced: FluidCell,
    world_map: &mut WorldMap,
    active_fluids: &mut ActiveFluids,
    ctx: &WorldCtxRef,
) {
    let mut remaining = displaced.level as i32;

    // Priority: up first (fluid pushed up by block), then sides, down last
    let neighbors = [(0, 1), (-1, 0), (1, 0), (0, -1)];

    for (dx, dy) in neighbors {
        if remaining <= 0 {
            break;
        }

        let nx = tile_x + dx;
        let ny = tile_y + dy;

        if world_map.is_solid(nx, ny, ctx) {
            continue;
        }

        let neighbor_fluid = world_map.get_fluid(nx, ny, ctx).unwrap_or_default();

        // Can't mix different fluid types
        if neighbor_fluid.fluid_id != FluidId::NONE
            && neighbor_fluid.fluid_id != displaced.fluid_id
        {
            continue;
        }

        let space = 255 - neighbor_fluid.level as i32;
        if space <= 0 {
            continue;
        }

        let transfer = remaining.min(space);
        let new_cell = FluidCell {
            fluid_id: displaced.fluid_id,
            level: (neighbor_fluid.level as i32 + transfer) as u8,
        };
        world_map.set_fluid(nx, ny, new_cell, ctx);
        active_fluids.wake(nx, ny);
        remaining -= transfer;
    }

    // If remaining > 0, fluid is destroyed (fully enclosed)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Integration tests for displacement require a full WorldMap + WorldCtxRef,
    // which are tested via the test_helpers fixtures in system-level tests.
    // Unit-level logic is straightforward: iterate neighbors, fill available space.

    #[test]
    fn fluid_cell_default_is_empty() {
        let cell = FluidCell::default();
        assert!(cell.is_empty());
    }
}
