pub mod components;
pub mod crafting_panel;
pub mod drag_drop;
pub mod hotbar;
pub mod icon_registry;
pub mod inventory;
pub mod slot_sync;
pub mod theme;
pub mod tooltip;
pub mod window;

use bevy::asset::RenderAssetUsages;
use bevy::picking::prelude::*;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::widget::ImageNode;

use crate::registry::AppState;

pub use components::*;
pub use icon_registry::*;
pub use theme::*;
pub use window::{
    FocusedWindow, GameWindow, WindowBody, WindowCloseButton, WindowConfig, WindowEntities,
};

/// Handles for slot frame textures.
#[derive(Resource)]
pub struct SlotFrames {
    pub common: Handle<Image>,
}

impl SlotFrames {
    /// Create with a generated white frame.
    pub fn new(images: &mut Assets<Image>) -> Self {
        let frame = Self::generate_frame();
        Self {
            common: images.add(frame),
        }
    }

    /// Generate a 32x32 white rounded frame.
    fn generate_frame() -> Image {
        let size = 32u32;
        let mut data = vec![0u8; (size * size * 4) as usize];

        // Draw rounded rectangle border
        let border = 2u32;
        let radius = 4u32;

        for y in 0..size {
            for x in 0..size {
                // Distance from edges
                let dx = if x < size / 2 { x } else { size - 1 - x };
                let dy = if y < size / 2 { y } else { size - 1 - y };

                // Check if in corner
                let in_corner = dx < radius && dy < radius;
                let corner_dist = ((dx as f32 - radius as f32).powi(2)
                    + (dy as f32 - radius as f32).powi(2))
                .sqrt();

                // Check if on border
                let on_border = if in_corner {
                    corner_dist <= radius as f32 && corner_dist >= (radius - border) as f32
                } else {
                    dx < border || dy < border
                };

                if on_border {
                    let idx = ((y * size + x) * 4) as usize;
                    data[idx] = 255; // R
                    data[idx + 1] = 255; // G
                    data[idx + 2] = 255; // B
                    data[idx + 3] = 255; // A
                }
            }
        }

        Image::new(
            Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        )
    }
}

pub struct GameUiPlugin;

impl Plugin for GameUiPlugin {
    fn build(&self, app: &mut App) {
        // UiTheme is loaded via the asset system during the Loading phase
        // and hot-reloaded in real-time by hot_reload_ui_theme.
        app.add_plugins(crafting_panel::CraftingUiPlugin)
            .init_resource::<DragState>()
            .init_resource::<HoveredSlot>()
            .init_resource::<InventoryScreenState>()
            .init_resource::<FocusedWindow>()
            .add_systems(
                OnEnter(AppState::InGame),
                (
                    init_slot_frames,
                    load_item_icons,
                    spawn_game_ui,
                    tooltip::spawn_tooltip,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    hotbar::update_hotbar_slots,
                    slot_sync::sync_slot_contents,
                    slot_sync::update_slot_icons,
                    toggle_inventory,
                    drag_drop::update_drag_position,
                    tooltip::update_tooltip,
                    window::close_topmost_on_esc,
                    window::handle_window_close_button,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Toggle inventory screen on I key press.
fn toggle_inventory(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<InventoryScreenState>,
    mut query: Query<&mut Visibility, With<InventoryScreen>>,
) {
    if keyboard.just_pressed(KeyCode::KeyI) {
        state.visible = !state.visible;

        for mut vis in &mut query {
            *vis = if state.visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}

/// Spawn all game UI elements (hotbar, inventory screen).
/// Skips spawning if UI already exists (e.g. after planet warp re-enters InGame).
fn spawn_game_ui(
    mut commands: Commands,
    theme: Res<UiTheme>,
    existing: Query<Entity, With<InventoryScreen>>,
    asset_server: Res<AssetServer>,
) {
    if !existing.is_empty() {
        return;
    }
    hotbar::spawn_hotbar(&mut commands, &theme, &asset_server);
    inventory::spawn_inventory_screen(&mut commands, &theme, &asset_server);
}

/// Initialize slot frame textures.
fn init_slot_frames(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(SlotFrames::new(&mut images));
}

/// Load item icons using paths from ItemDef.icon.
/// When `icon` is `None` and the item has `placeable_object`, the object's
/// sprite is used as the inventory icon (Starbound-style fallback).
fn load_item_icons(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    item_registry: Res<crate::item::ItemRegistry>,
    object_registry: Res<crate::object::registry::ObjectRegistry>,
) {
    let mut icon_registry = ItemIconRegistry::new();

    for i in 0..item_registry.len() {
        let id = crate::item::ItemId(i as u16);
        let def = item_registry.get(id);

        let icon_path: Option<String> = def.icon.clone().or_else(|| {
            // Fallback: use the placed object's sprite as the icon.
            def.placeable_object.as_deref().and_then(|obj_name| {
                object_registry
                    .by_name(obj_name)
                    .map(|oid| object_registry.get(oid).sprite.clone())
            })
        }).or_else(|| {
            // Fallback: generic blueprint icon for Blueprint items without explicit icon.
            if def.item_type == crate::item::definition::ItemType::Blueprint {
                Some("textures/blueprint_icon.png".to_string())
            } else {
                None
            }
        });

        if let Some(path) = icon_path {
            let handle: Handle<Image> = asset_server.load(&path);
            icon_registry.register(id, handle);
        } else {
            warn!(
                "Item '{}' has no icon and no placeable_object fallback",
                def.id
            );
        }
    }

    commands.insert_resource(icon_registry);
}

/// Spawn the standard icon/frame/count children inside a UI slot.
pub fn spawn_slot_icon_children(parent: &mut ChildSpawnerCommands) {
    // Item icon
    parent.spawn((
        ItemIcon,
        ImageNode::default(),
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        Visibility::Hidden,
        Pickable::IGNORE,
    ));
    // Frame overlay
    parent.spawn((
        SlotFrame,
        ImageNode::default(),
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            ..default()
        },
        Visibility::Hidden,
        Pickable::IGNORE,
    ));
    // Count text
    parent.spawn((
        ItemCount,
        Text::new(""),
        TextFont {
            font_size: 9.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(1.0),
            right: Val::Px(2.0),
            ..default()
        },
        Pickable::IGNORE,
    ));
}
