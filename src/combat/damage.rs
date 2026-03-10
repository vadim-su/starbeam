use bevy::prelude::*;

use super::{Health, InvincibilityTimer};
use crate::physics::Velocity;

const INVINCIBILITY_DURATION: f32 = 0.5;

#[derive(Message, Debug)]
pub struct DamageEvent {
    pub target: Entity,
    pub amount: f32,
    pub knockback: Vec2,
}

pub fn process_damage(
    mut commands: Commands,
    mut reader: bevy::ecs::message::MessageReader<DamageEvent>,
    mut query: Query<(&mut Health, Option<&InvincibilityTimer>)>,
) {
    for event in reader.read() {
        let Ok((mut health, invincibility)) = query.get_mut(event.target) else {
            continue;
        };
        if invincibility.is_some() {
            continue;
        }
        health.take_damage(event.amount);
        commands
            .entity(event.target)
            .insert(InvincibilityTimer::new(INVINCIBILITY_DURATION));
    }
}

pub fn apply_damage_knockback(
    mut reader: bevy::ecs::message::MessageReader<DamageEvent>,
    mut query: Query<(&mut Velocity, Option<&InvincibilityTimer>)>,
) {
    for event in reader.read() {
        let Ok((mut vel, invincibility)) = query.get_mut(event.target) else {
            continue;
        };
        if invincibility.is_some() {
            continue;
        }
        vel.x += event.knockback.x;
        vel.y += event.knockback.y;
    }
}
