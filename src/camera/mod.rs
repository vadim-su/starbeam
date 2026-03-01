pub mod follow;

use bevy::ecs::message::MessageReader;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;

use crate::sets::GameSet;

const CAMERA_SCALE: f32 = 1.0;
const ZOOM_MIN: f32 = 0.3;
const ZOOM_MAX: f32 = 3.0;
/// Each scroll tick multiplies/divides scale by this factor.
const ZOOM_SPEED: f32 = 1.1;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera).add_systems(
            Update,
            (
                camera_zoom.in_set(GameSet::Camera),
                follow::camera_follow_player
                    .after(camera_zoom)
                    .in_set(GameSet::Camera),
            ),
        );
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

fn camera_zoom(
    mut scroll_events: MessageReader<MouseWheel>,
    mut camera_query: Query<&mut Projection, With<Camera2d>>,
) {
    let total: f32 = scroll_events.read().map(|e| e.y).sum();
    if total == 0.0 {
        return;
    }
    let Ok(mut projection) = camera_query.single_mut() else {
        return;
    };
    let Projection::Orthographic(ref mut ortho) = *projection else {
        return;
    };
    // Scroll up (positive y) → zoom in (smaller scale)
    let factor = ZOOM_SPEED.powf(-total);
    ortho.scale = (ortho.scale * factor).clamp(ZOOM_MIN, ZOOM_MAX);
}
