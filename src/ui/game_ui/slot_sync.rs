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
    inventory_query: Query<Ref<Inventory>, With<Player>>,
    hotbar_query: Query<Ref<Hotbar>, With<Player>>,
    item_registry: Res<ItemRegistry>,
    icon_registry: Res<ItemIconRegistry>,
    slot_frames: Res<SlotFrames>,

    // Query for slots with children
    slot_query: Query<(Entity, &UiSlot), With<Children>>,
    // Single query for ImageNode children — Has<T> used to distinguish icon vs frame
    mut image_query: Query<(&mut ImageNode, Has<ItemIcon>, Has<SlotFrame>)>,
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

    // Skip if neither changed
    if !inventory.is_changed() && !hotbar.is_changed() {
        return;
    }

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
                let item_id_opt = match hand {
                    Hand::Left => slot_data.left_hand.as_deref(),
                    Hand::Right => slot_data.right_hand.as_deref(),
                };
                // Resolve count from inventory for hotbar references.
                // Return count=0 so the icon stays visible (greyed out).
                item_id_opt.map(|id| {
                    let count = inventory.count_item(id);
                    (id, count.min(u16::MAX as u32) as u16)
                })
            }
            SlotType::Equipment(_) => continue,
        };

        // Get children of this slot
        let Ok(children) = children_query.get(entity) else {
            continue;
        };

        // Update children based on item presence
        if let Some((item_id, count)) = item_data {
            let Some(item_id_typed) = item_registry.by_name(item_id) else {
                continue;
            };

            let depleted = count == 0;

            for child in children.iter() {
                // Update icon or frame image
                if let Ok((mut image_node, is_icon, is_frame)) = image_query.get_mut(child) {
                    if is_icon {
                        if let Some(handle) = icon_registry.get(item_id_typed) {
                            image_node.image = handle.clone();
                        }
                        // Grey out depleted hotbar items
                        image_node.color = if depleted {
                            Color::srgba(0.3, 0.3, 0.3, 0.5)
                        } else {
                            Color::WHITE
                        };
                    } else if is_frame {
                        image_node.image = slot_frames.common.clone();
                    }
                }
                // Update count
                if let Ok(mut text) = count_query.get_mut(child) {
                    *text = if count > 1 {
                        Text::new(format!("{}", count))
                    } else {
                        Text::new("")
                    };
                }
                // Inherit visibility from parent (respects InventoryScreen hidden/visible)
                if let Ok(mut vis) = visibility_query.get_mut(child) {
                    *vis = Visibility::Inherited;
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
