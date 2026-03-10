use bevy::prelude::*;

use crate::combat::DamageEvent;
use crate::enemy::components::{ContactDamage, Enemy};
use crate::physics::TileCollider;
use crate::player::Player;

/// Checks AABB overlap between enemies with `ContactDamage` and the player,
/// sending a `DamageEvent` on contact.
pub fn contact_damage_system(
    player_query: Query<(Entity, &Transform, &TileCollider), With<Player>>,
    enemy_query: Query<(&Transform, &TileCollider, &ContactDamage), With<Enemy>>,
    mut damage_writer: bevy::ecs::message::MessageWriter<DamageEvent>,
) {
    let Ok((player_entity, player_tf, player_col)) = player_query.single() else {
        return;
    };
    let pp = player_tf.translation.truncate();
    let pw = player_col.width * 0.5;
    let ph = player_col.height * 0.5;

    for (enemy_tf, enemy_col, contact) in &enemy_query {
        let ep = enemy_tf.translation.truncate();
        let ew = enemy_col.width * 0.5;
        let eh = enemy_col.height * 0.5;

        // AABB overlap test
        let overlaps_x = (pp.x - ep.x).abs() < pw + ew;
        let overlaps_y = (pp.y - ep.y).abs() < ph + eh;

        if overlaps_x && overlaps_y {
            let dir = (pp - ep).normalize_or_zero();
            damage_writer.write(DamageEvent {
                target: player_entity,
                amount: contact.0,
                knockback: dir * 200.0,
            });
        }
    }
}
