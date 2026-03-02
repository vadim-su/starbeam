use bevy::prelude::*;
use bevy::sprite_render::Material2dPlugin;

pub mod cell;
pub mod debug;
pub mod events;
pub mod reactions;
pub mod registry;
pub mod render;
pub mod simulation;
pub mod systems;

pub use cell::{FluidCell, FluidId};
pub use events::{ImpactKind, WaterImpactEvent};
pub use reactions::{FluidReactionDef, FluidReactionRegistry};
pub use registry::{FluidDef, FluidRegistry};
pub use render::build_fluid_mesh;
pub use simulation::FluidSimConfig;
pub use systems::{ActiveFluidChunks, FluidMaterial};

use crate::registry::AppState;
use crate::sets::GameSet;

pub struct FluidPlugin;

impl Plugin for FluidPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<events::WaterImpactEvent>()
            .add_plugins(Material2dPlugin::<FluidMaterial>::default())
            .init_resource::<FluidSimConfig>()
            .init_resource::<systems::ActiveFluidChunks>()
            .add_systems(Startup, systems::init_fluid_material)
            .add_systems(
                Update,
                (systems::fluid_simulation, systems::fluid_rebuild_meshes)
                    .chain()
                    .in_set(GameSet::WorldUpdate)
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<FluidRegistry>)
                    .run_if(resource_exists::<systems::SharedFluidMaterial>),
            )
            .add_systems(
                Update,
                systems::update_fluid_time
                    .in_set(GameSet::WorldUpdate)
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<systems::SharedFluidMaterial>),
            )
            .add_systems(
                Update,
                debug::debug_place_fluid
                    .in_set(GameSet::Input)
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<FluidRegistry>),
            );
    }
}
