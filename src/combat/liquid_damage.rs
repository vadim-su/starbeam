use bevy::prelude::*;

use super::DamageEvent;
use crate::combat::Health;
use crate::liquid::registry::LiquidRegistry;
use crate::physics::Submerged;

pub fn liquid_damage_system(
    time: Res<Time>,
    liquid_registry: Res<LiquidRegistry>,
    mut writer: bevy::ecs::message::MessageWriter<DamageEvent>,
    query: Query<(Entity, &Submerged), With<Health>>,
) {
    let dt = time.delta_secs();
    for (entity, sub) in &query {
        if sub.ratio < 0.01 || sub.liquid_id.is_none() {
            continue;
        }
        let Some(def) = liquid_registry.get(sub.liquid_id) else {
            continue;
        };
        if def.damage_on_contact <= 0.0 {
            continue;
        }
        writer.write(DamageEvent {
            target: entity,
            amount: def.damage_on_contact * dt,
            knockback: Vec2::ZERO,
        });
    }
}
