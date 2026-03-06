pub mod block_action;
pub mod interactable;

use bevy::prelude::*;

use crate::sets::GameSet;
use interactable::{HandCraftOpen, NearbyInteractable, OpenStation};

pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NearbyInteractable>()
            .init_resource::<OpenStation>()
            .init_resource::<HandCraftOpen>()
            .add_systems(
                Update,
                (
                    block_action::block_interaction_system,
                    interactable::detect_nearby_interactable,
                    interactable::handle_interaction_input,
                )
                    .in_set(GameSet::Input),
            );
    }
}
