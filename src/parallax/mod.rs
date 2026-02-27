pub mod config;
pub mod scroll;
pub mod spawn;
pub mod transition;

use bevy::prelude::*;

use crate::camera::follow::camera_follow_player;
use crate::registry::AppState;

pub struct ParallaxPlugin;

impl Plugin for ParallaxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                transition::track_player_biome,
                transition::parallax_transition_system,
                scroll::parallax_scroll,
            )
                .chain()
                .after(camera_follow_player)
                .run_if(in_state(AppState::InGame)),
        );
    }
}
