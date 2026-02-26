pub mod config;
pub mod spawn;

use bevy::prelude::*;

use crate::camera::follow::camera_follow_player;
use crate::registry::AppState;

pub struct ParallaxPlugin;

impl Plugin for ParallaxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn::spawn_parallax_layers)
            .add_systems(
                Update,
                spawn::spawn_parallax_layers
                    .after(camera_follow_player)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
