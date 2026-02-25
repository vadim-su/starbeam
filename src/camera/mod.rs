pub mod follow;

use bevy::prelude::*;

use crate::player::wrap::player_wrap_system;
use crate::registry::AppState;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            follow::camera_follow_player
                .after(player_wrap_system)
                .run_if(in_state(AppState::InGame)),
        );
    }
}
