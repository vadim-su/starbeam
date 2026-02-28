use bevy::prelude::*;

use super::components::*;
use crate::inventory::Inventory;
use crate::player::Player;

/// Sync all UI slots with their backing data (Inventory).
pub fn sync_slot_contents(
    inventory_query: Query<&Inventory, With<Player>>,
    mut slot_query: Query<(&UiSlot, &mut BackgroundColor)>,
) {
    let Ok(inventory) = inventory_query.single() else {
        return;
    };

    for (slot, mut bg_color) in &mut slot_query {
        let item_opt = match slot.slot_type {
            SlotType::MainBag(idx) => inventory.main_bag.get(idx).and_then(|s| s.as_ref()),
            SlotType::MaterialBag(idx) => inventory.material_bag.get(idx).and_then(|s| s.as_ref()),
            SlotType::Hotbar { .. } => {
                // Hotbar handled by hotbar::update_hotbar_slots
                continue;
            }
            SlotType::Equipment(_) => {
                // Equipment sync will be added later
                continue;
            }
        };

        // Update background color based on item presence
        if item_opt.is_some() {
            // Has item — show a color (placeholder until icons)
            *bg_color = BackgroundColor(Color::srgb(0.2, 0.4, 0.2));
        } else {
            // Empty slot
            *bg_color = BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0));
        }
    }
}
