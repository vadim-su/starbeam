use bevy::prelude::*;

use crate::enemy::Enemy;
use crate::inventory::Hotbar;
use crate::item::ItemRegistry;
use crate::player::Player;

use super::DamageEvent;

#[derive(Component, Debug)]
pub struct MeleeAttack {
    pub damage: f32,
    pub knockback: f32,
    pub range: f32,
    pub cooldown: f32,
    pub timer: f32,
}

impl Default for MeleeAttack {
    fn default() -> Self {
        Self {
            damage: 5.0,
            knockback: 200.0,
            range: 48.0,
            cooldown: 0.4,
            timer: 0.0,
        }
    }
}

pub fn melee_attack_system(
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    item_registry: Option<Res<ItemRegistry>>,
    mut writer: bevy::ecs::message::MessageWriter<DamageEvent>,
    mut player_query: Query<(&Transform, &Hotbar, &mut MeleeAttack), With<Player>>,
    enemy_query: Query<(Entity, &Transform), With<Enemy>>,
) {
    let dt = time.delta_secs();

    for (player_tf, hotbar, mut melee) in &mut player_query {
        melee.timer -= dt;
        if melee.timer > 0.0 || !mouse.just_pressed(MouseButton::Left) {
            continue;
        }

        // Resolve damage/knockback from active hotbar item or use defaults
        let mut damage = melee.damage;
        let mut knockback = melee.knockback;
        let mut cooldown = melee.cooldown;

        if let Some(ref registry) = item_registry {
            // Check right hand first (primary), then left hand
            let item_name = hotbar
                .get_item_for_hand(false)
                .or_else(|| hotbar.get_item_for_hand(true));

            if let Some(name) = item_name {
                if let Some(item_id) = registry.by_name(name) {
                    let def = registry.get(item_id);
                    if let Some(ref stats) = def.stats {
                        if let Some(d) = stats.damage {
                            damage = d;
                        }
                        if let Some(k) = stats.knockback {
                            knockback = k;
                        }
                        if let Some(speed) = stats.attack_speed {
                            if speed > 0.0 {
                                cooldown = 1.0 / speed;
                            }
                        }
                    }
                }
            }
        }

        let player_pos = player_tf.translation.truncate();
        let range_sq = melee.range * melee.range;

        for (enemy_entity, enemy_tf) in &enemy_query {
            let enemy_pos = enemy_tf.translation.truncate();
            let diff = enemy_pos - player_pos;
            let dist_sq = diff.length_squared();

            if dist_sq > range_sq || dist_sq < f32::EPSILON {
                continue;
            }

            let dir = diff.normalize();
            writer.write(DamageEvent {
                target: enemy_entity,
                amount: damage,
                knockback: dir * knockback,
            });
        }

        melee.timer = cooldown;
    }
}
