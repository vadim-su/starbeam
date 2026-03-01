//! Sync UI slot visuals with backing Inventory data.

use bevy::prelude::*;
use bevy::ui::widget::ImageNode;

use super::components::{Hand, ItemCount, ItemIcon, SlotFrame, SlotLabel, SlotType, UiSlot};
use super::icon_registry::ItemIconRegistry;
use super::SlotFrames;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::item::ItemRegistry;
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

/// Update slot icons, frames, and counts from inventory/hotbar data.
pub fn update_slot_icons(
    inventory_query: Query<&Inventory, With<Player>>,
    hotbar_query: Query<&Hotbar, With<Player>>,
    _item_registry: Res<ItemRegistry>,
    icon_registry: Res<ItemIconRegistry>,
    slot_frames: Res<SlotFrames>,

    // Query for slots with children
    slot_query: Query<(Entity, &UiSlot), With<Children>>,
    // Child queries
    mut icon_query: Query<&mut ImageNode, With<ItemIcon>>,
    mut frame_query: Query<&mut ImageNode, With<SlotFrame>>,
    mut count_query: Query<&mut Text, With<ItemCount>>,
    mut visibility_query: Query<&mut Visibility, Or<(With<ItemIcon>, With<SlotFrame>)>>,
    children_query: Query<&Children>,
) {
    let Ok(inventory) = inventory_query.single() else {
        return;
    };
    let Ok(hotbar) = hotbar_query.single() else {
        return;
    };

    for (entity, slot) in &slot_query {
        // Get item data for this slot
        let item_data: Option<(&str, u16)> = match slot.slot_type {
            SlotType::MainBag(idx) => inventory
                .main_bag
                .get(idx)
                .and_then(|s| s.as_ref())
                .map(|s| (s.item_id.as_str(), s.count)),
            SlotType::MaterialBag(idx) => inventory
                .material_bag
                .get(idx)
                .and_then(|s| s.as_ref())
                .map(|s| (s.item_id.as_str(), s.count)),
            SlotType::Hotbar { index, hand } => {
                let slot_data = &hotbar.slots[index];
                match hand {
                    Hand::Left => slot_data.left_hand.as_ref(),
                    Hand::Right => slot_data.right_hand.as_ref(),
                }
                .map(|s| (s.item_id.as_str(), s.count))
            }
            SlotType::Equipment(_) => continue,
        };

        // Get children of this slot
        let Ok(children) = children_query.get(entity) else {
            continue;
        };

        // Update children based on item presence
        if let Some((item_id, count)) = item_data {
            // Show icon and frame
            for child in children.iter() {
                // Update icon
                if let Ok(mut image_node) = icon_query.get_mut(child) {
                    if let Some(handle) = icon_registry.get(item_id) {
                        image_node.image = handle.clone();
                    }
                }
                // Update frame
                if let Ok(mut image_node) = frame_query.get_mut(child) {
                    image_node.image = slot_frames.common.clone();
                }
                // Update count
                if let Ok(mut text) = count_query.get_mut(child) {
                    *text = if count > 1 {
                        Text::new(format!("{}", count))
                    } else {
                        Text::new("")
                    };
                }
                // Show elements
                if let Ok(mut vis) = visibility_query.get_mut(child) {
                    *vis = Visibility::Visible;
                }
            }
        } else {
            // Hide icon and frame
            for child in children.iter() {
                if let Ok(mut vis) = visibility_query.get_mut(child) {
                    *vis = Visibility::Hidden;
                }
                if let Ok(mut text) = count_query.get_mut(child) {
                    *text = Text::new("");
                }
            }
        }
    }
}
