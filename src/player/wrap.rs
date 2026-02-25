use bevy::prelude::*;

use crate::player::Player;
use crate::world::{TILE_SIZE, WORLD_WIDTH_TILES};

/// Teleport player when they cross the horizontal world boundary.
pub fn player_wrap_system(mut query: Query<&mut Transform, With<Player>>) {
    let world_w = WORLD_WIDTH_TILES as f32 * TILE_SIZE;
    for mut transform in &mut query {
        let pos = &mut transform.translation;
        if pos.x < 0.0 {
            pos.x += world_w;
        } else if pos.x >= world_w {
            pos.x -= world_w;
        }
    }
}
