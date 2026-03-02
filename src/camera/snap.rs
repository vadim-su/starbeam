//! One-shot camera snap that runs on `OnEnter(InGame)`.
//!
//! Ensures the camera is positioned on the player *before* the first `Update`
//! frame.  Without this, `chunk_loading_system` (which runs in
//! `GameSet::WorldUpdate`, before `GameSet::Camera`) would use the stale camera
//! position from the previous world, loading chunks in the wrong area and
//! causing a lightmap-coverage mismatch (permanent darkness on first chunks).

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::player::Player;
use crate::registry::world::ActiveWorld;

/// Immediately places the camera at the player position with proper Y clamping.
///
/// Mirrors the logic of [`super::follow::camera_follow_player`] so that the
/// very first `Update` frame already has a correct camera transform.
pub fn snap_camera_to_player(
    player_query: Query<&Transform, (With<Player>, Without<Camera2d>)>,
    mut camera_query: Query<(&mut Transform, &Projection), (With<Camera2d>, Without<Player>)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    world_config: Res<ActiveWorld>,
) {
    let Ok(player_tf) = player_query.single() else {
        return;
    };
    let Ok((mut cam_tf, projection)) = camera_query.single_mut() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

    let proj_scale = match projection {
        Projection::Orthographic(ortho) => ortho.scale,
        _ => 1.0,
    };

    let half_h = window.height() / 2.0 * proj_scale;
    let world_h = world_config.world_pixel_height();

    let mut target = player_tf.translation;
    target.y = target.y.clamp(half_h, (world_h - half_h).max(half_h));

    let pixel = proj_scale;
    cam_tf.translation.x = (target.x / pixel).round() * pixel;
    cam_tf.translation.y = (target.y / pixel).round() * pixel;
}
