pub mod block_damage;
pub mod damage;
pub mod death;
pub mod fall_damage;
pub mod health;
pub mod liquid_damage;
pub mod melee;
pub mod projectile;
pub mod ranged;

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
            )
            .add_systems(
                Update,
                (
                    melee::melee_attack_system,
                    ranged::ranged_attack_system,
                )
                    .in_set(GameSet::Input),
            )
            .add_systems(
                Update,
                (
                    projectile::tick_projectiles,
                    projectile::projectile_hit_detection,
                )
                    .in_set(GameSet::Physics),
            )
            .add_systems(
                Update,
                invincibility_flash.in_set(GameSet::Ui),
            );
    }
}

fn invincibility_flash(
    mut query: Query<(&InvincibilityTimer, &mut Visibility)>,
) {
    for (timer, mut visibility) in &mut query {
        // Flash every 0.1s
        let flash = (timer.remaining * 10.0) as i32 % 2 == 0;
        *visibility = if flash { Visibility::Visible } else { Visibility::Hidden };
    }
}

fn tick_invincibility(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut InvincibilityTimer, Option<&mut Visibility>)>,
) {
    let dt = time.delta_secs();
    for (entity, mut timer, visibility) in &mut query {
        timer.remaining -= dt;
        if timer.remaining <= 0.0 {
            commands.entity(entity).remove::<InvincibilityTimer>();
            // Restore visibility when invincibility ends
            if let Some(mut vis) = visibility {
                *vis = Visibility::Visible;
            }
        }
    }
}
