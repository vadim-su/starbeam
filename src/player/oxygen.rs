use bevy::prelude::*;

use crate::combat::DamageEvent;
use crate::cosmos::pressurization::InVacuum;
use crate::player::Player;

/// Damage per second when oxygen is fully depleted.
const OXYGEN_DEPLETION_DPS: f32 = 10.0;

/// Oxygen supply for the player. Drains in vacuum, refills in atmosphere.
#[derive(Component, Debug)]
pub struct Oxygen {
    pub current: f32,
    pub max: f32,
    /// Units lost per second while in vacuum.
    pub drain_rate: f32,
    /// Units gained per second while in atmosphere.
    pub refill_rate: f32,
}

impl Default for Oxygen {
    fn default() -> Self {
        Self {
            current: 100.0,
            max: 100.0,
            drain_rate: 5.0,   // 20 seconds in vacuum
            refill_rate: 20.0, // 5 seconds to refill
        }
    }
}

/// Drains or refills oxygen based on whether the player is in vacuum.
pub fn tick_oxygen(
    time: Res<Time>,
    mut writer: bevy::ecs::message::MessageWriter<DamageEvent>,
    mut query: Query<(Entity, &InVacuum, &mut Oxygen), With<Player>>,
) {
    let dt = time.delta_secs();
    for (entity, in_vacuum, mut oxygen) in &mut query {
        if in_vacuum.0 {
            oxygen.current = (oxygen.current - oxygen.drain_rate * dt).max(0.0);
            if oxygen.current <= 0.0 {
                writer.write(DamageEvent {
                    target: entity,
                    amount: OXYGEN_DEPLETION_DPS * dt,
                    knockback: Vec2::ZERO,
                });
            }
        } else {
            oxygen.current = (oxygen.current + oxygen.refill_rate * dt).min(oxygen.max);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oxygen_drains_in_vacuum() {
        let mut oxygen = Oxygen::default();
        let dt = 1.0;
        oxygen.current -= oxygen.drain_rate * dt;
        assert_eq!(oxygen.current, 95.0);
    }

    #[test]
    fn oxygen_refills_in_atmosphere() {
        let mut oxygen = Oxygen {
            current: 50.0,
            ..Default::default()
        };
        let dt = 1.0;
        oxygen.current = (oxygen.current + oxygen.refill_rate * dt).min(oxygen.max);
        assert_eq!(oxygen.current, 70.0);
    }

    #[test]
    fn oxygen_does_not_go_below_zero() {
        let mut oxygen = Oxygen {
            current: 2.0,
            ..Default::default()
        };
        oxygen.current = (oxygen.current - oxygen.drain_rate * 10.0).max(0.0);
        assert_eq!(oxygen.current, 0.0);
    }

    #[test]
    fn oxygen_does_not_exceed_max() {
        let mut oxygen = Oxygen {
            current: 99.0,
            ..Default::default()
        };
        let dt = 1.0;
        oxygen.current = (oxygen.current + oxygen.refill_rate * dt).min(oxygen.max);
        assert_eq!(oxygen.current, 100.0);
    }
}
