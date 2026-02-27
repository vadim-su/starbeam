pub mod follow;

use bevy::prelude::*;

use crate::sets::GameSet;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, follow::camera_follow_player.in_set(GameSet::Camera));
    }
}
