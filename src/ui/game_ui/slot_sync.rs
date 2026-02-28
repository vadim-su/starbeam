//! Sync UI slot visuals with backing Inventory data.

use bevy::prelude::*;

use super::components::*;
use crate::inventory::Inventory;
use crate::player::Player;

/// Sync all UI slots with their backing data (Inventory).
pub fn sync_slot_contents(
    inventory_query: Query<&Inventory, With<Player>>,
    mut slot_query: Query<(&UiSlot, &mut BackgroundColor, Option<&Children>)>,
    mut text_query: Query<&mut Text, With<SlotLabel>>,
) {
    let Ok(inventory) = inventory_query.single() else {
        return;
    };

    for (slot, mut bg_color, children) in &mut slot_query {
        let item_opt = match slot.slot_type {
            SlotType::MainBag(idx) => inventory.main_bag.get(idx).and_then(|s| s.as_ref()),
            SlotType::MaterialBag(idx) => inventory.material_bag.get(idx).and_then(|s| s.as_ref()),
            SlotType::Hotbar { .. } => continue,
            SlotType::Equipment(_) => continue,
        };

        if let Some(item) = item_opt {
            // Occupied slot — tinted background
            *bg_color = BackgroundColor(Color::srgb(0.2, 0.4, 0.2));

            // Update count label
            if let Some(children) = children {
                for child in children.iter() {
                    if let Ok(mut text) = text_query.get_mut(child) {
                        *text = Text::new(format!("{}", item.count));
                    }
                }
            }
        } else {
            // Empty slot — transparent
            *bg_color = BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0));

            // Clear label
            if let Some(children) = children {
                for child in children.iter() {
                    if let Ok(mut text) = text_query.get_mut(child) {
                        *text = Text::new("");
                    }
                }
            }
        }
    }
}
