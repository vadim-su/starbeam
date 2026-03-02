use bevy::prelude::*;

use crate::fluid::cell::{FluidCell, FluidId};
use crate::fluid::registry::FluidRegistry;
use crate::registry::tile::{TileId, TileRegistry};

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
            iterations_per_tick: 3,
            min_mass: MIN_MASS,
            min_flow: MIN_FLOW,
            max_speed: MAX_SPEED,
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

    let flow;
    if is_primary {
        // Primary direction: use get_stable_state to determine how much
        // should be in the "lower" cell (the one fluid flows toward).
        let target_in_neighbor = get_stable_state(total, max_compress);
        flow = target_in_neighbor - neighbor_mass;
    } else {
        // Decompression: only compressed fluid flows in this direction
        if remaining <= MAX_MASS {
            return remaining;
        }
        let target_stay = get_stable_state(total, max_compress);
        flow = remaining - target_stay;
    }

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
}
