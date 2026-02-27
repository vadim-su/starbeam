use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::player::Player;
use crate::registry::world::WorldConfig;

#[allow(clippy::type_complexity)]
pub fn camera_follow_player(
    player_query: Query<&Transform, (With<Player>, Without<Camera2d>)>,
    mut camera_query: Query<(&mut Transform, &Projection), (With<Camera2d>, Without<Player>)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    world_config: Res<WorldConfig>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let Ok((mut camera_transform, projection)) = camera_query.single_mut() else {
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

    let mut target = player_transform.translation;
    target.y = target.y.clamp(half_h, (world_h - half_h).max(half_h));

    camera_transform.translation.x = target.x;
    camera_transform.translation.y = target.y;
}
