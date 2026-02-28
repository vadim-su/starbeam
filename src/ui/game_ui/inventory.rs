use bevy::picking::prelude::*;
use bevy::prelude::*;

use super::components::*;
use super::drag_drop::{handle_drop, on_bag_slot_drag_start, on_drag_end};
 on_bag_slot_drag_start};
use super::theme::UiTheme;
use crate::inventory::Inventory;
use crate::player::Player;

/// Spawn the inventory screen (hidden by default).
pub fn spawn_inventory_screen(
    commands: &mut Commands,
    theme: Res<UiTheme>,
) {
    let config = &theme.inventory_screen;
    let colors = &theme.colors;

    commands
        .spawn((
            InventoryScreen,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(config.width),
                height: Val::Px(config.height),
                left: Val::Percent(50.0),
                top: Val::Percent(50.0),
                margin: UiRect::new(
                    Val::Px(-config.width / 2.0),
                    Val::Auto,
                    Val::Auto,
                    Val::Px(-config.height / 2.0),
                ),
                flex_direction: FlexDirection::Row,
                padding: UiRect::all(Val::Px(config.padding)),
                column_gap: Val::Px(16.0),
                ..default()
            },
            BackgroundColor(Color::from(colors.bg_dark.clone())),
            BorderColor::all(Color::from(colors.border.clone())),
            Visibility::Hidden, // Start hidden
            Pickable::IGNORE,
        ))
        .with_children(|parent| {
            // Left column: Equipment
            let eq_config = &theme.inventory_screen.equipment;
            let eq_colors = colors;

            parent
                .spawn((
                    Node {
                        width: Val::Px(eq_config.slot_size),
                        height: Val::Auto,
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(eq_config.gap),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|eq_parent| {
                    let slots = [
                        EquipSlot::Head,
                        EquipSlot::Chest,
                        EquipSlot::Legs,
                        EquipSlot::Back,
                        EquipSlot::HeadCosmetic,
                        EquipSlot::ChestCosmetic,
                        EquipSlot::LegsCosmetic,
                        EquipSlot::BackCosmetic,
                    ];

                    for slot in slots {
                        let slot_size = eq_config.slot_size;
                        let bg_medium = eq_colors.bg_medium.clone();
                        let border_color = eq_colors.border.clone();

                        eq_parent.spawn((
                            UiSlot {
                                slot_type: SlotType::Equipment(slot),
                            },
                            Node {
                                width: Val::Px(slot_size),
                                height: Val::Px(slot_size),
                                border: UiRect::all(Val::Px(2.0)),
                                ..default()
                            },
                            BackgroundColor(Color::from(bg_medium)),
                            BorderColor::all(Color::from(border_color)),
                            Pickable {
                                should_block_lower: false,
                                is_hoverable: true,
                            },
                        ));
                        .observe(|trigger: On<Pointer<Over>>, mut hovered: ResMut<HoveredSlot>, slot_query: Query<&UiSlot>| {
                            if let Ok(slot) = slot_query.get(trigger.target()) {
                                hovered.slot = Some(slot.slot_type);
                            }
                        }
                        })
                        .observe(|_trigger: On<Pointer<Out>>, mut hovered: ResMut<HoveredSlot>| {
                            hovered.slot = None;
                        }
                    );
                });

            // Right column: Bags
            let main_config = &theme.inventory_screen.main_bag;
            let mat_config = &theme.inventory_screen.material_bag;
            let bag_colors = colors;

            parent
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(8.0),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|bag_parent| {
                    // Main bag (40 slots)
                    {
                        let columns = main_config.columns;
                        let rows = main_config.rows;
                        let slot_size = main_config.slot_size;
                        let gap = main_config.gap;
                        let total_width = columns as f32 * slot_size + (columns - 1) as f32 * gap;
                        let total_height = rows as f32 * slot_size + (rows - 1) as f32 * gap;
                        let bg_medium = bag_colors.bg_medium.clone();
                        let border_color = bag_colors.border.clone();

                        bag_parent
                            .spawn((
                                Node {
                                    width: Val::Px(total_width),
                                    height: Val::Px(total_height),
                                    display: Display::Grid,
                                    grid_template_columns: vec![GridTrack::px(slot_size); columns],
                                    grid_template_rows: vec![GridTrack::px(slot_size); rows],
                                    column_gap: Val::Px(gap),
                                    row_gap: Val::Px(gap),
                                    ..default()
                                },
                                Pickable::IGNORE,
                            ))
                            .with_children(|grid_parent| {
                                for i in 0..(columns * rows) {
                                    let slot_size = slot_size;
                                    let bg_medium_inner = bg_medium.clone();
                                    let border_color = border_color.clone();

                                    grid_parent.spawn((
                                        UiSlot {
                                            slot_type: SlotType::MainBag(i),
                                        },
                                        Node {
                                            width: Val::Px(slot_size),
                                            height: Val::Px(slot_size),
                                            border: UiRect::all(Val::Px(1.0)),
                                            ..default()
                                        },
                                        BackgroundColor(Color::from(bg_medium)),
                                        BorderColor::all(Color::from(border_color)),
                                        Pickable {
                                            should_block_lower: false,
                                            is_hoverable: true,
                                        },
                                    ))
                                    .observe(|trigger: On<Pointer<Over>>, mut hovered: ResMut<HoveredSlot>, slot_query: Query<&UiSlot>| {
                                        if let Ok(slot) = slot_query.get(trigger.target()) {
                                            hovered.slot = Some(slot.slot_type);
                                        }
                                    })
                    )
                    .observe(|_trigger: On<Pointer<Out>>, mut hovered: ResMut<HoveredSlot>| {
                        hovered.slot = None;
                    }
                );

                // Material bag (16 slots)
                {
                    let columns = mat_config.columns;
                    let rows = mat_config.rows;
                    let slot_size = mat_config.slot_size;
                    let gap = mat_config.gap;
                    let total_width = columns as f32 * slot_size + (columns - 1) as f32 * gap;
                    let total_height = rows as f32 * slot_size + (rows - 1) as f32 * gap;
                    let bg_medium = bag_colors.bg_medium.clone();
                    let border_color = bag_colors.border.clone();

                    bag_parent
                        .spawn((
                            Node {
                                width: Val::Px(total_width),
                                height: Val::Px(total_height),
                                display: Display::Grid,
                                grid_template_columns: vec![GridTrack::px(slot_size); columns],
                                grid_template_rows: vec![GridTrack::px(slot_size); rows],
                                column_gap: Val::Px(gap),
                                row_gap: Val::Px(gap),
                                ..default()
                            },
                            Pickable::IGNORE,
                        ))
                        .with_children(|grid_parent| {
                            for i in 0..(columns * rows) {
                                let slot_size = slot_size;
                                let bg_medium_inner = bg_medium.clone();
                                let border_color = border_color.clone();

                                grid_parent.spawn((
                                    UiSlot {
                                        slot_type: SlotType::MaterialBag(i),
                                    },
                                    Node {
                                        width: Val::Px(slot_size),
                                        height: Val::Px(slot_size),
                                        border: UiRect::all(Val::Px(1.0)),
                                        ..default()
                                    },
                                    BackgroundColor(Color::from(bg_medium)),
                                    BorderColor::all(Color::from(border_color)),
                                    Pickable {
                                        should_block_lower: false,
                                        is_hoverable: true,
                                    },
                                ))
                                .observe(|trigger: On<Pointer<Over>>, mut hovered: ResMut<HoveredSlot>, slot_query: Query<&UiSlot>| {
                                    if let Ok(slot) = slot_query.get(trigger.target()) {
                                        hovered.slot = Some(slot.slot_type);
                                    }
                })
            );
        }
    }
}
