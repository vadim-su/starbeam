pub mod atlas;
pub mod autotile;
pub mod biome_map;
pub mod chunk;
pub mod ctx;
pub mod mesh_builder;
pub mod terrain_gen;
pub mod tile_renderer;

use bevy::prelude::*;

use crate::sets::GameSet;
use crate::world::chunk::{LoadedChunks, WorldMap};
use crate::world::mesh_builder::MeshBuildBuffers;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldMap>()
            .init_resource::<LoadedChunks>()
            .init_resource::<MeshBuildBuffers>()
            .add_systems(
                Update,
                (chunk::chunk_loading_system, chunk::rebuild_dirty_chunks)
                    .chain()
                    .in_set(GameSet::WorldUpdate),
            );
    }
}
