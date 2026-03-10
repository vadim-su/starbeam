pub mod damage;
pub mod health;

use bevy::prelude::*;
use crate::sets::GameSet;

pub use damage::*;
pub use health::*;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DamageEvent>()
            .add_systems(
                Update,
                (
                    tick_invincibility,
                    damage::process_damage,
                    damage::apply_damage_knockback,
                )
                    .in_set(GameSet::Physics),
            );
    }
}

fn tick_invincibility(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut InvincibilityTimer)>,
) {
    let dt = time.delta_secs();
    for (entity, mut timer) in &mut query {
        timer.remaining -= dt;
        if timer.remaining <= 0.0 {
            commands.entity(entity).remove::<InvincibilityTimer>();
        }
    }
}
