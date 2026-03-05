pub mod data;
pub mod registry;

pub use data::*;
pub use registry::*;

use bevy::prelude::*;

pub struct LiquidPlugin;

impl Plugin for LiquidPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LiquidRegistry>();
    }
}
