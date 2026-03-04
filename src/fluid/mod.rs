use bevy::prelude::*;
use bevy::sprite_render::Material2dPlugin;

pub mod cell;
pub mod debug;
pub mod debug_overlay;
pub mod detectors;
pub mod events;
pub mod fluid_world;
pub mod reactions;
pub mod registry;
pub mod render;
pub mod simulation;
pub mod spatial_hash;
pub mod sph_collision;
pub mod sph_kernels;
pub mod sph_particle;
pub mod sph_render;
pub mod sph_simulation;
pub mod splash;
pub mod systems;
pub mod wave;

pub use cell::{FluidCell, FluidId};
pub use detectors::{FluidContactState, Projectile};
pub use events::{FluidReactionEvent, ImpactKind, WaterImpactEvent};
pub use reactions::{FluidReactionDef, FluidReactionRegistry};
pub use registry::{FluidDef, FluidRegistry};
pub use systems::{FluidMaterial, FluidTickAccumulator};

use crate::registry::AppState;
use crate::sets::GameSet;

pub struct FluidPlugin;

impl Plugin for FluidPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<events::WaterImpactEvent>()
            .add_message::<events::FluidReactionEvent>()
            .add_plugins(Material2dPlugin::<FluidMaterial>::default())
            .init_resource::<systems::FluidTickAccumulator>()
            .init_resource::<detectors::SwimThrottle>()
            .init_resource::<debug_overlay::FluidDebugState>()
            .init_resource::<sph_particle::ParticleStore>()
            .init_resource::<sph_simulation::SphConfig>()
            .add_systems(Startup, systems::init_fluid_material)
            .add_systems(
                Update,
                (
                    detectors::detect_entity_water_entry,
                    detectors::detect_entity_swimming,
                    detectors::detect_projectile_in_fluid,
                    systems::sph_fluid_simulation,
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
                (
                    debug::debug_place_fluid,
                    debug_overlay::toggle_fluid_debug,
                )
                    .in_set(GameSet::Input)
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<FluidRegistry>),
            )
            .add_systems(
                bevy_egui::EguiPrimaryContextPass,
                debug_overlay::draw_fluid_debug_panel
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<FluidRegistry>),
            );
    }
}
