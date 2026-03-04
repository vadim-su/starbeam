use bevy::prelude::*;
use serde::Deserialize;

use crate::fluid::cell::{FluidCell, FluidId, FluidSlot};
use crate::fluid::events::FluidReactionEvent;
use crate::fluid::fluid_world::FluidWorld;
use crate::fluid::registry::FluidRegistry;
use crate::registry::tile::{TileId, TileRegistry};

// --- Serde default functions ---

fn default_consume() -> f32 {
    1.0
}

// --- Adjacency ---

/// Describes the spatial relationship between two reacting fluids.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub enum Adjacency {
    #[default]
    Any,
    Above,
    Below,
    Side,
}

// --- FluidReactionDef (RON-deserialized) ---

/// A fluid reaction definition as loaded from RON data files.
/// String names are resolved to IDs during compilation.
#[derive(Debug, Clone, Deserialize)]
pub struct FluidReactionDef {
    pub fluid_a: String,
    pub fluid_b: String,
    #[serde(default)]
    pub adjacency: Adjacency,
    pub result_tile: Option<String>,
    pub result_fluid: Option<String>,
    #[serde(default)]
    pub min_mass_a: f32,
    #[serde(default)]
    pub min_mass_b: f32,
    #[serde(default = "default_consume")]
    pub consume_a: f32,
    #[serde(default = "default_consume")]
    pub consume_b: f32,
    pub byproduct_fluid: Option<String>,
    #[serde(default)]
    pub byproduct_mass: f32,
}

// --- CompiledReaction (resolved IDs) ---

/// A reaction with all string names resolved to compact IDs for fast runtime lookup.
#[derive(Debug, Clone)]
pub struct CompiledReaction {
    pub fluid_a: FluidId,
    pub fluid_b: FluidId,
    pub adjacency: Adjacency,
    pub result_tile: Option<TileId>,
    pub result_fluid: Option<FluidId>,
    pub min_mass_a: f32,
    pub min_mass_b: f32,
    pub consume_a: f32,
    pub consume_b: f32,
    pub byproduct_fluid: Option<FluidId>,
    pub byproduct_mass: f32,
}

// --- FluidReactionRegistry ---

/// Runtime registry of compiled fluid reactions. Inserted as a Resource after asset loading.
#[derive(Resource, Debug)]
pub struct FluidReactionRegistry {
    pub reactions: Vec<CompiledReaction>,
}

impl FluidReactionRegistry {
    /// Build registry by resolving all string names in reaction defs to IDs.
    pub fn from_defs(
        defs: &[FluidReactionDef],
        fluid_registry: &FluidRegistry,
        tile_registry: &TileRegistry,
    ) -> Self {
        let reactions = defs
            .iter()
            .map(|def| CompiledReaction {
                fluid_a: fluid_registry.by_name(&def.fluid_a),
                fluid_b: fluid_registry.by_name(&def.fluid_b),
                adjacency: def.adjacency.clone(),
                result_tile: def.result_tile.as_ref().map(|n| tile_registry.by_name(n)),
                result_fluid: def.result_fluid.as_ref().map(|n| fluid_registry.by_name(n)),
                min_mass_a: def.min_mass_a,
                min_mass_b: def.min_mass_b,
                consume_a: def.consume_a,
                consume_b: def.consume_b,
                byproduct_fluid: def
                    .byproduct_fluid
                    .as_ref()
                    .map(|n| fluid_registry.by_name(n)),
                byproduct_mass: def.byproduct_mass,
            })
            .collect();

        Self { reactions }
    }

    /// Find a reaction matching the given fluid pair and adjacency.
    /// Checks both orders (a,b) and (b,a). Adjacency::Any matches everything.
    pub fn find_reaction(
        &self,
        a: FluidId,
        b: FluidId,
        adjacency: &Adjacency,
    ) -> Option<&CompiledReaction> {
        self.reactions.iter().find(|r| {
            let ids_match =
                (r.fluid_a == a && r.fluid_b == b) || (r.fluid_a == b && r.fluid_b == a);
            let adj_match = r.adjacency == Adjacency::Any
                || *adjacency == Adjacency::Any
                || r.adjacency == *adjacency;
            ids_match && adj_match
        })
    }
}

/// Maximum reactions per chunk per tick (rate limiting to avoid lag spikes).
pub const MAX_REACTIONS_PER_CHUNK: u32 = 8;

// ---------------------------------------------------------------------------
// Global density displacement using FluidWorld
// ---------------------------------------------------------------------------

/// Maximum vertical swaps per column per tick.
/// Limits how fast heavy fluids sink through light ones (1 = one cell/tick).
const MAX_VERTICAL_SWAPS_PER_COLUMN: u32 = 2;

/// Maximum horizontal displacement operations per row per tick.
/// Limits sideways spreading to prevent chain-reaction displacement spikes.
const MAX_HORIZONTAL_DISPLACE_PER_ROW: u32 = 2;

