use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::player::Player;
use crate::registry::world::ActiveWorld;

#[allow(clippy::type_complexity)]
pub fn camera_follow_player(
    player_query: Query<&Transform, (With<Player>, Without<Camera2d>)>,
    mut camera_query: Query<(&mut Transform, &Projection), (With<Camera2d>, Without<Player>)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    world_config: Res<ActiveWorld>,
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

    // Clamp camera X for non-wrapping worlds so it doesn't scroll past edges
    if !world_config.wrap_x {
        let half_w = window.width() / 2.0 * proj_scale;
        let world_w = world_config.world_pixel_width();
        target.x = target.x.clamp(half_w, (world_w - half_w).max(half_w));
    }

    // Snap camera to pixel grid to prevent subpixel texture shimmer.
    // One screen pixel = proj_scale world units.
    let pixel = proj_scale;
    camera_transform.translation.x = (target.x / pixel).round() * pixel;
    camera_transform.translation.y = (target.y / pixel).round() * pixel;
}
