pub mod block_action;

use bevy::prelude::*;

use crate::sets::GameSet;

pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            block_action::block_interaction_system.in_set(GameSet::Input),
        );
    }
}