/// Rate at which heavy fluid displaces light fluid in dual-slot cells (per tick).
const SLOT_DISPLACEMENT_RATE: f32 = 0.5;

/// Minimum displacement transfer to avoid micro-oscillations.
const MIN_DISPLACEMENT: f32 = 0.002;

/// Resolve density displacement between immiscible fluids using global addressing.
///
/// Phase 0 — **Intra-cell**: enforce density order (heavy primary, light secondary).
///
/// Phase 1 — **Vertical**: bottom-up pass. For dual-slot cells sharing the same
/// two fluids, transfers heavy mass down and light mass up (slot-level redistribution).
/// For single-fluid cells with different types, swaps entire cells.
///
/// Phase 2 — **Horizontal**: heavy fluid swaps with adjacent lighter fluid.
pub fn resolve_density_displacement_global(world: &mut FluidWorld, active_chunks: &[(i32, i32)]) {
    let cs = world.chunk_size as i32;

    // Phase 0: Intra-cell — enforce density order within each cell.
    // Heavy fluid should be in primary slot (bottom), light in secondary (top).
    {
        let fluid_registry = world.fluid_registry;
        for &(cx, cy) in active_chunks {
            if let Some(chunk) = world.world_map.chunks.get_mut(&(cx, cy)) {
                for cell in chunk.fluids.iter_mut() {
                    if !cell.secondary.is_empty() {
                        cell.enforce_density_order(|fid| fluid_registry.get(fid).density);
                    }
                }
            }
        }
    }

    // Phase 1: Vertical — heavy sinks, light rises.
    for &(cx, cy) in active_chunks {
        let base_gx = cx * cs;
        let base_gy = cy * cs;
        for lx in 0..cs {
            let gx = base_gx + lx;
            let mut swaps = 0u32;
            for ly in 0..cs {
                if swaps >= MAX_VERTICAL_SWAPS_PER_COLUMN {
                    break;
                }
                let gy = base_gy + ly;
                let mut below = world.read_current(gx, gy);
                let mut above = world.read_current(gx, gy + 1);
                if below.is_empty() || above.is_empty() {
                    continue;
                }

                // Case A: Both cells have dual slots with the same two fluids.
                // Transfer heavy (primary) mass down, light (secondary) mass up.
                if !above.primary.is_empty()
                    && !above.secondary.is_empty()
                    && !below.primary.is_empty()
                    && !below.secondary.is_empty()
                    && above.primary.fluid_id == below.primary.fluid_id
                    && above.secondary.fluid_id == below.secondary.fluid_id
                {
                    let heavy_above = above.primary.mass;
                    let light_below = below.secondary.mass;
                    let transfer = heavy_above.min(light_below) * SLOT_DISPLACEMENT_RATE;
                    if transfer > MIN_DISPLACEMENT {
                        above.primary.mass -= transfer;
                        above.secondary.mass += transfer;
                        below.primary.mass += transfer;
                        below.secondary.mass -= transfer;
                        above.normalize();
                        below.normalize();
                        world.write(gx, gy, below);
                        world.write(gx, gy + 1, above);
                        swaps += 1;
                    }
                    continue;
                }

                // Case B: Different primary fluids — whole cell swap.
                if below.fluid_id() == above.fluid_id() {
                    continue;
                }
                let d_below = world.fluid_registry.get(below.fluid_id()).density;
                let d_above = world.fluid_registry.get(above.fluid_id()).density;
                if d_above > d_below {
                    world.swap_fluids((gx, gy), (gx, gy + 1));
                    swaps += 1;
                }
            }
        }
    }

    // Phase 2: Horizontal — simple same-level swap between heavy and light fluid.
    // No upward push; the vertical pass on the next tick handles rising.
    // Single alternating sweep direction (even chunks L→R, odd R→L) to reduce bias.
    for &(cx, cy) in active_chunks {
        let base_gx = cx * cs;
        let base_gy = cy * cs;
        for ly in 0..cs {
            let gy = base_gy + ly;
            let mut displace_count = 0u32;
            // Alternate sweep direction per row for symmetry
            if ly % 2 == 0 {
                for lx in 0..(cs - 1) {
                    if displace_count >= MAX_HORIZONTAL_DISPLACE_PER_ROW {
                        break;
                    }
                    let gx = base_gx + lx;
                    if horizontal_swap_global(world, gx, gx + 1, gy) {
                        displace_count += 1;
                    }
                }
            } else {
                for lx in (1..cs).rev() {
                    if displace_count >= MAX_HORIZONTAL_DISPLACE_PER_ROW {
                        break;
                    }
                    let gx = base_gx + lx;
                    if horizontal_swap_global(world, gx, gx - 1, gy) {
                        displace_count += 1;
                    }
                }
            }
        }
    }
}

