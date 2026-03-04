pub mod cell;
pub mod definition;

pub use cell::{FluidCell, FluidId};
pub use definition::{FluidDef, FluidRegistry};

use bevy::prelude::*;
use bevy::reflect::TypePath;
use serde::Deserialize;

/// Asset loaded from fluids.registry.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct FluidRegistryAsset {
    pub fluids: Vec<FluidDef>,
}

pub struct FluidPlugin;

impl Plugin for FluidPlugin {
    fn build(&self, _app: &mut App) {
        // FluidRegistry is inserted by RegistryPlugin during loading.
        // Simulation systems will be added in Phase 2.
    }
}
