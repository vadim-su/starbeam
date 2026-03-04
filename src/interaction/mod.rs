pub mod block_action;
pub mod debug_fluid;

use bevy::prelude::*;

use crate::registry::AppState;
use crate::sets::GameSet;

pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                block_action::block_interaction_system,
                debug_fluid::debug_fluid_place_system,
            )
                .in_set(GameSet::Input)
                .run_if(in_state(AppState::InGame)),
        );
    }
}
