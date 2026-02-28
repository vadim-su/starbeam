pub mod components;
pub mod hotbar;
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
            .add_systems(OnEnter(AppState::InGame), spawn_game_ui);
    }
}

/// Spawn all game UI elements (hotbar, inventory screen).
fn spawn_game_ui(mut commands: Commands, theme: Res<UiTheme>) {
    hotbar::spawn_hotbar(&mut commands, &theme);
}
