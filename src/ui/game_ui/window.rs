//! Unified window system — draggable, closable panels with header and ESC support.
//!
//! Use [`spawn_window_frame`] to create a new window with a standard header
//! (title + close button) and an empty body container. The caller receives the
//! root and body entity IDs, so it can insert extra components and add children
//! to the body.

use bevy::picking::events::{Drag, Press};
use bevy::picking::prelude::*;
use bevy::prelude::*;
use bevy::ui::widget::ImageNode;

use super::components::{DragState, InventoryScreenState};
use super::theme::UiTheme;
use crate::interaction::interactable::{HandCraftOpen, OpenStation};
use crate::trader::OpenTrader;

const HEADER_HEIGHT: f32 = 28.0;

// ── Components / Resources ──

/// Tracks which window currently has focus (last clicked).
///
/// ESC will close the focused window first; if no focused window is visible,
/// falls back to the highest-priority visible window.
#[derive(Resource, Default)]
pub struct FocusedWindow(pub Option<Entity>);

/// Marker identifying a unified game window and its kind.
///
/// Attach to the root entity of any window to opt in to dragging and ESC-close.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum GameWindow {
    Inventory,
    Crafting,
    Trading,
}

/// Close button inside a window header.
#[derive(Component)]
pub struct WindowCloseButton {
    pub window: Entity,
}

/// Marker for the window body container.
#[derive(Component)]
pub struct WindowBody;

// ── Spawn helper ──

/// Configuration for [`spawn_window_frame`].
pub struct WindowConfig<'a> {
    pub title: &'a str,
    pub width: f32,
    pub height: f32,
    pub padding: f32,
}

/// Entity IDs returned by [`spawn_window_frame`].
pub struct WindowEntities {
    pub root: Entity,
    pub body: Entity,
}

