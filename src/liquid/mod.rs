pub mod data;
pub mod debug;
pub mod registry;
pub mod render;
pub mod simulation;
pub mod sleep;
pub mod system;

pub use data::*;
pub use registry::*;
pub use render::{DirtyLiquidChunks, LiquidMaterial, LiquidMeshEntity, SharedLiquidMaterial};
pub use system::LiquidSimState;

use crate::registry::AppState;
use crate::sets::GameSet;
use bevy::prelude::*;

pub struct LiquidPlugin;

impl Plugin for LiquidPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LiquidRegistry>()
            .init_resource::<LiquidSimState>()
            .init_resource::<DirtyLiquidChunks>()
            .init_resource::<debug::DebugLiquidType>()
            .init_resource::<debug::LiquidDebugState>()
            .add_systems(OnEnter(AppState::InGame), render::init_liquid_material)
            .add_systems(
                Update,
                system::liquid_simulation_system.in_set(GameSet::WorldUpdate),
            )
            .add_systems(
                Update,
                render::rebuild_liquid_meshes
                    .in_set(GameSet::WorldUpdate)
                    .after(system::liquid_simulation_system),
            )
            .add_systems(
                Update,
                debug::debug_liquid_keys.in_set(GameSet::WorldUpdate),
            )
            .add_systems(
                Update,
                (debug::toggle_liquid_debug, debug::draw_liquid_debug_panel)
                    .chain()
                    .in_set(GameSet::Ui),
            );
    }
}
