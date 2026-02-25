pub mod chunk;
pub mod terrain_gen;
pub mod tile;

use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::world::chunk::{LoadedChunks, TilemapTextureHandle, WorldMap};

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

/// Wrap tile X coordinate for horizontal wrap-around.
pub fn wrap_tile_x(tile_x: i32) -> i32 {
    tile_x.rem_euclid(WORLD_WIDTH_TILES)
}

/// Wrap chunk X coordinate for horizontal wrap-around.
pub fn wrap_chunk_x(chunk_x: i32) -> i32 {
    chunk_x.rem_euclid(WORLD_WIDTH_CHUNKS)
}

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldMap>()
            .init_resource::<LoadedChunks>()
            .add_systems(Startup, create_tilemap_texture)
            .add_systems(Update, chunk::chunk_loading_system);
    }
}

fn create_tilemap_texture(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let image = Image::new_fill(
        Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[255, 255, 255, 255],
        TextureFormat::Rgba8UnormSrgb,
        default(),
    );
    let handle = images.add(image);
    commands.insert_resource(TilemapTextureHandle(handle));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_tile_x_identity() {
        assert_eq!(wrap_tile_x(0), 0);
        assert_eq!(wrap_tile_x(100), 100);
        assert_eq!(wrap_tile_x(WORLD_WIDTH_TILES - 1), WORLD_WIDTH_TILES - 1);
    }

    #[test]
    fn wrap_tile_x_overflow() {
        assert_eq!(wrap_tile_x(WORLD_WIDTH_TILES), 0);
        assert_eq!(wrap_tile_x(WORLD_WIDTH_TILES + 1), 1);
        assert_eq!(wrap_tile_x(WORLD_WIDTH_TILES * 2 + 5), 5);
    }

    #[test]
    fn wrap_tile_x_negative() {
        assert_eq!(wrap_tile_x(-1), WORLD_WIDTH_TILES - 1);
        assert_eq!(wrap_tile_x(-WORLD_WIDTH_TILES), 0);
        assert_eq!(wrap_tile_x(-WORLD_WIDTH_TILES - 1), WORLD_WIDTH_TILES - 1);
    }

    #[test]
    fn wrap_chunk_x_identity() {
        assert_eq!(wrap_chunk_x(0), 0);
        assert_eq!(wrap_chunk_x(WORLD_WIDTH_CHUNKS - 1), WORLD_WIDTH_CHUNKS - 1);
    }

    #[test]
    fn wrap_chunk_x_overflow() {
        assert_eq!(wrap_chunk_x(WORLD_WIDTH_CHUNKS), 0);
        assert_eq!(wrap_chunk_x(WORLD_WIDTH_CHUNKS + 3), 3);
    }

    #[test]
    fn wrap_chunk_x_negative() {
        assert_eq!(wrap_chunk_x(-1), WORLD_WIDTH_CHUNKS - 1);
        assert_eq!(wrap_chunk_x(-3), WORLD_WIDTH_CHUNKS - 3);
    }
}
