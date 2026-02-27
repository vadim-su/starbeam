pub mod config;
pub mod scroll;
pub mod spawn;
pub mod transition;

use bevy::prelude::*;

use crate::sets::GameSet;

pub struct ParallaxPlugin;

impl Plugin for ParallaxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                transition::track_player_biome,
                transition::parallax_transition_system,
                scroll::parallax_scroll,
            )
                .chain()
                .in_set(GameSet::Parallax),
        );
    }
}
