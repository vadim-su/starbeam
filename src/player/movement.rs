use bevy::prelude::*;

use crate::player::{Grounded, Player, Velocity, GRAVITY, JUMP_VELOCITY, PLAYER_SPEED};

pub fn player_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut Velocity, &Grounded), With<Player>>,
) {
    for (mut vel, grounded) in &mut query {
        vel.x = 0.0;
        if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
            vel.x -= PLAYER_SPEED;
        }
        if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
            vel.x += PLAYER_SPEED;
        }
        if keys.just_pressed(KeyCode::Space) && grounded.0 {
            vel.y = JUMP_VELOCITY;
        }
    }
}

pub fn apply_gravity(time: Res<Time>, mut query: Query<&mut Velocity, With<Player>>) {
    let dt = time.delta_secs();
    for mut vel in &mut query {
        vel.y -= GRAVITY * dt;
    }
}

pub fn apply_velocity(
    time: Res<Time>,
    mut query: Query<(&Velocity, &mut Transform), With<Player>>,
) {
    let dt = time.delta_secs();
    for (vel, mut transform) in &mut query {
        transform.translation.x += vel.x * dt;
        transform.translation.y += vel.y * dt;
    }
}
