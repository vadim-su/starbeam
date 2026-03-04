use bevy::prelude::*;
use serde::Deserialize;

use crate::fluid::cell::{FluidCell, FluidId};
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

/// Resolve density displacement between immiscible fluids using global addressing.
///
/// Phase 1 — **Vertical**: multi-pass bubble-sort so heavier fluids sink
/// completely through lighter ones.
///
/// Phase 2 — **Horizontal spreading**: heavy fluid undercuts lighter fluid
/// via 3-cell rotation. Two sweeps (L→R then R→L) ensure symmetric spread.
pub fn resolve_density_displacement_global(world: &mut FluidWorld, active_chunks: &[(i32, i32)]) {
    let cs = world.chunk_size as i32;

    // Phase 1: Vertical bubble sort — limited to 2 passes per tick
    // for smooth settling instead of instant teleportation.
    // Heavy fluids sink ~2 cells/tick; at 60 ticks/sec that's 120 cells/sec.
    for _pass in 0..2 {
        let mut any_swap = false;
        for &(cx, cy) in active_chunks {
            let base_gx = cx * cs;
            let base_gy = cy * cs;
            for ly in 0..cs {
                let gy = base_gy + ly;
                for lx in 0..cs {
                    let gx = base_gx + lx;
                    let below = world.read_current(gx, gy);
                    let above = world.read_current(gx, gy + 1);
                    if below.is_empty() || above.is_empty() {
                        continue;
                    }
                    if below.fluid_id == above.fluid_id {
                        continue;
                    }
                    let d_below = world.fluid_registry.get(below.fluid_id).density;
                    let d_above = world.fluid_registry.get(above.fluid_id).density;
                    if d_above > d_below {
                        world.swap_fluids((gx, gy), (gx, gy + 1));
                        any_swap = true;
                    }
                }
            }
        }
        if !any_swap {
            break;
        }
    }

    // Phase 2: Horizontal spreading (L→R then R→L)
    for &(cx, cy) in active_chunks {
        let base_gx = cx * cs;
        let base_gy = cy * cs;
        // Left-to-right sweep
        for ly in 0..cs {
            let gy = base_gy + ly;
            for lx in 0..(cs - 1) {
                let gx = base_gx + lx;
                horizontal_displace_global(world, gx, gx + 1, gy);
            }
        }
        // Right-to-left sweep
        for ly in 0..cs {
            let gy = base_gy + ly;
            for lx in (1..cs).rev() {
                let gx = base_gx + lx;
                horizontal_displace_global(world, gx, gx - 1, gy);
            }
        }
    }
}

/// Try a single horizontal displacement between cells at (src_gx, gy) and (dst_gx, gy)
/// using global coordinates.
///
/// If src is heavier than dst, move src sideways to dst's position and push
/// dst up one cell. The cell above dst must be available (empty and not solid).
fn horizontal_displace_global(world: &mut FluidWorld, src_gx: i32, dst_gx: i32, gy: i32) {
    let src = world.read_current(src_gx, gy);
    let dst = world.read_current(dst_gx, gy);

    if src.is_empty() || dst.is_empty() {
        return;
    }
    if src.fluid_id == dst.fluid_id {
        return;
    }

    let d_src = world.fluid_registry.get(src.fluid_id).density;
    let d_dst = world.fluid_registry.get(dst.fluid_id).density;

    // Only the heavier fluid displaces the lighter one
    if d_src <= d_dst {
        return;
    }

    // Need a cell above the destination for the displaced light fluid
    if world.is_solid(dst_gx, gy + 1) {
        return;
    }

    let above = world.read_current(dst_gx, gy + 1);

    if above.is_empty() {
        // Empty cell above → light fluid goes up
        world.write(dst_gx, gy + 1, dst);
        world.write(dst_gx, gy, src);
        world.write(src_gx, gy, FluidCell::EMPTY);
    } else if above.fluid_id == dst.fluid_id {
        // Same-type fluid above → merge light fluid mass upward
        let mut merged = above;
        merged.mass += dst.mass;
        world.write(dst_gx, gy + 1, merged);
        world.write(dst_gx, gy, src);
        world.write(src_gx, gy, FluidCell::EMPTY);
    }
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
                    if neighbor.is_empty() || neighbor.fluid_id == cell.fluid_id {
                        continue;
                    }

                    let Some(reaction) =
                        reaction_registry.find_reaction(cell.fluid_id, neighbor.fluid_id, adj)
                    else {
                        continue;
                    };

                    // Determine which position is fluid_a vs fluid_b
                    let (a_pos, b_pos) = if cell.fluid_id == reaction.fluid_a {
                        ((gx, gy), (ngx, ngy))
                    } else {
                        ((ngx, ngy), (gx, gy))
                    };

                    let cell_a = world.read_current(a_pos.0, a_pos.1);
                    let cell_b = world.read_current(b_pos.0, b_pos.1);
                    if cell_a.mass < reaction.min_mass_a || cell_b.mass < reaction.min_mass_b {
                        continue;
                    }

                    // Consume mass
                    let mut new_a = cell_a;
                    let mut new_b = cell_b;
                    new_a.mass -= reaction.consume_a;
                    new_b.mass -= reaction.consume_b;
                    if new_a.mass < 0.001 {
                        new_a = FluidCell::EMPTY;
                    }
                    if new_b.mass < 0.001 {
                        new_b = FluidCell::EMPTY;
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
                        fluid_a: cell_a.fluid_id,
                        fluid_b: cell_b.fluid_id,
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
