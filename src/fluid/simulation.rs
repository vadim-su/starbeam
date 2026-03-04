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
                    // Liquid: flow DOWN first (primary), then horizontal, then UP (decompression)
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
        let config = FluidSimConfig::default();

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
        let config = FluidSimConfig::default();
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
