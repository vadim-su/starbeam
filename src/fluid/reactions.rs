use bevy::prelude::*;
use serde::Deserialize;

use crate::fluid::cell::FluidId;
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
}
