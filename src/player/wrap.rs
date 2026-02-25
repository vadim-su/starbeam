use bevy::prelude::*;

use crate::player::Player;
use crate::registry::world::WorldConfig;

/// Teleport player when they cross the horizontal world boundary.
pub fn player_wrap_system(
    world_config: Res<WorldConfig>,
    mut query: Query<&mut Transform, With<Player>>,
) {
    let world_w = world_config.world_pixel_width();
    for mut transform in &mut query {
        let pos = &mut transform.translation;
        if pos.x < 0.0 {
            pos.x += world_w;
        } else if pos.x >= world_w {
            pos.x -= world_w;
        }
    }
}
