pub mod ai;
pub mod components;
pub mod loot;
pub mod slime;
pub mod spawner;

use bevy::prelude::*;

pub use ai::*;
pub use components::*;
pub use loot::*;
pub use spawner::MobSpawnConfig;

use crate::sets::GameSet;

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MobSpawnConfig>()
            .add_systems(Update, ai::enemy_ai_tick.in_set(GameSet::Input))
            .add_systems(Update, loot::enemy_death_system.in_set(GameSet::WorldUpdate))
            .add_systems(
                Update,
                slime::contact_damage_system.in_set(GameSet::Physics),
            )
            .add_systems(
                Update,
                spawner::mob_spawn_system.in_set(GameSet::WorldUpdate),
            );
    }
}
