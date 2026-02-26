pub mod atlas;
pub mod autotile;
pub mod chunk;
pub mod terrain_gen;
pub mod tile;

use bevy::prelude::*;

use crate::registry::AppState;
use crate::world::chunk::{LoadedChunks, WorldMap};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldMap>()
            .init_resource::<LoadedChunks>()
            .add_systems(
                Update,
                chunk::chunk_loading_system.run_if(in_state(AppState::InGame)),
            );
    }
}