/// Try a horizontal displacement between two adjacent cells at the same Y level.
///
/// Handles two cases:
/// - **Dual-slot equalization**: both cells share the same two fluids — equalize
///   heavy/light mass between them so the heavy fluid spreads evenly on its layer.
/// - **Single-fluid swap**: different primary fluids — exchange entire cells if
///   src is heavier than dst.
///
/// Returns true if a displacement was performed.
fn horizontal_swap_global(world: &mut FluidWorld, src_gx: i32, dst_gx: i32, gy: i32) -> bool {
    let src = world.read_current(src_gx, gy);
    let dst = world.read_current(dst_gx, gy);

    if src.is_empty() || dst.is_empty() {
        return false;
    }

    // Case A: Both cells have dual slots with the same two fluids.
    // Equalize heavy fluid mass between neighbors so lava spreads evenly.
    if !src.primary.is_empty()
        && !src.secondary.is_empty()
        && !dst.primary.is_empty()
        && !dst.secondary.is_empty()
        && src.primary.fluid_id == dst.primary.fluid_id
        && src.secondary.fluid_id == dst.secondary.fluid_id
    {
        let heavy_diff = src.primary.mass - dst.primary.mass;
        if heavy_diff.abs() < MIN_DISPLACEMENT * 2.0 {
            return false;
        }
        // Transfer from the cell with more heavy fluid to the one with less.
        // Move heavy one way, light the other to keep totals constant.
        let transfer = heavy_diff * 0.25;
        if transfer.abs() < MIN_DISPLACEMENT {
            return false;
        }
        let mut new_src = src;
        let mut new_dst = dst;
        new_src.primary.mass -= transfer;
        new_src.secondary.mass += transfer;
        new_dst.primary.mass += transfer;
        new_dst.secondary.mass -= transfer;
        new_src.normalize();
        new_dst.normalize();
        world.write(src_gx, gy, new_src);
        world.write(dst_gx, gy, new_dst);
        return true;
    }

    // Case B: Different primary fluids — whole cell swap.
    if src.fluid_id() == dst.fluid_id() {
        return false;
    }

    let d_src = world.fluid_registry.get(src.fluid_id()).density;
    let d_dst = world.fluid_registry.get(dst.fluid_id()).density;

    if d_src <= d_dst {
        return false;
    }

    world.swap_fluids((src_gx, gy), (dst_gx, gy));
    true
}

// ---------------------------------------------------------------------------
// Global fluid reactions using FluidWorld
// ---------------------------------------------------------------------------

/// Process fluid reactions for all active chunks using global addressing.
///
/// Checks each non-empty cell against its 4 neighbors. When a reaction is
/// found in the registry, consumes fluid mass, places result tiles/fluids,
/// and returns `FluidReactionEvent`s for VFX systems.
pub fn execute_fluid_reactions_global(
    world: &mut FluidWorld,
    active_chunks: &[(i32, i32)],
    reaction_registry: &FluidReactionRegistry,
    tile_size: f32,
) -> Vec<FluidReactionEvent> {
    let cs = world.chunk_size as i32;
    let mut events = Vec::new();
    let mut reaction_count: u32 = 0;
    let max_total = MAX_REACTIONS_PER_CHUNK * active_chunks.len().max(1) as u32;

    for &(cx, cy) in active_chunks {
        let base_gx = cx * cs;
        let base_gy = cy * cs;
        for ly in 0..cs {
            for lx in 0..cs {
                if reaction_count >= max_total {
                    return events;
                }
                let gx = base_gx + lx;
                let gy = base_gy + ly;
                let cell = world.read_current(gx, gy);
                if cell.is_empty() {
                    continue;
                }

                // Check 4 neighbors (dx, dy, adjacency from cell's perspective)
                let neighbors: [(i32, i32, Adjacency); 4] = [
                    (0, -1, Adjacency::Below),
                    (0, 1, Adjacency::Above),
                    (-1, 0, Adjacency::Side),
                    (1, 0, Adjacency::Side),
                ];

                for (dx, dy, adj) in &neighbors {
                    let ngx = gx + dx;
                    let ngy = gy + dy;
                    let neighbor = world.read_current(ngx, ngy);
                    if neighbor.is_empty() || neighbor.fluid_id() == cell.fluid_id() {
                        continue;
                    }

                    let Some(reaction) =
                        reaction_registry.find_reaction(cell.fluid_id(), neighbor.fluid_id(), adj)
                    else {
                        continue;
                    };

                    // Determine which position is fluid_a vs fluid_b
                    let (a_pos, b_pos) = if cell.fluid_id() == reaction.fluid_a {
                        ((gx, gy), (ngx, ngy))
                    } else {
                        ((ngx, ngy), (gx, gy))
                    };

                    let cell_a = world.read_current(a_pos.0, a_pos.1);
                    let cell_b = world.read_current(b_pos.0, b_pos.1);
                    if cell_a.mass() < reaction.min_mass_a || cell_b.mass() < reaction.min_mass_b {
                        continue;
                    }

                    // Consume mass from primary slots
                    let mut new_a = cell_a;
                    let mut new_b = cell_b;
                    new_a.primary.mass -= reaction.consume_a;
                    new_b.primary.mass -= reaction.consume_b;
                    if new_a.primary.mass < 0.001 {
                        new_a.primary = FluidSlot::EMPTY;
                        new_a.normalize();
                    }
                    if new_b.primary.mass < 0.001 {
                        new_b.primary = FluidSlot::EMPTY;
                        new_b.normalize();
                    }

                    // Place result tile at the primary cell (a_pos)
                    if let Some(tile_id) = reaction.result_tile {
                        world.set_tile(a_pos.0, a_pos.1, tile_id);
                        new_a = FluidCell::EMPTY; // tile replaces fluid
                    }

                    // Place result fluid or byproduct at primary cell
                    if new_a.is_empty() {
                        if let Some(fid) = reaction.result_fluid {
                            new_a = FluidCell::new(fid, reaction.byproduct_mass.max(0.1));
                        } else if let Some(fid) = reaction.byproduct_fluid {
                            new_a = FluidCell::new(fid, reaction.byproduct_mass.max(0.1));
                        }
                    }

                    world.write(a_pos.0, a_pos.1, new_a);
                    world.write(b_pos.0, b_pos.1, new_b);

                    // Emit event for VFX
                    let world_x = gx as f32 * tile_size + tile_size * 0.5;
                    let world_y = gy as f32 * tile_size + tile_size * 0.5;
                    events.push(FluidReactionEvent {
                        position: Vec2::new(world_x, world_y),
                        fluid_a: cell_a.fluid_id(),
                        fluid_b: cell_b.fluid_id(),
                        result_tile: reaction.result_tile,
                        result_fluid: reaction.result_fluid,
                    });

                    reaction_count += 1;
                    break; // at most one reaction per cell per tick
                }
            }
        }
    }
    events
}

