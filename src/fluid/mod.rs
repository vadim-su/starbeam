use bevy::prelude::*;
use bevy::sprite_render::Material2dPlugin;

pub mod cell;
pub mod debug;
pub mod detectors;
pub mod events;
pub mod fluid_world;
pub mod reactions;
pub mod registry;
pub mod render;
pub mod simulation;
pub mod splash;
pub mod systems;
pub mod wave;

pub use cell::{FluidCell, FluidId};
pub use detectors::{FluidContactState, Projectile};
pub use events::{FluidReactionEvent, ImpactKind, WaterImpactEvent};
pub use reactions::{FluidReactionDef, FluidReactionRegistry};
pub use registry::{FluidDef, FluidRegistry};
pub use simulation::FluidSimConfig;
pub use systems::{ActiveFluidChunks, ChunkFluidMaterials, FluidMaterial, FluidTickAccumulator};

use crate::registry::AppState;
use crate::sets::GameSet;

pub struct FluidPlugin;

impl Plugin for FluidPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<events::WaterImpactEvent>()
            .add_message::<events::FluidReactionEvent>()
            .add_plugins(Material2dPlugin::<FluidMaterial>::default())
            .init_resource::<FluidSimConfig>()
            .init_resource::<systems::FluidTickAccumulator>()
            .init_resource::<debug::FluidPlacementMode>()
            .init_resource::<systems::ActiveFluidChunks>()
            .init_resource::<wave::WaveConfig>()
            .init_resource::<wave::WaveState>()
            .init_resource::<systems::ChunkFluidMaterials>()
            .init_resource::<splash::SplashConfig>()
            .init_resource::<detectors::SwimThrottle>()
            .add_systems(Startup, systems::init_fluid_material)
            .add_systems(
                Update,
                (
                    detectors::detect_entity_water_entry,
                    detectors::detect_entity_swimming,
                    detectors::detect_projectile_in_fluid,
                    systems::fluid_simulation,
                    systems::wave_consume_events,
                    systems::wave_simulation,
                    splash::spawn_splash_particles,
                    splash::reabsorb_particles,
                    systems::fluid_rebuild_meshes,
                )
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
