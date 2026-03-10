pub mod components;
pub mod spawn;

use bevy::prelude::*;
pub use components::*;

use crate::registry::AppState;

pub struct TraderPlugin;

#[derive(Resource, Default)]
pub struct OpenTrader(pub Option<Entity>);

impl Plugin for TraderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OpenTrader>()
            .add_systems(OnEnter(AppState::InGame), spawn::spawn_trader);
    }
}
