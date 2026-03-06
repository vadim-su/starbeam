use bevy::prelude::*;

use crate::physics::{Grounded, Submerged, Velocity, MAX_DELTA_SECS};
use crate::player::Player;
use crate::registry::player::PlayerConfig;

pub fn player_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    player_config: Res<PlayerConfig>,
    mut query: Query<(&mut Velocity, &Grounded, &Submerged), With<Player>>,
) {
    let dt = time.delta_secs().min(MAX_DELTA_SECS);

    for (mut vel, grounded, submerged) in &mut query {
        if submerged.is_swimming() {
            // --- Swimming mode ---
            let swim_speed = player_config.speed * submerged.swim_speed_factor;

            // Horizontal movement
            vel.x = 0.0;
            if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
                vel.x -= swim_speed;
            }
            if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
                vel.x += swim_speed;
            }

            // Vertical swimming: W/Space = up, S = down
            if keys.pressed(KeyCode::Space)
                || keys.pressed(KeyCode::KeyW)
                || keys.pressed(KeyCode::ArrowUp)
            {
                vel.y += player_config.swim_impulse * dt;
            }
            if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
                vel.y -= player_config.swim_impulse * dt;
            }

            // FPS-independent drag (exponential decay)
            let drag = player_config.swim_drag.powf(dt);
            vel.x *= drag;
            vel.y *= drag;
        } else {
            // --- Normal ground/air mode ---
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
}