/// Spawn a window frame: a root container with a header (title + close button)
/// and an empty body. The body has `flex_grow: 1.0` so it fills the remaining
/// space.
///
/// The root is absolutely positioned at the centre of the screen and supports
/// dragging via the [`on_window_drag`] observer.
pub fn spawn_window_frame(
    commands: &mut Commands,
    theme: &UiTheme,
    config: &WindowConfig,
    window_kind: GameWindow,
    asset_server: &AssetServer,
) -> WindowEntities {
    let colors = &theme.colors;
    let bg_dark = Color::from(colors.bg_dark.clone());
    let bg_medium = Color::from(colors.bg_medium.clone());
    let border_color = Color::from(colors.border.clone());
    let text_color = Color::from(colors.text.clone());

    let panel_image = theme.panel_texture.as_ref().map(|sc| {
        let slicer = TextureSlicer {
            border: BorderRect::all(sc.border),
            center_scale_mode: SliceScaleMode::Stretch,
            sides_scale_mode: SliceScaleMode::Stretch,
            max_corner_scale: 1.0,
        };
        (asset_server.load::<Image>(&sc.texture), slicer)
    });

    // ── Root ──
    let mut root_cmd = commands.spawn((
        window_kind,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(config.width),
            height: Val::Px(config.height),
            left: Val::Percent(50.0),
            top: Val::Percent(50.0),
            margin: UiRect::new(
                Val::Px(-config.width / 2.0),
                Val::Auto,
                Val::Px(-config.height / 2.0),
                Val::Auto,
            ),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(config.padding)),
            border: if panel_image.is_some() {
                UiRect::ZERO
            } else {
                UiRect::all(Val::Px(2.0))
            },
            ..default()
        },
        ZIndex(0),
        Pickable {
            should_block_lower: true,
            is_hoverable: true,
        },
    ));

    if let Some((ref handle, ref slicer)) = panel_image {
        root_cmd.insert(ImageNode {
            image: handle.clone(),
            image_mode: NodeImageMode::Sliced(slicer.clone()),
            ..default()
        });
    } else {
        root_cmd.insert((
            BackgroundColor(bg_dark),
            BorderColor::all(border_color),
        ));
    }

    let root_id = root_cmd
        .observe(on_window_drag)
        .observe(on_window_focus)
        .id();

    // ── Header ──
    commands.entity(root_id).with_children(|root| {
        root.spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(HEADER_HEIGHT),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                padding: UiRect::horizontal(Val::Px(4.0)),
                border: UiRect::bottom(Val::Px(1.0)),
                margin: UiRect::bottom(Val::Px(4.0)),
                ..default()
            },
            BorderColor::all(border_color),
            Pickable::IGNORE,
        ))
        .with_children(|header| {
            // Title
            header.spawn((
                Text::new(config.title),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(text_color),
                Pickable::IGNORE,
            ));

            // Close button
            header
                .spawn((
                    WindowCloseButton { window: root_id },
                    Button,
                    Node {
                        width: Val::Px(24.0),
                        height: Val::Px(24.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(bg_medium),
                    BorderColor::all(border_color),
                    Pickable {
                        should_block_lower: true,
                        is_hoverable: true,
                    },
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("X"),
                        TextFont {
                            font_size: 12.0,
                            ..default()
                        },
                        TextColor(text_color),
                        Pickable::IGNORE,
                    ));
                });
        });
    });

    // ── Body ──
    let body_id = commands
        .spawn((
            WindowBody,
            Node {
                flex_grow: 1.0,
                width: Val::Percent(100.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();

    commands.entity(root_id).add_child(body_id);

    WindowEntities {
        root: root_id,
        body: body_id,
    }
}

// ── Observers ──

/// Observer: drag a window by adjusting its margin offsets.
fn on_window_drag(
    trigger: On<Pointer<Drag>>,
    mut query: Query<&mut Node, With<GameWindow>>,
    drag_state: Res<DragState>,
) {
    // Don't move the window while dragging an inventory item.
    if drag_state.dragging.is_some() {
        return;
    }

    let Ok(mut node) = query.get_mut(trigger.event_target()) else {
        return;
    };

    let delta = trigger.event().delta;
    if let Val::Px(ref mut left) = node.margin.left {
        *left += delta.x;
    }
    if let Val::Px(ref mut top) = node.margin.top {
        *top += delta.y;
    }
}

/// Observer: give focus to a window when it is clicked.
///
/// Raises the clicked window's `ZIndex` above all other windows and records it
/// in the [`FocusedWindow`] resource so that ESC closes it first.
fn on_window_focus(
    trigger: On<Pointer<Press>>,
    mut focused: ResMut<FocusedWindow>,
    mut query: Query<&mut ZIndex, With<GameWindow>>,
) {
    let clicked = trigger.event_target();
    focused.0 = Some(clicked);

    // Lower every window, then raise the one that was clicked.
    for mut z in query.iter_mut() {
        *z = ZIndex(0);
    }
    if let Ok(mut z) = query.get_mut(clicked) {
        *z = ZIndex(1);
    }
}

// ── Systems ──

/// Close the topmost visible window when ESC is pressed.
///
/// Closes the [`FocusedWindow`] first (last window the user clicked).
/// Falls back to priority order (Crafting > Inventory) when no focused window
/// is currently visible.
pub fn close_topmost_on_esc(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut windows: Query<(Entity, &GameWindow, &mut Visibility)>,
    mut inv_state: ResMut<InventoryScreenState>,
    mut open_station: ResMut<OpenStation>,
    mut hand_craft_open: ResMut<HandCraftOpen>,
    mut open_trader: ResMut<OpenTrader>,
    focused: Res<FocusedWindow>,
    chat_state: Res<crate::chat::ChatState>,
) {
    if chat_state.is_active {
        return;
    }

    if !keyboard.just_pressed(KeyCode::Escape) {
        return;
    }

    // Prefer the focused window if it still exists and is visible.
    let focused_target = focused.0.and_then(|e| {
        windows
            .get(e)
            .ok()
            .filter(|(_, _, vis)| **vis != Visibility::Hidden)
            .map(|(entity, _, _)| entity)
    });

    // Fall back to highest-priority visible window.
    let target = focused_target.or_else(|| {
        let mut best: Option<(Entity, u8)> = None;
        for (entity, window, vis) in windows.iter() {
            if *vis == Visibility::Hidden {
                continue;
            }
            let priority = match window {
                GameWindow::Trading => 3,
                GameWindow::Crafting => 2,
                GameWindow::Inventory => 1,
            };
            if best.is_none() || priority > best.unwrap().1 {
                best = Some((entity, priority));
            }
        }
        best.map(|(e, _)| e)
    });

    let Some(entity) = target else {
        return;
    };

    if let Ok((_, window, mut vis)) = windows.get_mut(entity) {
        close_window(
            *window,
            &mut vis,
            &mut inv_state,
            &mut open_station,
            &mut hand_craft_open,
            &mut open_trader,
        );
    }
}

/// Handle clicks on any [`WindowCloseButton`].
pub fn handle_window_close_button(
    buttons: Query<(&Interaction, &WindowCloseButton), Changed<Interaction>>,
    mut windows: Query<(&GameWindow, &mut Visibility)>,
    mut inv_state: ResMut<InventoryScreenState>,
    mut open_station: ResMut<OpenStation>,
    mut hand_craft_open: ResMut<HandCraftOpen>,
    mut open_trader: ResMut<OpenTrader>,
) {
    for (interaction, close_btn) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if let Ok((window, mut vis)) = windows.get_mut(close_btn.window) {
            close_window(
                *window,
                &mut vis,
                &mut inv_state,
                &mut open_station,
                &mut hand_craft_open,
                &mut open_trader,
            );
        }
    }
}

/// Perform the close action for a given window kind.
fn close_window(
    window: GameWindow,
    vis: &mut Visibility,
    inv_state: &mut InventoryScreenState,
    open_station: &mut OpenStation,
    hand_craft_open: &mut HandCraftOpen,
    open_trader: &mut OpenTrader,
) {
    match window {
        GameWindow::Inventory => {
            inv_state.visible = false;
            *vis = Visibility::Hidden;
        }
        GameWindow::Crafting => {
            open_station.0 = None;
            hand_craft_open.0 = false;
        }
        GameWindow::Trading => {
            open_trader.0 = None;
        }
    }
}
