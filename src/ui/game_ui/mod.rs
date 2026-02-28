pub mod components;
pub mod hotbar;
pub mod inventory;
pub mod theme;

use bevy::prelude::*;

use crate::registry::AppState;

pub use components::*;
pub use theme::*;

pub struct GameUiPlugin;

impl Plugin for GameUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DragState>()
            .init_resource::<HoveredSlot>()
            .init_resource::<InventoryScreenState>()
            .insert_resource(UiTheme::load())
            .add_systems(OnEnter(AppState::InGame), spawn_game_ui)
            .add_systems(Update, (hotbar::update_hotbar_slots, toggle_inventory));
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
