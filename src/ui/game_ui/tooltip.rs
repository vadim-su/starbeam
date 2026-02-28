use bevy::picking::prelude::*;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::components::*;
use super::theme::UiTheme;
use crate::inventory::Inventory;
use crate::player::Player;

/// Spawn tooltip entity (singleton).
pub fn spawn_tooltip(mut commands: Commands, theme: Res<UiTheme>) {
    let colors = &theme.colors;
    let padding = theme.tooltip.padding;

    commands.spawn((
        UiTooltip {
            item_id: String::new(),
            count: 0,
        },
        Node {
            position_type: PositionType::Absolute,
            padding: UiRect::all(Val::Px(padding)),
            ..default()
        },
        BackgroundColor(Color::from(colors.bg_dark.clone())),
        BorderColor::all(Color::from(colors.border.clone())),
        Visibility::Hidden,
        Pickable::IGNORE,
        ZIndex(1000),
    ));
}

/// Update tooltip position and content based on hovered slot.
pub fn update_tooltip(
    mut tooltip_query: Query<(&mut Node, &mut Visibility, &mut UiTooltip), With<UiTooltip>>,
    hovered: Res<HoveredSlot>,
    inventory_query: Query<&Inventory, With<Player>>,
    window: Query<&Window, With<PrimaryWindow>>,
    theme: Res<UiTheme>,
) {
    let Ok((mut node, mut vis, mut tooltip)) = tooltip_query.single_mut() else {
        return;
    };

    let Some(slot_type) = hovered.slot else {
        *vis = Visibility::Hidden;
        return;
    };

    let Ok(inventory) = inventory_query.single() else {
        *vis = Visibility::Hidden;
        return;
    };

    // Get item from slot
    let item_opt = match slot_type {
        SlotType::MainBag(idx) => inventory.main_bag.get(idx).and_then(|s| s.as_ref()),
        SlotType::MaterialBag(idx) => inventory.material_bag.get(idx).and_then(|s| s.as_ref()),
        _ => None,
    };

    let Some(item) = item_opt else {
        *vis = Visibility::Hidden;
        return;
    };

    // Show tooltip with item info
    *vis = Visibility::Visible;
    tooltip.item_id = item.item_id.clone();
    tooltip.count = item.count;

    // Position near cursor
    let Ok(window) = window.single() else {
        return;
    };
    if let Some(cursor_pos) = window.cursor_position() {
        let offset = theme.tooltip.padding;
        node.left = Val::Px(cursor_pos.x + offset);
        node.top = Val::Px(cursor_pos.y + offset);
    }
}
