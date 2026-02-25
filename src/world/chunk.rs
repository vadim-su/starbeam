use std::collections::HashMap;

use bevy::prelude::*;

use crate::world::terrain_gen;
use crate::world::tile::TileType;
use crate::world::{CHUNK_SIZE, TILE_SIZE, WORLD_HEIGHT_TILES, WORLD_WIDTH_TILES};

/// Marker component on tilemap entities to identify which chunk they represent.
#[derive(Component)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
}

/// Tile data for a single chunk. Row-major: index = local_y * CHUNK_SIZE + local_x.
pub struct ChunkData {
    pub tiles: Vec<TileType>,
}

impl ChunkData {
    pub fn get(&self, local_x: u32, local_y: u32) -> TileType {
        self.tiles[(local_y * CHUNK_SIZE + local_x) as usize]
    }

    pub fn set(&mut self, local_x: u32, local_y: u32, tile: TileType) {
        self.tiles[(local_y * CHUNK_SIZE + local_x) as usize] = tile;
    }
}

/// Authoritative world tile data. Chunks are lazily generated and cached.
#[derive(Resource)]
pub struct WorldMap {
    pub seed: u32,
    pub chunks: HashMap<(i32, i32), ChunkData>,
}

impl Default for WorldMap {
    fn default() -> Self {
        Self {
            seed: 42,
            chunks: HashMap::new(),
        }
    }
}

impl WorldMap {
    /// Get or generate chunk data at the given chunk coordinates.
    pub fn get_or_generate_chunk(&mut self, chunk_x: i32, chunk_y: i32) -> &ChunkData {
        self.chunks
            .entry((chunk_x, chunk_y))
            .or_insert_with(|| ChunkData {
                tiles: terrain_gen::generate_chunk_tiles(self.seed, chunk_x, chunk_y),
            })
    }

    /// Get tile type at absolute tile coordinates.
    pub fn get_tile(&mut self, tile_x: i32, tile_y: i32) -> TileType {
        if tile_x < 0 || tile_x >= WORLD_WIDTH_TILES || tile_y < 0 || tile_y >= WORLD_HEIGHT_TILES {
            if tile_y >= WORLD_HEIGHT_TILES {
                return TileType::Air; // sky
            }
            return TileType::Stone; // walls/floor
        }
        let (cx, cy) = tile_to_chunk(tile_x, tile_y);
        let (lx, ly) = tile_to_local(tile_x, tile_y);
        self.get_or_generate_chunk(cx, cy).get(lx, ly)
    }

    /// Set tile type at absolute tile coordinates.
    pub fn set_tile(&mut self, tile_x: i32, tile_y: i32, tile: TileType) {
        if tile_x < 0 || tile_x >= WORLD_WIDTH_TILES || tile_y < 0 || tile_y >= WORLD_HEIGHT_TILES {
            return;
        }
        let (cx, cy) = tile_to_chunk(tile_x, tile_y);
        let (lx, ly) = tile_to_local(tile_x, tile_y);
        // Ensure chunk exists
        self.get_or_generate_chunk(cx, cy);
        self.chunks.get_mut(&(cx, cy)).unwrap().set(lx, ly, tile);
    }

    /// Check if a tile is solid at absolute tile coordinates.
    pub fn is_solid(&mut self, tile_x: i32, tile_y: i32) -> bool {
        self.get_tile(tile_x, tile_y).is_solid()
    }
}

/// Tracks which chunks currently have spawned tilemap entities.
#[derive(Resource, Default)]
pub struct LoadedChunks {
    pub map: HashMap<(i32, i32), Entity>,
}

/// Handle to the 1x1 white pixel texture used for color-only tiles.
#[derive(Resource)]
pub struct TilemapTextureHandle(pub Handle<Image>);

// --- Coordinate conversion helpers ---

pub fn tile_to_chunk(tile_x: i32, tile_y: i32) -> (i32, i32) {
    (
        tile_x.div_euclid(CHUNK_SIZE as i32),
        tile_y.div_euclid(CHUNK_SIZE as i32),
    )
}

pub fn tile_to_local(tile_x: i32, tile_y: i32) -> (u32, u32) {
    (
        tile_x.rem_euclid(CHUNK_SIZE as i32) as u32,
        tile_y.rem_euclid(CHUNK_SIZE as i32) as u32,
    )
}

pub fn world_to_tile(world_x: f32, world_y: f32) -> (i32, i32) {
    (
        (world_x / TILE_SIZE).floor() as i32,
        (world_y / TILE_SIZE).floor() as i32,
    )
}

pub fn chunk_world_position(chunk_x: i32, chunk_y: i32) -> Vec3 {
    Vec3::new(
        chunk_x as f32 * CHUNK_SIZE as f32 * TILE_SIZE,
        chunk_y as f32 * CHUNK_SIZE as f32 * TILE_SIZE,
        0.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_to_chunk_basic() {
        assert_eq!(tile_to_chunk(0, 0), (0, 0));
        assert_eq!(tile_to_chunk(31, 31), (0, 0));
        assert_eq!(tile_to_chunk(32, 0), (1, 0));
        assert_eq!(tile_to_chunk(63, 63), (1, 1));
    }

    #[test]
    fn tile_to_local_basic() {
        assert_eq!(tile_to_local(0, 0), (0, 0));
        assert_eq!(tile_to_local(31, 31), (31, 31));
        assert_eq!(tile_to_local(32, 0), (0, 0));
        assert_eq!(tile_to_local(33, 35), (1, 3));
    }

    #[test]
    fn world_to_tile_basic() {
        assert_eq!(world_to_tile(0.0, 0.0), (0, 0));
        assert_eq!(world_to_tile(32.0, 0.0), (1, 0));
        assert_eq!(world_to_tile(31.9, 63.9), (0, 1));
        assert_eq!(world_to_tile(64.0, 64.0), (2, 2));
    }

    #[test]
    fn world_to_tile_negative() {
        assert_eq!(world_to_tile(-1.0, -1.0), (-1, -1));
        assert_eq!(world_to_tile(-32.0, 0.0), (-1, 0));
    }

    #[test]
    fn chunk_world_position_basic() {
        let pos = chunk_world_position(0, 0);
        assert_eq!(pos, Vec3::new(0.0, 0.0, 0.0));
        let pos = chunk_world_position(1, 2);
        assert_eq!(pos, Vec3::new(1024.0, 2048.0, 0.0));
    }

    #[test]
    fn worldmap_get_tile_deterministic() {
        let mut map = WorldMap::default();
        let t1 = map.get_tile(100, 500);
        let t2 = map.get_tile(100, 500);
        assert_eq!(t1, t2);
    }

    #[test]
    fn worldmap_set_tile() {
        let mut map = WorldMap::default();
        map.set_tile(100, 500, TileType::Air);
        assert_eq!(map.get_tile(100, 500), TileType::Air);
    }

    #[test]
    fn worldmap_out_of_bounds() {
        let mut map = WorldMap::default();
        // Above world is Air
        assert_eq!(map.get_tile(0, WORLD_HEIGHT_TILES), TileType::Air);
        // Below/sides are Stone
        assert_eq!(map.get_tile(-1, 500), TileType::Stone);
        assert_eq!(map.get_tile(0, -1), TileType::Stone);
    }
}
