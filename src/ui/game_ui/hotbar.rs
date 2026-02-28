use bevy::prelude::*;

use super::components::*;
use super::theme::UiTheme;

/// Spawn the hotbar UI at the bottom of the screen.
pub fn spawn_hotbar(commands: &mut Commands, theme: &UiTheme) {
    let config = &theme.hotbar;
    let colors = &theme.colors;

    // Hotbar container
    let total_width =
        config.slots as f32 * config.slot_size + (config.slots - 1) as f32 * config.gap;

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
            for i in 0..config.slots {
                let slot_size = config.slot_size;
                let border_width = config.border_width;
                let bg_medium = colors.bg_medium.clone();
                let border_color = colors.border.clone();
                let text_dim = colors.text_dim.clone();

                // Slot container (holds L and R hands)
                parent
                    .spawn((
                        UiSlot {
                            slot_type: SlotType::Hotbar {
                                index: i,
                                hand: Hand::Left,
                            },
                        },
                        Node {
                            width: Val::Px(slot_size),
                            height: Val::Px(slot_size),
                            border: UiRect::all(Val::Px(border_width)),
                            flex_direction: FlexDirection::Row,
                            ..default()
                        },
                        BackgroundColor(Color::from(bg_medium)),
                        BorderColor::all(Color::from(border_color)),
                        Pickable {
                            should_block_lower: false,
                            is_hoverable: true,
                        },
                    ))
                    .with_children(|slot_parent| {
                        // Left hand half
                        slot_parent.spawn((
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
                        ));
                        // Right hand half
                        slot_parent.spawn((
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
                        ));
                        // Slot number label
                        slot_parent.spawn((
                            Text::new(format!("{}", i + 1)),
                            TextFont {
                                font_size: 10.0,
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
