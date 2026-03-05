pub mod data;
pub mod registry;
pub mod simulation;
pub mod sleep;
pub mod system;

pub use data::*;
pub use registry::*;
pub use system::LiquidSimState;

use crate::sets::GameSet;
use bevy::prelude::*;

pub struct LiquidPlugin;

impl Plugin for LiquidPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LiquidRegistry>()
            .init_resource::<LiquidSimState>()
            .add_systems(
                Update,
                system::liquid_simulation_system.in_set(GameSet::WorldUpdate),
            );
    }
}
