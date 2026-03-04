pub mod active;
pub mod cell;
pub mod definition;
pub mod displacement;
pub mod simulation;

pub use active::ActiveFluids;
pub use cell::{FluidCell, FluidId};
pub use definition::{FluidDef, FluidRegistry};

use bevy::prelude::*;
use bevy::reflect::TypePath;
use serde::Deserialize;

use crate::registry::AppState;
use crate::sets::GameSet;

/// Asset loaded from fluids.fluids.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct FluidRegistryAsset {
    pub fluids: Vec<FluidDef>,
}

pub struct FluidPlugin;

impl Plugin for FluidPlugin {
    fn build(&self, app: &mut App) {
        // FluidRegistry is inserted by RegistryPlugin during loading.
        app.init_resource::<ActiveFluids>()
            .init_resource::<simulation::FluidTickTimer>()
            .add_systems(
                Update,
                simulation::fluid_simulation_system
                    .in_set(GameSet::Physics)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
