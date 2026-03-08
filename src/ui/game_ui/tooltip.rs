use bevy::prelude::*;
use bevy::ui::widget::ImageNode;
use bevy::window::PrimaryWindow;

use super::components::*;
use super::icon_registry::ItemIconRegistry;
use super::theme::UiTheme;
use crate::inventory::{Hotbar, Inventory};
use crate::item::definition::{ItemType, Rarity};
use crate::item::ItemRegistry;
use crate::player::Player;

// --- Marker components for tooltip children ---

#[derive(Component)]
pub(super) struct TooltipIcon;

#[derive(Component)]
pub(super) struct TooltipName;

#[derive(Component)]
pub(super) struct TooltipType;

#[derive(Component)]
pub(super) struct TooltipDesc;

#[derive(Component)]
pub(super) struct TooltipStats;

#[derive(Component)]
pub(super) struct TooltipHint;

/// Spawn tooltip entity (singleton) with child layout.
pub fn spawn_tooltip(
    mut commands: Commands,
    theme: Res<UiTheme>,
    existing: Query<Entity, With<UiTooltip>>,
) {
    if !existing.is_empty() {
        return;
    }
    let colors = &theme.colors;
    let padding = theme.tooltip.padding;
    let border_width = theme.tooltip.border_width;
    let max_width = theme.tooltip.max_width;

    commands
        .spawn((
            UiTooltip {
                item_id: String::new(),
                count: 0,
            },
            Node {
                position_type: PositionType::Absolute,
                padding: UiRect::all(Val::Px(padding)),
                border: UiRect::all(Val::Px(border_width)),
                max_width: Val::Px(max_width),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(padding),
                ..default()
            },
            BackgroundColor(Color::from(colors.bg_dark.clone())),
            BorderColor::all(Color::from(colors.border.clone())),
            Visibility::Hidden,
            Pickable::IGNORE,
            ZIndex(1000),
        ))
        .with_children(|parent| {
            // Left column: text info
            parent
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(4.0),
                        flex_shrink: 1.0,
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|col| {
                    col.spawn((
                        TooltipName,
                        Text::new(""),
                        TextFont { font_size: 14.0, ..default() },
                        TextColor(Color::WHITE),
                        Pickable::IGNORE,
                    ));
                    col.spawn((
                        TooltipType,
                        Text::new(""),
                        TextFont { font_size: 10.0, ..default() },
                        TextColor(Color::srgba(0.6, 0.6, 0.6, 1.0)),
                        Pickable::IGNORE,
                    ));
                    col.spawn((
                        TooltipDesc,
                        Text::new(""),
                        TextFont { font_size: 10.0, ..default() },
                        TextColor(Color::srgba(0.8, 0.75, 0.65, 1.0)),
                        Pickable::IGNORE,
                    ));
                    col.spawn((
                        TooltipStats,
                        Text::new(""),
                        TextFont { font_size: 10.0, ..default() },
                        TextColor(Color::srgba(0.5, 0.9, 0.5, 1.0)),
                        Pickable::IGNORE,
                    ));
                    col.spawn((
                        TooltipHint,
                        Text::new(""),
                        TextFont { font_size: 9.0, ..default() },
                        TextColor(Color::srgba(0.6, 0.6, 0.4, 1.0)),
                        Pickable::IGNORE,
                    ));
                });

            // Right: large icon
            parent.spawn((
                TooltipIcon,
                ImageNode::default(),
                Node {
                    width: Val::Px(48.0),
                    height: Val::Px(48.0),
                    flex_shrink: 0.0,
                    ..default()
                },
                Visibility::Hidden,
                Pickable::IGNORE,
            ));
        });
}

/// Resolve hovered item, update UiTooltip data, position, and visibility.
pub fn update_tooltip(
    mut tooltip_query: Query<(&mut Node, &mut Visibility, &mut UiTooltip)>,
    hovered: Res<HoveredSlot>,
    inventory_query: Query<&Inventory, With<Player>>,
    hotbar_query: Query<&Hotbar, With<Player>>,
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

    // Resolve item_id from slot
    let item_id_str: Option<&str> = match slot_type {
        SlotType::MainBag(idx) => inventory
            .main_bag
            .get(idx)
            .and_then(|s| s.as_ref())
            .map(|s| s.item_id.as_str()),
        SlotType::MaterialBag(idx) => inventory
            .material_bag
            .get(idx)
            .and_then(|s| s.as_ref())
            .map(|s| s.item_id.as_str()),
        SlotType::Hotbar { index, hand } => {
            let Ok(hotbar) = hotbar_query.single() else {
                *vis = Visibility::Hidden;
                return;
            };
            let slot_data = &hotbar.slots[index];
            match hand {
                Hand::Left => slot_data.left_hand.as_deref(),
                Hand::Right => slot_data.right_hand.as_deref(),
            }
        }
        SlotType::Equipment(_) => None,
    };

    let Some(item_id_str) = item_id_str else {
        *vis = Visibility::Hidden;
        return;
    };

    let count = inventory.count_item(item_id_str);

    tooltip.item_id = item_id_str.to_string();
    tooltip.count = count.min(u16::MAX as u32) as u16;
    *vis = Visibility::Visible;

    // Position near cursor
    let Ok(window) = window.single() else {
        return;
    };
    if let Some(cursor_pos) = window.cursor_position() {
        let offset = theme.tooltip.padding;
        let max_w = theme.tooltip.max_width;
        let win_w = window.width();
        let win_h = window.height();
        let tip_x = if cursor_pos.x + offset + max_w > win_w {
            (cursor_pos.x - offset - max_w).max(0.0)
        } else {
            cursor_pos.x + offset
        };
        let tip_y = if cursor_pos.y + offset + 100.0 > win_h {
            (cursor_pos.y - offset - 100.0).max(0.0)
        } else {
            cursor_pos.y + offset
        };
        node.left = Val::Px(tip_x);
        node.top = Val::Px(tip_y);
    }
}

