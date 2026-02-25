pub mod debug_panel;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

use crate::registry::AppState;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<debug_panel::DebugUiState>()
            .add_systems(
                Update,
                debug_panel::toggle_debug_panel.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                debug_panel::draw_debug_panel.run_if(in_state(AppState::InGame)),
            );
    }
}
