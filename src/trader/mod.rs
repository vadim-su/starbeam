pub mod components;

use bevy::prelude::*;
pub use components::*;

pub struct TraderPlugin;

#[derive(Resource, Default)]
pub struct OpenTrader(pub Option<Entity>);

impl Plugin for TraderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OpenTrader>();
    }
}