// ---------------------------------------------------------------------------
// SPH particle-proximity reactions
// ---------------------------------------------------------------------------

use crate::fluid::spatial_hash::SpatialHash;
use crate::fluid::sph_particle::ParticleStore;

/// Check SPH particles for reactions based on proximity within `radius`.
/// When two particles of different fluid types are within `radius` and match
/// a reaction, both particles are removed and an event is emitted.
/// Returns reaction events and indices of consumed particles (sorted descending for safe removal).
pub fn execute_sph_particle_reactions(
    particles: &ParticleStore,
    radius: f32,
    reaction_registry: &FluidReactionRegistry,
) -> (Vec<FluidReactionEvent>, Vec<usize>) {
    if particles.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let grid = SpatialHash::from_positions(&particles.positions, radius);
    let mut events = Vec::new();
    let mut consumed: Vec<bool> = vec![false; particles.len()];

    for i in 0..particles.len() {
        if consumed[i] {
            continue;
        }
        let fid_i = particles.fluid_ids[i];
        if fid_i == FluidId::NONE {
            continue;
        }
        let pos_i = particles.positions[i];

        for &j in &grid.query(pos_i) {
            if j <= i || consumed[j] {
                continue;
            }
            let fid_j = particles.fluid_ids[j];
            if fid_j == FluidId::NONE || fid_j == fid_i {
                continue;
            }
            let dist = pos_i.distance(particles.positions[j]);
            if dist > radius {
                continue;
            }

            // Check reaction registry (adjacency = Any for particles)
            let Some(_reaction) =
                reaction_registry.find_reaction(fid_i, fid_j, &Adjacency::Any)
            else {
                continue;
            };

            // Mark both particles as consumed
            consumed[i] = true;
            consumed[j] = true;

            // Emit event at midpoint
            let midpoint = (pos_i + particles.positions[j]) * 0.5;
            events.push(FluidReactionEvent {
                position: midpoint,
                fluid_a: fid_i,
                fluid_b: fid_j,
                result_tile: _reaction.result_tile,
                result_fluid: _reaction.result_fluid,
            });

            break; // one reaction per particle per tick
        }
    }

    // Collect consumed indices in descending order for safe swap-removal
    let mut to_remove: Vec<usize> = consumed
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| if c { Some(i) } else { None })
        .collect();
    to_remove.sort_unstable_by(|a, b| b.cmp(a));

    (events, to_remove)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::fluid_world::FluidWorld;
    use crate::fluid::registry::FluidDef;
    use crate::world::chunk::{ChunkData, TileLayer, WorldMap};

    fn test_tile_registry() -> TileRegistry {
        crate::test_helpers::fixtures::test_tile_registry()
    }

    fn test_fluid_registry() -> FluidRegistry {
        FluidRegistry::from_defs(vec![
            FluidDef {
                id: "water".to_string(),
                density: 1000.0,
                viscosity: 0.1,
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
                id: "lava".to_string(),
                density: 3000.0,
                viscosity: 0.6,
                max_compress: 0.01,
                is_gas: false,
                color: [255, 80, 20, 220],
                damage_on_contact: 10.0,
                light_emission: [255, 100, 20],
                effects: vec![],
                wave_amplitude: 0.4,
                wave_speed: 0.3,
                light_absorption: 0.8,
            },
            FluidDef {
                id: "steam".to_string(),
                density: 0.6,
                viscosity: 0.05,
                max_compress: 0.01,
                is_gas: true,
                color: [200, 200, 200, 100],
                damage_on_contact: 0.0,
                light_emission: [0, 0, 0],
                effects: vec![],
                wave_amplitude: 0.6,
                wave_speed: 1.5,
                light_absorption: 0.05,
            },
        ])
    }

    fn test_reaction_defs() -> Vec<FluidReactionDef> {
        vec![
            FluidReactionDef {
                fluid_a: "water".to_string(),
                fluid_b: "lava".to_string(),
                adjacency: Adjacency::Any,
                result_tile: Some("stone".to_string()),
                result_fluid: None,
                min_mass_a: 0.0,
                min_mass_b: 0.0,
                consume_a: 1.0,
                consume_b: 1.0,
                byproduct_fluid: Some("steam".to_string()),
                byproduct_mass: 0.5,
            },
            FluidReactionDef {
                fluid_a: "water".to_string(),
                fluid_b: "lava".to_string(),
                adjacency: Adjacency::Below,
                result_tile: None,
                result_fluid: Some("steam".to_string()),
                min_mass_a: 0.0,
                min_mass_b: 0.0,
                consume_a: 0.5,
                consume_b: 0.0,
                byproduct_fluid: None,
                byproduct_mass: 0.0,
            },
        ]
    }

    fn make_chunk(cs: u32) -> ChunkData {
        let len = (cs * cs) as usize;
        ChunkData {
            fg: TileLayer::new_air(len),
            bg: TileLayer::new_air(len),
            fluids: vec![FluidCell::EMPTY; len],
            objects: Vec::new(),
            occupancy: vec![None; len],
            damage: vec![0; len],
        }
    }

    // --- Density displacement tests ---

    #[test]
    fn density_displacement_heavy_sinks() {
        let fr = test_fluid_registry();
        let tr = test_tile_registry();
        let water_id = fr.by_name("water"); // density 1000
        let lava_id = fr.by_name("lava"); // density 3000

        let cs = 4u32;
        let mut world_map = WorldMap::default();
        let mut chunk = make_chunk(cs);
        // Water below (y=0), lava above (y=1) — lava is heavier, should sink
        chunk.fluids[(0 * cs + 1) as usize] = FluidCell::new(water_id, 1.0); // (1, 0)
        chunk.fluids[(1 * cs + 1) as usize] = FluidCell::new(lava_id, 1.0); // (1, 1)
        world_map.chunks.insert((0, 0), chunk);

        let active = vec![(0, 0)];
        let mut world = FluidWorld::new(&mut world_map, cs, 1, 1, &tr, &fr);
        resolve_density_displacement_global(&mut world, &active);

        // After displacement: lava should be below, water above
        let below = world.read_current(1, 0);
        let above = world.read_current(1, 1);
        assert_eq!(below.fluid_id(), lava_id);
        assert_eq!(above.fluid_id(), water_id);
    }

    #[test]
    fn density_displacement_light_stays_on_top() {
        let fr = test_fluid_registry();
        let tr = test_tile_registry();
        let water_id = fr.by_name("water"); // density 1000
        let steam_id = fr.by_name("steam"); // density 0.6

        let cs = 4u32;
        let mut world_map = WorldMap::default();
        let mut chunk = make_chunk(cs);
        chunk.fluids[(0 * cs + 1) as usize] = FluidCell::new(water_id, 1.0); // (1, 0)
        chunk.fluids[(1 * cs + 1) as usize] = FluidCell::new(steam_id, 0.8); // (1, 1)
        world_map.chunks.insert((0, 0), chunk);

        let active = vec![(0, 0)];
        let mut world = FluidWorld::new(&mut world_map, cs, 1, 1, &tr, &fr);
        resolve_density_displacement_global(&mut world, &active);

        // No change — water is heavier and already below
        assert_eq!(world.read_current(1, 0).fluid_id(), water_id);
        assert_eq!(world.read_current(1, 1).fluid_id(), steam_id);
    }

    #[test]
    fn density_displacement_preserves_mass() {
        let fr = test_fluid_registry();
        let tr = test_tile_registry();
        let water_id = fr.by_name("water");
        let lava_id = fr.by_name("lava");

        let cs = 4u32;
        let mut world_map = WorldMap::default();
        let mut chunk = make_chunk(cs);
        chunk.fluids[(0 * cs + 0) as usize] = FluidCell::new(water_id, 0.7); // (0, 0)
        chunk.fluids[(1 * cs + 0) as usize] = FluidCell::new(lava_id, 0.9); // (0, 1)
        world_map.chunks.insert((0, 0), chunk);

        let active = vec![(0, 0)];
        let mut world = FluidWorld::new(&mut world_map, cs, 1, 1, &tr, &fr);
        resolve_density_displacement_global(&mut world, &active);

        // Lava sinks, water rises — masses preserved
        let below = world.read_current(0, 0);
        let above = world.read_current(0, 1);
        assert_eq!(below.fluid_id(), lava_id);
        assert!((below.mass() - 0.9).abs() < f32::EPSILON);
        assert_eq!(above.fluid_id(), water_id);
        assert!((above.mass() - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn density_displacement_skips_empty_cells() {
        let fr = test_fluid_registry();
        let tr = test_tile_registry();
        let water_id = fr.by_name("water");

        let cs = 4u32;
        let mut world_map = WorldMap::default();
        let mut chunk = make_chunk(cs);
        chunk.fluids[(0 * cs + 0) as usize] = FluidCell::new(water_id, 1.0); // (0, 0)
        world_map.chunks.insert((0, 0), chunk);

        let active = vec![(0, 0)];
        let mut world = FluidWorld::new(&mut world_map, cs, 1, 1, &tr, &fr);
        resolve_density_displacement_global(&mut world, &active);

        assert_eq!(world.read_current(0, 0).fluid_id(), water_id);
        assert!(world.read_current(0, 1).is_empty());
    }

    #[test]
    fn density_displacement_skips_same_fluid() {
        let fr = test_fluid_registry();
        let tr = test_tile_registry();
        let water_id = fr.by_name("water");

        let cs = 4u32;
        let mut world_map = WorldMap::default();
        let mut chunk = make_chunk(cs);
        chunk.fluids[(0 * cs + 0) as usize] = FluidCell::new(water_id, 0.5); // (0, 0)
        chunk.fluids[(1 * cs + 0) as usize] = FluidCell::new(water_id, 0.8); // (0, 1)
        world_map.chunks.insert((0, 0), chunk);

        let active = vec![(0, 0)];
        let mut world = FluidWorld::new(&mut world_map, cs, 1, 1, &tr, &fr);
        resolve_density_displacement_global(&mut world, &active);

        // Masses unchanged
        assert!((world.read_current(0, 0).mass() - 0.5).abs() < f32::EPSILON);
        assert!((world.read_current(0, 1).mass() - 0.8).abs() < f32::EPSILON);
    }

    // --- Phase 0: intra-cell density enforcement ---

    #[test]
    fn phase0_enforces_density_order_within_cell() {
        let fr = test_fluid_registry();
        let tr = test_tile_registry();
        let water_id = fr.by_name("water"); // density 1000
        let lava_id = fr.by_name("lava"); // density 3000

        let cs = 4u32;
        let mut world_map = WorldMap::default();
        let mut chunk = make_chunk(cs);
        // Light fluid in primary, heavy in secondary — wrong order
        chunk.fluids[0] = FluidCell {
            primary: FluidSlot::new(water_id, 0.5),
            secondary: FluidSlot::new(lava_id, 0.3),
        };
        world_map.chunks.insert((0, 0), chunk);

        let active = vec![(0, 0)];
        let mut world = FluidWorld::new(&mut world_map, cs, 1, 1, &tr, &fr);
        resolve_density_displacement_global(&mut world, &active);

        // After Phase 0: heavy (lava) should be primary, light (water) secondary
        let cell = world.read_current(0, 0);
        assert_eq!(cell.primary.fluid_id, lava_id);
        assert_eq!(cell.secondary.fluid_id, water_id);
        assert!((cell.primary.mass - 0.3).abs() < f32::EPSILON);
        assert!((cell.secondary.mass - 0.5).abs() < f32::EPSILON);
    }

    // --- Reaction registry tests ---

    #[test]
    fn find_reaction_any_adjacency() {
        let fr = test_fluid_registry();
        let tr = test_tile_registry();
        let defs = test_reaction_defs();
        let registry = FluidReactionRegistry::from_defs(&defs, &fr, &tr);

        let water_id = fr.by_name("water");
        let lava_id = fr.by_name("lava");

        // Adjacency::Any in the reaction should match any query adjacency
        let result = registry.find_reaction(water_id, lava_id, &Adjacency::Side);
        assert!(result.is_some());
        let reaction = result.unwrap();
        assert_eq!(reaction.adjacency, Adjacency::Any);
        assert_eq!(reaction.result_tile, Some(tr.by_name("stone")));
    }

    #[test]
    fn find_reaction_either_order() {
        let fr = test_fluid_registry();
        let tr = test_tile_registry();
        let defs = test_reaction_defs();
        let registry = FluidReactionRegistry::from_defs(&defs, &fr, &tr);

        let water_id = fr.by_name("water");
        let lava_id = fr.by_name("lava");

        // (water, lava) should match
        let r1 = registry.find_reaction(water_id, lava_id, &Adjacency::Any);
        assert!(r1.is_some());

        // (lava, water) should also match
        let r2 = registry.find_reaction(lava_id, water_id, &Adjacency::Any);
        assert!(r2.is_some());
    }

    #[test]
    fn find_reaction_specific_adjacency() {
        let fr = test_fluid_registry();
        let tr = test_tile_registry();

        // Only a Below-specific reaction
        let defs = vec![FluidReactionDef {
            fluid_a: "water".to_string(),
            fluid_b: "lava".to_string(),
            adjacency: Adjacency::Below,
            result_tile: None,
            result_fluid: Some("steam".to_string()),
            min_mass_a: 0.0,
            min_mass_b: 0.0,
            consume_a: 0.5,
            consume_b: 0.0,
            byproduct_fluid: None,
            byproduct_mass: 0.0,
        }];
        let registry = FluidReactionRegistry::from_defs(&defs, &fr, &tr);

        let water_id = fr.by_name("water");
        let lava_id = fr.by_name("lava");

        // Below matches Below
        let r = registry.find_reaction(water_id, lava_id, &Adjacency::Below);
        assert!(r.is_some());

        // Side does NOT match Below
        let r = registry.find_reaction(water_id, lava_id, &Adjacency::Side);
        assert!(r.is_none());

        // Any query matches any reaction (Any acts as wildcard from caller side)
        let r = registry.find_reaction(water_id, lava_id, &Adjacency::Any);
        assert!(r.is_some());
    }

    #[test]
    fn find_reaction_returns_none_for_unknown_pair() {
        let fr = test_fluid_registry();
        let tr = test_tile_registry();
        let defs = test_reaction_defs();
        let registry = FluidReactionRegistry::from_defs(&defs, &fr, &tr);

        let water_id = fr.by_name("water");
        let steam_id = fr.by_name("steam");

        // No reaction defined for water + steam
        let r = registry.find_reaction(water_id, steam_id, &Adjacency::Any);
        assert!(r.is_none());
    }

    #[test]
    fn compiled_reaction_has_correct_ids() {
        let fr = test_fluid_registry();
        let tr = test_tile_registry();
        let defs = test_reaction_defs();
        let registry = FluidReactionRegistry::from_defs(&defs, &fr, &tr);

        let reaction = &registry.reactions[0];
        assert_eq!(reaction.fluid_a, fr.by_name("water"));
        assert_eq!(reaction.fluid_b, fr.by_name("lava"));
        assert_eq!(reaction.result_tile, Some(tr.by_name("stone")));
        assert_eq!(reaction.byproduct_fluid, Some(fr.by_name("steam")));
        assert!((reaction.byproduct_mass - 0.5).abs() < f32::EPSILON);
        assert!((reaction.consume_a - 1.0).abs() < f32::EPSILON);
        assert!((reaction.consume_b - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn from_defs_compiles_all_reactions() {
        let fr = test_fluid_registry();
        let tr = test_tile_registry();
        let defs = test_reaction_defs();
        let registry = FluidReactionRegistry::from_defs(&defs, &fr, &tr);

        assert_eq!(registry.reactions.len(), 2);
    }

    // --- execute_fluid_reactions_global tests ---

    fn simple_water_lava_registry() -> FluidReactionRegistry {
        let fr = test_fluid_registry();
        let tr = test_tile_registry();
        // Single reaction: water + lava (Any) -> stone tile + steam byproduct
        let defs = vec![FluidReactionDef {
            fluid_a: "water".to_string(),
            fluid_b: "lava".to_string(),
            adjacency: Adjacency::Any,
            result_tile: Some("stone".to_string()),
            result_fluid: None,
            min_mass_a: 0.0,
            min_mass_b: 0.0,
            consume_a: 1.0,
            consume_b: 1.0,
            byproduct_fluid: Some("steam".to_string()),
            byproduct_mass: 0.5,
        }];
        FluidReactionRegistry::from_defs(&defs, &fr, &tr)
    }

    #[test]
    fn water_lava_produces_stone_and_steam() {
        let fr = test_fluid_registry();
        let tr = test_tile_registry();
        let registry = simple_water_lava_registry();
        let water_id = fr.by_name("water");
        let lava_id = fr.by_name("lava");
        let steam_id = fr.by_name("steam");
        let stone_id = tr.by_name("stone");

        let cs = 4u32;
        let mut world_map = WorldMap::default();
        let mut chunk = make_chunk(cs);
        // water at (0,0), lava at (1,0) — adjacent horizontally
        chunk.fluids[(0 * cs + 0) as usize] = FluidCell::new(water_id, 1.0);
        chunk.fluids[(0 * cs + 1) as usize] = FluidCell::new(lava_id, 1.0);
        world_map.chunks.insert((0, 0), chunk);

        let active = vec![(0, 0)];
        let mut world = FluidWorld::new(&mut world_map, cs, 1, 1, &tr, &fr);
        let events = execute_fluid_reactions_global(&mut world, &active, &registry, 16.0);

        // Reaction should have occurred
        assert!(!events.is_empty(), "expected at least one reaction event");
        // Stone tile placed at reaction site
        let tile = world.tile_at(0, 0);
        assert_eq!(tile, stone_id, "expected stone tile at water position");
        // Water cell should have steam byproduct or be empty
        let water_cell = world.read_current(0, 0);
        assert!(
            water_cell.is_empty() || water_cell.fluid_id() == steam_id,
            "expected steam or empty at reacted cell"
        );
    }

    #[test]
    fn reaction_rate_limited() {
        let fr = test_fluid_registry();
        let tr = test_tile_registry();
        let registry = simple_water_lava_registry();
        let water_id = fr.by_name("water");
        let lava_id = fr.by_name("lava");

        let cs = 4u32;
        let mut world_map = WorldMap::default();
        let mut chunk = make_chunk(cs);
        // Fill chunk with alternating water/lava = many possible reactions
        for y in 0..cs {
            for x in 0..cs {
                let id = if (x + y) % 2 == 0 { water_id } else { lava_id };
                chunk.fluids[(y * cs + x) as usize] = FluidCell::new(id, 1.0);
            }
        }
        world_map.chunks.insert((0, 0), chunk);

        let active = vec![(0, 0)];
        let mut world = FluidWorld::new(&mut world_map, cs, 1, 1, &tr, &fr);
        let events = execute_fluid_reactions_global(&mut world, &active, &registry, 16.0);

        assert!(
            events.len() <= MAX_REACTIONS_PER_CHUNK as usize,
            "reactions should be capped at MAX_REACTIONS_PER_CHUNK, got {}",
            events.len()
        );
    }

    #[test]
    fn reaction_respects_min_mass() {
        let fr = test_fluid_registry();
        let tr = test_tile_registry();
        let water_id = fr.by_name("water");
        let lava_id = fr.by_name("lava");

        // Reaction requires min_mass_a = 0.5
        let defs = vec![FluidReactionDef {
            fluid_a: "water".to_string(),
            fluid_b: "lava".to_string(),
            adjacency: Adjacency::Any,
            result_tile: Some("stone".to_string()),
            result_fluid: None,
            min_mass_a: 0.5,
            min_mass_b: 0.0,
            consume_a: 1.0,
            consume_b: 1.0,
            byproduct_fluid: None,
            byproduct_mass: 0.0,
        }];
        let registry = FluidReactionRegistry::from_defs(&defs, &fr, &tr);

        let cs = 4u32;
        let mut world_map = WorldMap::default();
        let mut chunk = make_chunk(cs);
        // Water mass is below min_mass_a
        chunk.fluids[(0 * cs + 0) as usize] = FluidCell::new(water_id, 0.1);
        chunk.fluids[(0 * cs + 1) as usize] = FluidCell::new(lava_id, 1.0);
        world_map.chunks.insert((0, 0), chunk);

        let active = vec![(0, 0)];
        let mut world = FluidWorld::new(&mut world_map, cs, 1, 1, &tr, &fr);
        let events = execute_fluid_reactions_global(&mut world, &active, &registry, 16.0);

        assert!(
            events.is_empty(),
            "reaction should not fire when mass below minimum"
        );
        assert_eq!(world.tile_at(0, 0), TileId::AIR, "no tile should be placed");
    }

    // --- SPH particle reaction tests ---

    use crate::fluid::sph_particle::{Particle, ParticleStore};
    use bevy::math::Vec2;

    #[test]
    fn sph_reaction_fires_for_close_particles() {
        let fr = test_fluid_registry();
        let registry = simple_water_lava_registry();
        let water_id = fr.by_name("water");
        let lava_id = fr.by_name("lava");

        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(10.0, 10.0), water_id, 1.0));
        store.add(Particle::new(Vec2::new(14.0, 10.0), lava_id, 1.0));

        let (events, to_remove) = execute_sph_particle_reactions(&store, 16.0, &registry);
        assert_eq!(events.len(), 1, "should fire one reaction");
        assert_eq!(to_remove.len(), 2, "both particles consumed");
        assert_eq!(events[0].fluid_a, water_id);
        assert_eq!(events[0].fluid_b, lava_id);
    }

    #[test]
    fn sph_no_reaction_for_distant_particles() {
        let fr = test_fluid_registry();
        let registry = simple_water_lava_registry();
        let water_id = fr.by_name("water");
        let lava_id = fr.by_name("lava");

        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(0.0, 0.0), water_id, 1.0));
        store.add(Particle::new(Vec2::new(100.0, 100.0), lava_id, 1.0));

        let (events, to_remove) = execute_sph_particle_reactions(&store, 16.0, &registry);
        assert!(events.is_empty(), "no reaction for distant particles");
        assert!(to_remove.is_empty());
    }

    #[test]
    fn sph_no_reaction_for_same_fluid() {
        let fr = test_fluid_registry();
        let registry = simple_water_lava_registry();
        let water_id = fr.by_name("water");

        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(10.0, 10.0), water_id, 1.0));
        store.add(Particle::new(Vec2::new(14.0, 10.0), water_id, 1.0));

        let (events, to_remove) = execute_sph_particle_reactions(&store, 16.0, &registry);
        assert!(events.is_empty(), "same fluid type should not react");
        assert!(to_remove.is_empty());
    }
}
