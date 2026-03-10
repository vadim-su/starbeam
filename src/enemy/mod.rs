pub mod components;

use bevy::prelude::*;

pub use components::*;

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, _app: &mut App) {
        // Will be extended in subsequent tasks
    }
}
