use bevy::prelude::*;

pub mod cell;
pub mod reactions;
pub mod registry;

pub use cell::{FluidCell, FluidContactState, FluidId};
pub use reactions::{FluidReactionDef, FluidReactionRegistry};
pub use registry::{FluidDef, FluidRegistry};

pub struct FluidPlugin;

impl Plugin for FluidPlugin {
    fn build(&self, _app: &mut App) {
        // Fluid physics removed. Registry types (FluidRegistry, FluidReactionRegistry)
        // are loaded by the registry/loading pipeline via RON assets.
    }
}
