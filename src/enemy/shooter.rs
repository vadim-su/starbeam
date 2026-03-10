use bevy::prelude::*;

use crate::combat::projectile::spawn_projectile;
use crate::enemy::ai::{AiStateMachine, State};
use crate::enemy::components::*;
use crate::player::Player;

/// When a Shooter enemy is in Attack state and its cooldown is ready,
/// fire a projectile toward the player.
pub fn shooter_attack_system(
    time: Res<Time>,
    mut commands: Commands,
    player_query: Query<&Transform, With<Player>>,
    mut enemy_query: Query<
        (
            Entity,
            &Transform,
            &mut AttackCooldown,
            &AttackRange,
            &AiStateMachine,
            &EnemyType,
        ),
        With<Enemy>,
    >,
) {
    let dt = time.delta_secs();
    let Ok(player_tf) = player_query.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();

    for (entity, tf, mut cooldown, attack_range, ai, enemy_type) in &mut enemy_query {
        if *enemy_type != EnemyType::Shooter {
            continue;
        }

        // Tick cooldown
        cooldown.timer -= dt;

        // Check if in Attack state
        if !matches!(ai.machine.state(), State::Attack {}) {
            continue;
        }

        let enemy_pos = tf.translation.truncate();
        let dist = enemy_pos.distance(player_pos);

        // Must be in attack range and cooldown ready
        if dist > attack_range.0 || cooldown.timer > 0.0 {
            continue;
        }

        // Fire projectile toward player
        let direction = (player_pos - enemy_pos).normalize_or_zero();
        spawn_projectile(
            &mut commands,
            enemy_pos,
            direction,
            300.0, // speed
            8.0,   // damage
            3.0,   // knockback
            entity,
        );

        // Reset cooldown
        cooldown.timer = cooldown.duration;
    }
}
