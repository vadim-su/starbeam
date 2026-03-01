//! Sync UI slot visuals with backing Inventory data.

use bevy::prelude::*;
use bevy::ui::widget::ImageNode;

use super::components::{Hand, ItemCount, ItemIcon, SlotFrame, SlotType, UiSlot};
use super::icon_registry::ItemIconRegistry;
use super::SlotFrames;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::item::ItemRegistry;
use crate::player::Player;

/// Sync inventory bag slot backgrounds (tinted when occupied).
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
            SlotType::Hotbar { .. } => continue,
            SlotType::Equipment(_) => continue,
        };

        if item_opt.is_some() {
            *bg_color = BackgroundColor(Color::srgb(0.2, 0.4, 0.2));
        } else {
            *bg_color = BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0));
        }
    }
}

/// Update slot icons, frames, and counts from inventory/hotbar data.
/// Only runs when Inventory or Hotbar components have changed.
pub fn update_slot_icons(
    inventory_query: Query<&Inventory, (With<Player>, Changed<Inventory>)>,
    hotbar_query: Query<&Hotbar, (With<Player>, Changed<Hotbar>)>,
    item_registry: Res<ItemRegistry>,
    icon_registry: Res<ItemIconRegistry>,
    slot_frames: Res<SlotFrames>,

    // Query for slots with children
    slot_query: Query<(Entity, &UiSlot), With<Children>>,
    // Child queries — use Without<T> to prove disjoint access to ImageNode
    mut icon_query: Query<&mut ImageNode, (With<ItemIcon>, Without<SlotFrame>)>,
    mut frame_query: Query<&mut ImageNode, (With<SlotFrame>, Without<ItemIcon>)>,
    mut count_query: Query<&mut Text, With<ItemCount>>,
    mut visibility_query: Query<&mut Visibility, Or<(With<ItemIcon>, With<SlotFrame>)>>,
    children_query: Query<&Children>,
) {
    let inventory = inventory_query.single();
    let hotbar = hotbar_query.single();

    // Skip if neither changed
    if inventory.is_err() && hotbar.is_err() {
        return;
    }

    // We need the actual data even if only one changed, so fall back to non-filtered queries
    // won't work — use Option-based approach instead. For simplicity, query without Changed
    // filter below when we need the data.
    // Actually, since Changed filters mean the query returns nothing when unchanged,
    // we need separate non-filtered queries for reading. Let's restructure:
    // The Changed filter already gives us the component when it matches.
    // When one changes, we still need the other's data. So we use the changed result
    // if available, otherwise skip that half.

    for (entity, slot) in &slot_query {
        // Get item data for this slot
        let item_data: Option<(&str, u16)> = match slot.slot_type {
            SlotType::MainBag(idx) => {
                let Ok(inv) = &inventory else { continue };
                inv.main_bag
                    .get(idx)
                    .and_then(|s| s.as_ref())
                    .map(|s| (s.item_id.as_str(), s.count))
            }
            SlotType::MaterialBag(idx) => {
                let Ok(inv) = &inventory else { continue };
                inv.material_bag
                    .get(idx)
                    .and_then(|s| s.as_ref())
                    .map(|s| (s.item_id.as_str(), s.count))
            }
            SlotType::Hotbar { index, hand } => {
                let Ok(hb) = &hotbar else { continue };
                let slot_data = &hb.slots[index];
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
            let item_id_typed = item_registry.by_name(item_id);

            for child in children.iter() {
                // Update icon
                if let Ok(mut image_node) = icon_query.get_mut(child) {
                    if let Some(handle) = icon_registry.get(item_id_typed) {
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
