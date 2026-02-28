//! Inventory screen UI — equipment panel (left) + bag grids (right).
//!
//! Spawned hidden; toggled by E/I keys via `toggle_inventory` in mod.rs.

use bevy::picking::prelude::*;
use bevy::prelude::*;

use super::components::*;
use super::drag_drop::{handle_drop, on_bag_slot_drag_start, on_drag_end};
use super::theme::UiTheme;

/// Spawn the inventory screen (hidden by default).
pub fn spawn_inventory_screen(commands: &mut Commands, theme: &UiTheme) {
    let config = &theme.inventory_screen;
    let colors = &theme.colors;

    // Pre-compute colors (Color is Copy, so we can reuse freely)
    let bg_dark = Color::from(colors.bg_dark.clone());
    let bg_medium = Color::from(colors.bg_medium.clone());
    let border_color = Color::from(colors.border.clone());

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
            BackgroundColor(bg_dark),
            BorderColor::all(border_color),
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .with_children(|parent| {
            // ── Left column: Equipment ──
            let eq_slot_size = config.equipment.slot_size;
            let eq_gap = config.equipment.gap;

            parent
                .spawn((
                    Node {
                        width: Val::Px(eq_slot_size),
                        height: Val::Auto,
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(eq_gap),
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
                        eq_parent
                            .spawn((
                                UiSlot {
                                    slot_type: SlotType::Equipment(slot),
                                },
                                Node {
                                    width: Val::Px(eq_slot_size),
                                    height: Val::Px(eq_slot_size),
                                    border: UiRect::all(Val::Px(2.0)),
                                    ..default()
                                },
                                BackgroundColor(bg_medium),
                                BorderColor::all(border_color),
                                Pickable {
                                    should_block_lower: false,
                                    is_hoverable: true,
                                },
                            ))
                            .observe(
                                |trigger: On<Pointer<Over>>,
                                 mut hovered: ResMut<HoveredSlot>,
                                 slot_query: Query<&UiSlot>| {
                                    if let Ok(slot) = slot_query.get(trigger.event_target()) {
                                        hovered.slot = Some(slot.slot_type);
                                    }
                                },
                            )
                            .observe(
                                |_trigger: On<Pointer<Out>>, mut hovered: ResMut<HoveredSlot>| {
                                    hovered.slot = None;
                                },
                            );
                    }
                });

            // ── Right column: Bags ──
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
                    // ── Main bag grid ──
                    let main_cols = config.main_bag.columns;
                    let main_rows = config.main_bag.rows;
                    let main_slot = config.main_bag.slot_size;
                    let main_gap = config.main_bag.gap;
                    let main_w = main_cols as f32 * main_slot + (main_cols - 1) as f32 * main_gap;
                    let main_h = main_rows as f32 * main_slot + (main_rows - 1) as f32 * main_gap;

                    bag_parent
                        .spawn((
                            Node {
                                width: Val::Px(main_w),
                                height: Val::Px(main_h),
                                display: Display::Grid,
                                grid_template_columns: vec![GridTrack::px(main_slot); main_cols],
                                grid_template_rows: vec![GridTrack::px(main_slot); main_rows],
                                column_gap: Val::Px(main_gap),
                                row_gap: Val::Px(main_gap),
                                ..default()
                            },
                            Pickable::IGNORE,
                        ))
                        .with_children(|grid| {
                            for i in 0..(main_cols * main_rows) {
                                grid.spawn((
                                    UiSlot {
                                        slot_type: SlotType::MainBag(i),
                                    },
                                    Node {
                                        width: Val::Px(main_slot),
                                        height: Val::Px(main_slot),
                                        border: UiRect::all(Val::Px(1.0)),
                                        ..default()
                                    },
                                    BackgroundColor(bg_medium),
                                    BorderColor::all(border_color),
                                    Pickable {
                                        should_block_lower: false,
                                        is_hoverable: true,
                                    },
                                ))
                                .observe(
                                    |trigger: On<Pointer<Over>>,
                                     mut hovered: ResMut<HoveredSlot>,
                                     slot_query: Query<&UiSlot>| {
                                        if let Ok(slot) =
                                            slot_query.get(trigger.event_target())
                                        {
                                            hovered.slot = Some(slot.slot_type);
                                        }
                                    },
                                )
                                .observe(
                                    |_trigger: On<Pointer<Out>>,
                                     mut hovered: ResMut<HoveredSlot>| {
                                        hovered.slot = None;
                                    },
                                )
                                .observe(on_bag_slot_drag_start)
                                .observe(on_drag_end)
                                .observe(handle_drop);
                            }
                        });

                    // ── Material bag grid ──
                    let mat_cols = config.material_bag.columns;
                    let mat_rows = config.material_bag.rows;
                    let mat_slot = config.material_bag.slot_size;
                    let mat_gap = config.material_bag.gap;
                    let mat_w = mat_cols as f32 * mat_slot + (mat_cols - 1) as f32 * mat_gap;
                    let mat_h = mat_rows as f32 * mat_slot + (mat_rows - 1) as f32 * mat_gap;

                    bag_parent
                        .spawn((
                            Node {
                                width: Val::Px(mat_w),
                                height: Val::Px(mat_h),
                                display: Display::Grid,
                                grid_template_columns: vec![GridTrack::px(mat_slot); mat_cols],
                                grid_template_rows: vec![GridTrack::px(mat_slot); mat_rows],
                                column_gap: Val::Px(mat_gap),
                                row_gap: Val::Px(mat_gap),
                                ..default()
                            },
                            Pickable::IGNORE,
                        ))
                        .with_children(|grid| {
                            for i in 0..(mat_cols * mat_rows) {
                                grid.spawn((
                                    UiSlot {
                                        slot_type: SlotType::MaterialBag(i),
                                    },
                                    Node {
                                        width: Val::Px(mat_slot),
                                        height: Val::Px(mat_slot),
                                        border: UiRect::all(Val::Px(1.0)),
                                        ..default()
                                    },
                                    BackgroundColor(bg_medium),
                                    BorderColor::all(border_color),
                                    Pickable {
                                        should_block_lower: false,
                                        is_hoverable: true,
                                    },
                                ))
                                .observe(
                                    |trigger: On<Pointer<Over>>,
                                     mut hovered: ResMut<HoveredSlot>,
                                     slot_query: Query<&UiSlot>| {
                                        if let Ok(slot) =
                                            slot_query.get(trigger.event_target())
                                        {
                                            hovered.slot = Some(slot.slot_type);
                                        }
                                    },
                                )
                                .observe(
                                    |_trigger: On<Pointer<Out>>,
                                     mut hovered: ResMut<HoveredSlot>| {
                                        hovered.slot = None;
                                    },
                                )
                                .observe(on_bag_slot_drag_start)
                                .observe(on_drag_end)
                                .observe(handle_drop);
                            }
                        });
                });
        });
}
