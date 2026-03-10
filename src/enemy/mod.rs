pub mod ai;
pub mod components;

use bevy::prelude::*;

pub use ai::*;
pub use components::*;

use crate::sets::GameSet;

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, ai::enemy_ai_tick.in_set(GameSet::Input));
    }
}
