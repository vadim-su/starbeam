use bevy::prelude::*;
use bevy::sprite_render::Material2dPlugin;

pub mod cell;
pub mod material;
pub mod reactions;
pub mod registry;
pub mod render;
pub mod simulation;

pub use cell::{FluidCell, FluidContactState, FluidId};
pub use material::{FluidMaterial, SharedFluidMaterial};
pub use reactions::{FluidReactionDef, FluidReactionRegistry};
pub use registry::{FluidDef, FluidRegistry};
pub use render::{FluidChunkMarker, FluidDirty, FluidMeshBuffers};
pub use simulation::{DirtyFluidChunks, FluidSimConfig, FluidSimState};

use crate::registry::AppState;
use crate::sets::GameSet;

pub struct FluidPlugin;

impl Plugin for FluidPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<FluidMaterial>::default())
            .init_resource::<FluidSimConfig>()
            .init_resource::<FluidSimState>()
            .init_resource::<DirtyFluidChunks>()
            .init_resource::<FluidMeshBuffers>()
            .add_systems(
                OnEnter(AppState::InGame),
                render::init_fluid_material,
            )
            .add_systems(
                Update,
                simulation::fluid_simulation_step.in_set(GameSet::Physics),
            )
            .add_systems(
                Update,
                (render::rebuild_fluid_meshes, render::update_fluid_material)
                    .in_set(GameSet::WorldUpdate),
            );
    }
}
