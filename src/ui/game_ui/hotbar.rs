use bevy::picking::prelude::*;
use bevy::prelude::*;
use bevy::ui::widget::ImageNode;

use super::components::*;
use super::components::{on_slot_hover, on_slot_unhover};
use super::drag_drop::handle_drop;
use super::spawn_slot_icon_children;
use super::theme::UiTheme;
use crate::inventory::Hotbar;
use crate::player::Player;

/// Spawn the hotbar UI at the bottom of the screen.
pub fn spawn_hotbar(commands: &mut Commands, theme: &UiTheme, asset_server: &AssetServer) {
    let config = &theme.hotbar;
    let colors = &theme.colors;

    // Hotbar container
    let pair_width = config.slot_size * 2.0;
    let total_width =
        config.slots as f32 * pair_width + (config.slots - 1) as f32 * config.gap;

    commands
        .spawn((
            HotbarRoot,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(config.margin_bottom),
                left: Val::Percent(50.0),
                width: Val::Px(total_width),
                height: Val::Px(config.slot_size),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(config.gap),
                margin: UiRect::new(
                    Val::Px(-total_width / 2.0), // Center via negative margin
                    Val::Auto,
                    Val::Auto,
                    Val::Auto,
                ),
                ..default()
            },
            BackgroundColor(Color::from(colors.bg_dark.clone())),
            Pickable::IGNORE,
        ))
        .with_children(|parent| {
            let slot_image = config.slot_texture.as_ref().map(|sc| {
                let slicer = TextureSlicer {
                    border: BorderRect::all(sc.border),
                    center_scale_mode: SliceScaleMode::Stretch,
                    sides_scale_mode: SliceScaleMode::Stretch,
                    max_corner_scale: 1.0,
                };
                (asset_server.load::<Image>(&sc.texture), slicer)
            });

            for i in 0..config.slots {
                let slot_size = config.slot_size;
                let border_width = config.border_width;
                let bg_medium = colors.bg_medium.clone();
                let border_color = colors.border.clone();
                let text_dim = colors.text_dim.clone();

                // Slot container (no UiSlot — only hand children have it)
                // Width = 2× slot_size so each hand half is a square.
                let mut slot_cmd = parent
                    .spawn((
                        Node {
                            width: Val::Px(slot_size * 2.0),
                            height: Val::Px(slot_size),
                            border: if slot_image.is_some() {
                                UiRect::ZERO
                            } else {
                                UiRect::all(Val::Px(border_width))
                            },
                            flex_direction: FlexDirection::Row,
                            ..default()
                        },
                        Pickable {
                            should_block_lower: false,
                            is_hoverable: true,
                        },
                    ));

                if let Some((ref handle, ref slicer)) = slot_image {
                    slot_cmd.insert(ImageNode {
                        image: handle.clone(),
                        image_mode: NodeImageMode::Sliced(slicer.clone()),
                        ..default()
                    });
                } else {
                    slot_cmd.insert((
                        BackgroundColor(Color::from(bg_medium)),
                        BorderColor::all(Color::from(border_color)),
                    ));
                }

                slot_cmd
                    .observe(handle_drop)
                    .with_children(|slot_parent| {
                        // Left hand half
                        slot_parent
                            .spawn((
                                UiSlot {
                                    slot_type: SlotType::Hotbar {
                                        index: i,
                                        hand: Hand::Left,
                                    },
                                },
                                Node {
                                    width: Val::Percent(50.0),
                                    height: Val::Percent(100.0),
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
                                Pickable {
                                    should_block_lower: false,
                                    is_hoverable: true,
                                },
                            ))
                            .observe(on_slot_hover)
                            .observe(on_slot_unhover)
                            .observe(handle_drop)
                            .with_children(spawn_slot_icon_children);
                        // Right hand half
                        slot_parent
                            .spawn((
                                UiSlot {
                                    slot_type: SlotType::Hotbar {
                                        index: i,
                                        hand: Hand::Right,
                                    },
                                },
                                Node {
                                    width: Val::Percent(50.0),
                                    height: Val::Percent(100.0),
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
                                Pickable {
                                    should_block_lower: false,
                                    is_hoverable: true,
                                },
                            ))
                            .observe(on_slot_hover)
                            .observe(on_slot_unhover)
                            .observe(handle_drop)
                            .with_children(spawn_slot_icon_children);
                        // Slot number label
                        slot_parent.spawn((
                            Text::new(format!("{}", i + 1)),
                            TextFont {
                                font_size: config.label_font_size,
                                ..default()
                            },
                            TextColor(Color::from(text_dim)),
                            Node {
                                position_type: PositionType::Absolute,
                                top: Val::Px(2.0),
                                left: Val::Px(2.0),
                                ..default()
                            },
                            Pickable::IGNORE,
                        ));
                    });
            }
        });
}

/// Sync hotbar UI slots with Hotbar component data.
pub fn update_hotbar_slots(
    hotbar_query: Query<&Hotbar, With<Player>>,
    mut slot_query: Query<(&UiSlot, &mut BackgroundColor, Option<&Children>)>,
    _child_slots: Query<&UiSlot>,
) {
    let Ok(hotbar) = hotbar_query.single() else {
        return;
    };

    for (slot, mut bg_color, children) in &mut slot_query {
        let SlotType::Hotbar { index, hand } = slot.slot_type else {
            continue;
        };

        let Some(_children) = children else {
            continue;
        };

        // Get item_id for THIS slot (not just active slot)
        let item_opt = if hand == Hand::Left {
            hotbar.slots[index].left_hand.as_deref()
        } else {
            hotbar.slots[index].right_hand.as_deref()
        };

        // Clear slot background — icons and greyed-out tint handle visuals now.
        *bg_color = BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0));
    }
}
