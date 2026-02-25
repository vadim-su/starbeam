pub mod debug_hud;

use bevy::prelude::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, debug_hud::spawn_debug_hud)
            .add_systems(Update, debug_hud::update_debug_hud);
    }
}
