//! Drag and drop functionality for inventory slots.
//!
//! This module handles:
//! - Spawning visual drag icons that follow the cursor
//! - Updating drag icon position during drag operations
//! - Canceling drags and returning items to source slots

use bevy::picking::events::{DragDrop, DragEnd, DragStart};
use bevy::picking::prelude::*;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::components::{DragInfo, DragState, SlotType, UiSlot};
use super::theme::UiTheme;
use crate::inventory::Inventory;
use crate::player::Player;

/// Marker component for the visual drag icon entity.
#[derive(Component)]
pub struct DragIcon;

/// Create visual drag icon following cursor.
pub fn spawn_drag_icon(
    commands: &mut Commands,
    item_id: &str,
    count: u16,
    theme: &UiTheme,
) -> Entity {
    let _ = (item_id, count, theme); // Suppress unused warnings until icons implemented

    commands
        .spawn((
            DragIcon,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(32.0),
                height: Val::Px(32.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.5, 0.7, 0.5)), // Placeholder color
            Pickable::IGNORE,
            GlobalZIndex(1000),
        ))
        .id()
}

/// Update drag icon position to follow cursor.
pub fn update_drag_position(
    drag_state: Res<DragState>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut query: Query<&mut Node, With<DragIcon>>,
) {
    let Some(drag) = drag_state.dragging.as_ref() else {
        return;
    };

    let Ok(window) = window.single() else {
        return;
    };

    let Some(cursor) = window.cursor_position() else {
        return;
    };

    if let Ok(mut node) = query.get_mut(drag.drag_icon) {
        node.left = Val::Px(cursor.x - 16.0);
        node.top = Val::Px(cursor.y - 16.0);
    }
}

/// Cancel drag, return item to source.
pub fn cancel_drag(mut drag_state: ResMut<DragState>, mut commands: Commands) {
    if let Some(drag) = drag_state.dragging.take() {
        commands.entity(drag.drag_icon).despawn();
    }
}

/// Handle drag start on inventory bag slots (MainBag and MaterialBag).
pub fn on_bag_slot_drag_start(
    trigger: On<Pointer<DragStart>>,
    mut drag_state: ResMut<DragState>,
    slot_query: Query<&UiSlot>,
    inventory_query: Query<&Inventory, With<Player>>,
    mut commands: Commands,
    theme: Res<UiTheme>,
) {
    let Ok(slot) = slot_query.get(trigger.event_target()) else {
        return;
    };
    let Ok(inv) = inventory_query.single() else {
        return;
    };

    // Get item from slot based on slot type
    let item_opt = match slot.slot_type {
        SlotType::MainBag(idx) => inv.main_bag.get(idx).and_then(|s| s.as_ref()),
        SlotType::MaterialBag(idx) => inv.material_bag.get(idx).and_then(|s| s.as_ref()),
        _ => return, // Only handle bag slots here
    };

    let Some(item) = item_opt else {
        return; // Empty slot, don't start drag
    };

    let drag_icon = spawn_drag_icon(&mut commands, &item.item_id, item.count, &theme);

    drag_state.dragging = Some(DragInfo {
        item_id: item.item_id.clone(),
        count: item.count,
        source_slot: slot.slot_type,
        drag_icon,
    });
}

/// Handle drag end - despawn drag icon and clear state.
pub fn on_drag_end(
    _trigger: On<Pointer<DragEnd>>,
    mut drag_state: ResMut<DragState>,
    mut commands: Commands,
) {
    if let Some(drag) = drag_state.dragging.take() {
        commands.entity(drag.drag_icon).despawn();
    }
}

/// Handle drop on target slot.
 Canc the inventory data.
pub fn handle_drop(
    drag_state: ResMut<DragState>,
    target_slot: SlotType,
    mut inventory_query: Query<&mut Inventory, With<Player>>,
    mut commands: Commands,
) {
    let Some(drag) = drag_state.dragging.take() else {
        return;
    }

    // Same slot = cancel
    if drag.source_slot == target_slot {
        return;
    }

    // Remove from source
    let Some(mut item) = removed else {
        return; // Item was nowhere, shouldn continue
    }

    
    // Add to target
    match target_slot {
        SlotType::MainBag(idx) => {
            if let Some(target_slot_ref) = inventory.main_bag.get_mut(idx) {
                if target_slot_ref.is_none() {
                    *target_slot_ref = Some(item);
                } else if let Some(ref target) = target_slot_ref {
                    // Swap
                    std::mem::swap(&mut item, target);
                    // Return swapped item to source (simplified)
                    return;
                }
            }
        }
        SlotType::MaterialBag(idx) => {
            if let Some(target_slot_ref) = inventory.material_bag.get_mut(idx) {
                if target_slot_ref.is_none() {
                    *target_slot_ref = Some(item);
                }
            }
        }
    }
}

    let Ok(mut inventory) = inventory_query.single_mut() else {
        return;
    };

    // Remove from source
    let source_item = match drag.source_slot {
        SlotType::MainBag(idx) => inventory.main_bag.get(idx).cloned(),
        SlotType::MaterialBag(idx) => inventory.material_bag.get(idx).cloned(),
        _ => None,
    };

    let Some(mut source_item) = source_item else {
        return; // No item in source
    };

    // Add to target
    match target_slot {
        SlotType::MainBag(idx) => {
            if let Some(target) = inventory.main_bag.get_mut(idx) {
                if target.is_none() {
                    *target = Some(source_item);
                } else {
                    // Swap
                    std::mem::swap(&mut source_item, target);
                }
            }
        }
        SlotType::MaterialBag(idx) => {
            if let Some(target) = inventory.material_bag.get_mut(idx) {
                if target.is_none() {
                    *target = Some(source_item);
                }
            }
        }
        _ => {}
    }
}

    let Ok(mut inventory) = inventory_query.single_mut() else {
        return;
    };

    // Remove from source
    let removed = match drag.source_slot {
        SlotType::MainBag(idx) => {
            if let Some(slot) = inventory.main_bag.get_mut(idx) {
                slot.take()
            } else {
                None
            }
        }
        SlotType::MaterialBag(idx) => {
            if let Some(slot) = inventory.material_bag.get_mut(idx) {
                slot.take()
            } else {
                None
            }
        }
        _ => None,
    };

    let Some(mut item) = removed else {
        return;
    };

    // Add to target
    match target_slot {
        SlotType::MainBag(idx) => {
            if let Some(target_slot_ref) = inventory.main_bag.get_mut(idx) {
                if target_slot_ref.is_none() {
                    *target_slot_ref = Some(item);
                } else if let Some(ref target) = target_slot_ref {
                    // Swap
                    std::mem::swap(&mut item, target);
                    // Return swapped item to source (simplified)
                }
            }
        }
        SlotType::MaterialBag(idx) => {
            if let Some(target_slot_ref) = inventory.material_bag.get_mut(idx) {
                if target_slot_ref.is_none() {
                    *target_slot_ref = Some(item);
                }
            }
        }
        _ => {}
    }
}
