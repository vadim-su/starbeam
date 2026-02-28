pub mod components;
pub mod drag_drop;
pub mod hotbar;
pub mod inventory;
pub mod slot_sync;
pub mod theme;
pub mod tooltip;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::registry::AppState;

pub use components::*;
pub use theme::*;

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
        app.init_resource::<DragState>()
            .init_resource::<HoveredSlot>()
            .init_resource::<InventoryScreenState>()
            .insert_resource(UiTheme::load())
            .add_systems(
                OnEnter(AppState::InGame),
                (init_slot_frames, spawn_game_ui, tooltip::spawn_tooltip).chain(),
            )
            .add_systems(
                Update,
                (
                    hotbar::update_hotbar_slots,
                    slot_sync::sync_slot_contents,
                    toggle_inventory,
                    drag_drop::update_drag_position,
                    tooltip::update_tooltip,
                ),
            );
    }
}

/// Toggle inventory screen on E or I key press.
fn toggle_inventory(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<InventoryScreenState>,
    mut query: Query<&mut Visibility, With<InventoryScreen>>,
) {
    if keyboard.just_pressed(KeyCode::KeyE) || keyboard.just_pressed(KeyCode::KeyI) {
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
fn spawn_game_ui(mut commands: Commands, theme: Res<UiTheme>) {
    hotbar::spawn_hotbar(&mut commands, &theme);
    inventory::spawn_inventory_screen(&mut commands, &theme);
}

/// Initialize slot frame textures.
fn init_slot_frames(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(SlotFrames::new(&mut images));
}
