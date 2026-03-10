pub mod block_damage;
pub mod damage;
pub mod death;
pub mod fall_damage;
pub mod health;
pub mod liquid_damage;

use bevy::prelude::*;
use crate::sets::GameSet;

pub use block_damage::*;
pub use damage::*;
pub use death::*;
pub use health::*;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<block_damage::BlockDamageMap>()
            .add_systems(
                Update,
                block_damage::tick_block_damage_regen.in_set(GameSet::WorldUpdate),
            )
            .add_message::<DamageEvent>()
            .add_message::<PlayerDeathEvent>()
            .add_systems(
                Update,
                (
                    tick_invincibility,
                    damage::process_damage,
                    damage::apply_damage_knockback,
                )
                    .in_set(GameSet::Physics),
            )
            .add_systems(
                Update,
                (death::detect_player_death, death::handle_player_death)
                    .chain()
                    .in_set(GameSet::Physics),
            )
            .add_systems(
                Update,
                (
                    fall_damage::fall_damage_system,
                    liquid_damage::liquid_damage_system,
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
