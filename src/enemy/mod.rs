pub mod ai;
pub mod components;
pub mod loot;

use bevy::prelude::*;

pub use ai::*;
pub use components::*;
pub use loot::*;

use crate::sets::GameSet;

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, ai::enemy_ai_tick.in_set(GameSet::Input));
        app.add_systems(Update, loot::enemy_death_system.in_set(GameSet::WorldUpdate));
    }
}
