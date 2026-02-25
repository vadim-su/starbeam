use bevy::prelude::*;

use crate::player::{Grounded, Player, Velocity, MAX_DELTA_SECS};
use crate::registry::player::PlayerConfig;

pub fn player_input(
    keys: Res<ButtonInput<KeyCode>>,
    player_config: Res<PlayerConfig>,
    mut query: Query<(&mut Velocity, &Grounded), With<Player>>,
) {
    for (mut vel, grounded) in &mut query {
        vel.x = 0.0;
        if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
            vel.x -= player_config.speed;
        }
        if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
            vel.x += player_config.speed;
        }
        if keys.just_pressed(KeyCode::Space) && grounded.0 {
            vel.y = player_config.jump_velocity;
        }
    }
}

pub fn apply_gravity(
    time: Res<Time>,
    player_config: Res<PlayerConfig>,
    mut query: Query<&mut Velocity, With<Player>>,
) {
    let dt = time.delta_secs().min(MAX_DELTA_SECS);
    for mut vel in &mut query {
        vel.y -= player_config.gravity * dt;
    }
}
