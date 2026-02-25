pub mod chunk;
pub mod terrain_gen;
pub mod tile;

use bevy::prelude::*;

use crate::registry::AppState;
use crate::world::chunk::{LoadedChunks, TilemapTextureHandle, WorldMap};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldMap>()
            .init_resource::<LoadedChunks>()
            .add_systems(OnEnter(AppState::InGame), load_tile_atlas)
            .add_systems(
                Update,
                chunk::chunk_loading_system.run_if(in_state(AppState::InGame)),
            );
    }
}

fn load_tile_atlas(mut commands: Commands, asset_server: Res<AssetServer>) {
    let handle: Handle<Image> = asset_server.load("terrain/tiles.png");
    commands.insert_resource(TilemapTextureHandle(handle));
}
