pub mod block_action;
pub mod interactable;
pub mod use_item;

use bevy::prelude::*;

use crate::sets::GameSet;
use interactable::{HandCraftOpen, NearbyInteractable, OpenStation};

/// Internal ordering sets for interaction systems.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum InteractionSet {
    /// Runs first: consume items (e.g. blueprints) on right-click.
    UseItem,
    /// Runs after UseItem: block placement / breaking, interactables, etc.
    BlockAction,
}

pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NearbyInteractable>()
            .init_resource::<OpenStation>()
            .init_resource::<HandCraftOpen>()
            .init_resource::<use_item::ItemUsedThisFrame>()
            .configure_sets(
                Update,
                (InteractionSet::UseItem, InteractionSet::BlockAction)
                    .chain()
                    .in_set(GameSet::Input),
            )
            .add_systems(
                Update,
                use_item::use_item_system.in_set(InteractionSet::UseItem),
            )
            .add_systems(
                Update,
                (
                    block_action::block_interaction_system,
                    interactable::detect_nearby_interactable,
                    interactable::handle_interaction_input,
                    interactable::update_interactable_highlight,
                )
                    .in_set(InteractionSet::BlockAction),
            );
    }
}
