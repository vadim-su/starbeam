pub mod block_action;

use bevy::prelude::*;

use crate::registry::AppState;
use crate::world::chunk;

pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            block_action::block_interaction_system
                .before(chunk::rebuild_dirty_chunks)
                .run_if(in_state(AppState::InGame)),
        );
    }
}
