pub mod debug_hud;

use bevy::prelude::*;

use crate::registry::AppState;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), debug_hud::spawn_debug_hud)
            .add_systems(
                Update,
                debug_hud::update_debug_hud.run_if(in_state(AppState::InGame)),
            );
    }
}
