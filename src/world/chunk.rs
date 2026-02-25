use std::collections::HashMap;
use std::collections::HashSet;

use bevy::prelude::*;
use bevy_ecs_tilemap::prelude::*;

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

pub fn spawn_chunk(
    commands: &mut Commands,
    world_map: &mut WorldMap,
    loaded_chunks: &mut LoadedChunks,
    texture_handle: &Handle<Image>,
    chunk_x: i32,
    chunk_y: i32,
) {
    if loaded_chunks.map.contains_key(&(chunk_x, chunk_y)) {
        return; // already loaded
    }

    let chunk_data = world_map.get_or_generate_chunk(chunk_x, chunk_y);
    let tilemap_size = TilemapSize::new(CHUNK_SIZE, CHUNK_SIZE);
    let mut tile_storage = TileStorage::empty(tilemap_size);
    let tile_size = TilemapTileSize::new(TILE_SIZE, TILE_SIZE);

    let tilemap_entity = commands.spawn_empty().id();
    let tilemap_id = TilemapId(tilemap_entity);

    // Spawn tile entities as children (skip Air tiles — sparse storage)
    commands.entity(tilemap_entity).with_children(|parent| {
        for local_y in 0..CHUNK_SIZE {
            for local_x in 0..CHUNK_SIZE {
                let tile_type = chunk_data.get(local_x, local_y);
                if let Some(color) = tile_type.color() {
                    let tile_pos = TilePos::new(local_x, local_y);
                    let tile_entity = parent
                        .spawn(TileBundle {
                            position: tile_pos,
                            tilemap_id,
                            texture_index: TileTextureIndex(0),
                            color: TileColor(color),
                            ..Default::default()
                        })
                        .id();
                    tile_storage.set(&tile_pos, tile_entity);
                }
            }
        }
    });

    let grid_size: TilemapGridSize = tile_size.into();
    commands.entity(tilemap_entity).insert((
        TilemapBundle {
            grid_size,
            map_type: TilemapType::Square,
            size: tilemap_size,
            storage: tile_storage,
            texture: TilemapTexture::Single(texture_handle.clone()),
            tile_size,
            transform: Transform::from_translation(chunk_world_position(chunk_x, chunk_y)),
            render_settings: TilemapRenderSettings {
                render_chunk_size: UVec2::new(CHUNK_SIZE, CHUNK_SIZE),
                y_sort: false,
            },
            ..Default::default()
        },
        ChunkCoord {
            x: chunk_x,
            y: chunk_y,
        },
    ));

    loaded_chunks.map.insert((chunk_x, chunk_y), tilemap_entity);
}

pub fn despawn_chunk(
    commands: &mut Commands,
    loaded_chunks: &mut LoadedChunks,
    chunk_x: i32,
    chunk_y: i32,
) {
    if let Some(entity) = loaded_chunks.map.remove(&(chunk_x, chunk_y)) {
        commands.entity(entity).despawn();
    }
}

pub fn chunk_loading_system(
    mut commands: Commands,
    camera_query: Query<&Transform, With<Camera2d>>,
    mut world_map: ResMut<WorldMap>,
    mut loaded_chunks: ResMut<LoadedChunks>,
    texture_handle: Res<TilemapTextureHandle>,
) {
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };
    let camera_pos = camera_transform.translation.truncate();

    // Which chunk is the camera in?
    let (cam_tile_x, cam_tile_y) = world_to_tile(camera_pos.x, camera_pos.y);
    let (cam_chunk_x, cam_chunk_y) = tile_to_chunk(cam_tile_x, cam_tile_y);

    // Determine which chunks should be loaded
    let mut desired: HashSet<(i32, i32)> = HashSet::new();
    let load_radius = crate::world::CHUNK_LOAD_RADIUS;
    for cx in (cam_chunk_x - load_radius)..=(cam_chunk_x + load_radius) {
        for cy in (cam_chunk_y - load_radius)..=(cam_chunk_y + load_radius) {
            if cx >= 0
                && cx < crate::world::WORLD_WIDTH_CHUNKS
                && cy >= 0
                && cy < crate::world::WORLD_HEIGHT_CHUNKS
            {
                desired.insert((cx, cy));
            }
        }
    }

    // Spawn missing chunks
    for &(cx, cy) in &desired {
        if !loaded_chunks.map.contains_key(&(cx, cy)) {
            spawn_chunk(
                &mut commands,
                &mut world_map,
                &mut loaded_chunks,
                &texture_handle.0,
                cx,
                cy,
            );
        }
    }

    // Despawn chunks that are no longer needed
    let to_remove: Vec<(i32, i32)> = loaded_chunks
        .map
        .keys()
        .filter(|k| !desired.contains(k))
        .copied()
        .collect();
    for (cx, cy) in to_remove {
        despawn_chunk(&mut commands, &mut loaded_chunks, cx, cy);
    }
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
