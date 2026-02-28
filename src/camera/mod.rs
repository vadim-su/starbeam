pub mod follow;

use bevy::prelude::*;

use crate::sets::GameSet;

const CAMERA_SCALE: f32 = 1.0; // TODO: restore to 0.7 after lighting debug

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, follow::camera_follow_player.in_set(GameSet::Camera));
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: CAMERA_SCALE,
            ..OrthographicProjection::default_2d()
        }),
    ));
}