/// Render tooltip text and icon children from UiTooltip data.
pub(super) fn render_tooltip_content(
    tooltip_query: Query<(Entity, &UiTooltip, &Visibility)>,
    item_registry: Res<ItemRegistry>,
    icon_registry: Res<ItemIconRegistry>,
    theme: Res<UiTheme>,
    children_query: Query<&Children>,
    mut name_q: Query<(&mut Text, &mut TextColor), With<TooltipName>>,
    mut type_q: Query<&mut Text, (With<TooltipType>, Without<TooltipName>)>,
    mut desc_q: Query<&mut Text, (With<TooltipDesc>, Without<TooltipName>, Without<TooltipType>)>,
    mut stats_q: Query<
        &mut Text,
        (With<TooltipStats>, Without<TooltipName>, Without<TooltipType>, Without<TooltipDesc>),
    >,
    mut hint_q: Query<
        &mut Text,
        (
            With<TooltipHint>,
            Without<TooltipName>,
            Without<TooltipType>,
            Without<TooltipDesc>,
            Without<TooltipStats>,
        ),
    >,
    mut icon_q: Query<(&mut ImageNode, &mut Visibility), (With<TooltipIcon>, Without<UiTooltip>)>,
) {
    let Ok((tooltip_entity, tooltip, vis)) = tooltip_query.single() else {
        return;
    };

    if *vis == Visibility::Hidden || tooltip.item_id.is_empty() {
        return;
    }

    let Some(item_id) = item_registry.by_name(&tooltip.item_id) else {
        return;
    };
    let def = item_registry.get(item_id);

    for descendant in children_query.iter_descendants(tooltip_entity) {
        if let Ok((mut text, mut color)) = name_q.get_mut(descendant) {
            let suffix = if tooltip.count > 1 {
                format!(" (x{})", tooltip.count)
            } else {
                String::new()
            };
            *text = Text::new(format!("{}{}", def.display_name, suffix));
            *color = TextColor(rarity_color(&def.rarity, &theme));
        }
        if let Ok(mut text) = type_q.get_mut(descendant) {
            *text = Text::new(format!(
                "{} · {}",
                rarity_label(&def.rarity),
                item_type_label(def.item_type)
            ));
        }
        if let Ok(mut text) = desc_q.get_mut(descendant) {
            *text = Text::new(def.description.clone());
        }
        if let Ok(mut text) = stats_q.get_mut(descendant) {
            *text = Text::new(build_stats_text(def));
        }
        if let Ok(mut text) = hint_q.get_mut(descendant) {
            let hint = match def.item_type {
                ItemType::Blueprint => "Right-click to learn",
                ItemType::Consumable => "Right-click to use",
                ItemType::Block => "Place with right-click",
                _ => "",
            };
            *text = Text::new(hint);
        }
        if let Ok((mut img, mut icon_vis)) = icon_q.get_mut(descendant) {
            if let Some(handle) = icon_registry.get(item_id) {
                img.image = handle.clone();
                *icon_vis = Visibility::Inherited;
            }
        }
    }
}

fn rarity_color(rarity: &Rarity, theme: &UiTheme) -> Color {
    match rarity {
        Rarity::Common => Color::from(theme.colors.rarity_common.clone()),
        Rarity::Uncommon => Color::from(theme.colors.rarity_uncommon.clone()),
        Rarity::Rare => Color::from(theme.colors.rarity_rare.clone()),
        Rarity::Legendary => Color::from(theme.colors.rarity_legendary.clone()),
    }
}

fn rarity_label(rarity: &Rarity) -> &'static str {
    match rarity {
        Rarity::Common => "Common",
        Rarity::Uncommon => "Uncommon",
        Rarity::Rare => "Rare",
        Rarity::Legendary => "Legendary",
    }
}

fn item_type_label(item_type: ItemType) -> &'static str {
    match item_type {
        ItemType::Block => "Block",
        ItemType::Resource => "Resource",
        ItemType::Tool => "Tool",
        ItemType::Weapon => "Weapon",
        ItemType::Armor => "Armor",
        ItemType::Consumable => "Consumable",
        ItemType::Material => "Material",
        ItemType::Blueprint => "Blueprint",
    }
}

fn build_stats_text(def: &crate::item::definition::ItemDef) -> String {
    let Some(ref stats) = def.stats else {
        return String::new();
    };
    let mut parts = Vec::new();
    if let Some(dmg) = stats.damage {
        parts.push(format!("Damage: {:.0}", dmg));
    }
    if let Some(def_val) = stats.defense {
        parts.push(format!("Defense: {:.0}", def_val));
    }
    if let Some(spd) = stats.speed_bonus {
        parts.push(format!("Speed: {:+.0}%", spd * 100.0));
    }
    if let Some(hp) = stats.health_bonus {
        parts.push(format!("Health: {:+}", hp));
    }
    parts.join(" · ")
}
