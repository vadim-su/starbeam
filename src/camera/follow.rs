use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::player::Player;
use crate::world::{TILE_SIZE, WORLD_HEIGHT_TILES, WORLD_WIDTH_TILES};

pub fn camera_follow_player(
    player_query: Query<&Transform, (With<Player>, Without<Camera2d>)>,
    mut camera_query: Query<(&mut Transform, &Projection), (With<Camera2d>, Without<Player>)>,
    windows: Query<&Window, With<PrimaryWindow>>,
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
    let half_w = window.width() / 2.0 * proj_scale;
    let half_h = window.height() / 2.0 * proj_scale;
    let world_w = WORLD_WIDTH_TILES as f32 * TILE_SIZE;
    let world_h = WORLD_HEIGHT_TILES as f32 * TILE_SIZE;

    let mut target = player_transform.translation;

    target.x = target.x.clamp(half_w, (world_w - half_w).max(half_w));
    target.y = target.y.clamp(half_h, (world_h - half_h).max(half_h));

    camera_transform.translation.x = target.x;
    camera_transform.translation.y = target.y;
}
