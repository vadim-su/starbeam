use bevy::prelude::*;

use super::DamageEvent;
use crate::physics::{Grounded, Velocity};

/// Velocity threshold (pixels/s) below which fall damage kicks in.
const FALL_DAMAGE_THRESHOLD: f32 = -600.0;

/// Damage per unit of excess speed beyond the threshold.
const DAMAGE_PER_SPEED: f32 = 0.1;

/// Tracks the previous frame's vertical velocity so we can detect landings.
#[derive(Component, Debug, Default)]
pub struct FallTracker {
    pub prev_vel_y: f32,
    pub was_grounded: bool,
}

pub fn fall_damage_system(
    mut writer: bevy::ecs::message::MessageWriter<DamageEvent>,
    mut query: Query<(Entity, &Velocity, &Grounded, &mut FallTracker)>,
) {
    for (entity, vel, grounded, mut tracker) in &mut query {
        let just_landed = grounded.0 && !tracker.was_grounded;

        if just_landed && tracker.prev_vel_y < FALL_DAMAGE_THRESHOLD {
            let excess = tracker.prev_vel_y - FALL_DAMAGE_THRESHOLD; // negative
            let damage = (-excess) * DAMAGE_PER_SPEED;
            writer.write(DamageEvent {
                target: entity,
                amount: damage,
                knockback: Vec2::ZERO,
            });
        }

        tracker.prev_vel_y = vel.y;
        tracker.was_grounded = grounded.0;
    }
}
