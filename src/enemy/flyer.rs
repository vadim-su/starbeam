use bevy::prelude::*;

use crate::enemy::ai::{AiStateMachine, State};
use crate::enemy::components::*;
use crate::physics::Velocity;

/// Apply a sinusoidal bobbing motion to Flyer enemies when idle,
/// giving them a floating appearance.
pub fn flyer_bob_system(
    time: Res<Time>,
    mut query: Query<(&EnemyType, &mut Velocity, &AiStateMachine)>,
) {
    let t = time.elapsed_secs();

    for (enemy_type, mut vel, ai) in &mut query {
        if *enemy_type != EnemyType::Flyer {
            continue;
        }

        // Only apply bobbing when idle
        if !matches!(ai.machine.state(), State::Idle {}) {
            continue;
        }

        // Sinusoidal vertical bobbing
        let bob = (t * 2.5).sin() * 20.0;
        vel.y += bob;
    }
}
