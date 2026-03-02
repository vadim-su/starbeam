pub mod debug_panel;
pub mod game_ui;
pub mod star_map;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

use crate::cosmos::warp::{handle_warp, WarpToBody};
use crate::registry::AppState;
use crate::sets::GameSet;
use game_ui::GameUiPlugin;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<debug_panel::DebugUiState>()
            .init_resource::<star_map::StarMapState>()
            .add_message::<WarpToBody>()
            .add_plugins(GameUiPlugin)
            .add_systems(
                Update,
                (debug_panel::toggle_debug_panel, star_map::toggle_star_map).in_set(GameSet::Ui),
            )
            .add_systems(
                EguiPrimaryContextPass,
                debug_panel::draw_debug_panel.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                star_map::draw_star_map.run_if(in_state(AppState::InGame)),
            )
            .add_systems(Update, handle_warp.run_if(in_state(AppState::InGame)));
    }
}
