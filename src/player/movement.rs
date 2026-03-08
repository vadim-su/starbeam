use bevy::prelude::*;

use crate::cosmos::pressurization::InVacuum;
use crate::physics::{Grounded, Submerged, Velocity, MAX_DELTA_SECS};
use crate::player::Player;
use crate::registry::player::PlayerConfig;

/// EVA jetpack impulse (px/s^2) when pressing movement keys in vacuum.
const EVA_IMPULSE: f32 = 200.0;

/// EVA drag: per-second velocity retention in vacuum (0.0 = instant stop, 1.0 = no drag).
/// Slightly higher than swim drag for a floaty feel.
const EVA_DRAG: f32 = 0.25;

pub fn player_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    player_config: Res<PlayerConfig>,
    mut query: Query<(&mut Velocity, &Grounded, &Submerged, Option<&InVacuum>), With<Player>>,
    chat_state: Res<crate::chat::ChatState>,
) {
    if chat_state.is_active {
        return;
    }

    let dt = time.delta_secs().min(MAX_DELTA_SECS);

    for (mut vel, grounded, submerged, in_vacuum) in &mut query {
        let is_in_vacuum = in_vacuum.is_some_and(|v| v.0);

        if is_in_vacuum {
            // --- EVA jetpack mode (zero-g in vacuum) ---
            // WASD gives impulse in all 4 directions
            if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
                vel.x -= EVA_IMPULSE * dt;
            }
            if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
                vel.x += EVA_IMPULSE * dt;
            }
            if keys.pressed(KeyCode::KeyW)
                || keys.pressed(KeyCode::ArrowUp)
                || keys.pressed(KeyCode::Space)
            {
                vel.y += EVA_IMPULSE * dt;
            }
            if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
                vel.y -= EVA_IMPULSE * dt;
            }

            // FPS-independent drag for playability
            let drag = EVA_DRAG.powf(dt);
            vel.x *= drag;
            vel.y *= drag;
        } else if submerged.is_swimming() {
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
