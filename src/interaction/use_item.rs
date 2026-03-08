use bevy::prelude::*;

use crate::crafting::UnlockedRecipes;
use crate::inventory::{Hotbar, Inventory};
use crate::item::ItemRegistry;
use crate::item::definition::ItemType;
use crate::player::Player;

/// Signals that `use_item_system` consumed an item this frame (e.g. a blueprint),
/// so other right-click systems can skip their processing.
#[derive(Resource, Default)]
pub struct ItemUsedThisFrame(pub bool);

/// Consumes blueprint items from the active hotbar slot on right-click.
pub fn use_item_system(
    mouse: Res<ButtonInput<MouseButton>>,
    mut player_query: Query<(&Hotbar, &mut Inventory, &mut UnlockedRecipes), With<Player>>,
    item_registry: Res<ItemRegistry>,
    mut item_used: ResMut<ItemUsedThisFrame>,
) {
    item_used.0 = false;

    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }

    let Ok((hotbar, mut inventory, mut unlocked)) = player_query.single_mut() else {
        return;
    };

    // Check left hand of active slot
    let Some(item_id) = hotbar.slots[hotbar.active_slot].left_hand.as_deref() else {
        return;
    };

    if inventory.count_item(item_id) == 0 {
        return;
    }

    let Some(def_id) = item_registry.by_name(item_id) else {
        return;
    };
    let def = item_registry.get(def_id);

    if def.item_type != ItemType::Blueprint {
        return;
    }

    let Some(ref item_id_to_unlock) = def.blueprint_item else {
        return;
    };

    // Unlock all recipes gated by Blueprint(item_id) for this item
    unlocked.blueprints.insert(item_id_to_unlock.clone());
    inventory.remove_item(item_id, 1);
    item_used.0 = true;

    info!("Blueprint used: unlocked item '{}'", item_id_to_unlock);
}
