pub mod chunk;
pub mod terrain_gen;
pub mod tile;

use bevy::prelude::*;

// World dimensions in tiles
pub const WORLD_WIDTH_TILES: i32 = 2048;
pub const WORLD_HEIGHT_TILES: i32 = 1024;

// Chunk dimensions in tiles
pub const CHUNK_SIZE: u32 = 32;

// Tile size in pixels
pub const TILE_SIZE: f32 = 32.0;

// World dimensions in chunks
pub const WORLD_WIDTH_CHUNKS: i32 = WORLD_WIDTH_TILES / CHUNK_SIZE as i32;
pub const WORLD_HEIGHT_CHUNKS: i32 = WORLD_HEIGHT_TILES / CHUNK_SIZE as i32;

// How many chunks around camera to keep loaded
pub const CHUNK_LOAD_RADIUS: i32 = 3;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, _app: &mut App) {
        // Systems will be added in later tasks
    }
}
