# Minimum Planet Prototype — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Playable 2D sandbox prototype — procedurally generated planet (2048×1024 tiles), walk/jump, break/place colored placeholder blocks.

**Architecture:** Chunk-based tilemap (32×32 tiles per chunk) via `bevy_ecs_tilemap`. `WorldMap` resource as authoritative tile data; ECS entities for rendering visible chunks only. Custom minimal AABB physics.

**Tech Stack:** Bevy 0.18.0, bevy_ecs_tilemap 0.18.1, noise 0.9 (Perlin)

**Design Doc:** `docs/plans/2026-02-25-minimum-planet-prototype-design.md`

**External API Reference:**
- `.tmp/external-context/bevy/` — Bevy 0.18 API (app, sprites, input, camera, transforms, queries, time, window)
- `.tmp/external-context/bevy_ecs_tilemap/` — bevy_ecs_tilemap 0.18.1 API + chunk patterns

---

## Task 1: Project Setup

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/main.rs`

**Step 1: Update Cargo.toml**

```toml
[package]
name = "starbeam"
version = "0.1.0"
edition = "2024"

[dependencies]
bevy = "0.18.0"
bevy_ecs_tilemap = "0.18"
noise = "0.9"
```

**Step 2: Create minimal App in main.rs**

```rust
use bevy::prelude::*;
use bevy_ecs_tilemap::prelude::*;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Starbeam".into(),
                        resolution: (1280.0, 720.0).into(),
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins(TilemapPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}
```

**Step 3: Verify compilation and run**

Run: `cargo run`
Expected: window titled "Starbeam" at 1280×720 with black background.

**Step 4: Commit**

```bash
git add -A && git commit -m "feat: project setup with Bevy 0.18 and bevy_ecs_tilemap"
```

---

## Task 2: Tile Types & World Constants

**Files:**
- Create: `src/world/mod.rs`
- Create: `src/world/tile.rs`
- Modify: `src/main.rs`

**Step 1: Write test for tile color mapping**

In `src/world/tile.rs`:

```rust
use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TileType {
    #[default]
    Air,
    Grass,
    Dirt,
    Stone,
}

impl TileType {
    pub fn color(self) -> Option<Color> {
        match self {
            TileType::Air => None,
            TileType::Grass => Some(Color::srgb(0.2, 0.7, 0.2)),
            TileType::Dirt => Some(Color::srgb(0.55, 0.35, 0.15)),
            TileType::Stone => Some(Color::srgb(0.5, 0.5, 0.5)),
        }
    }

    pub fn is_solid(self) -> bool {
        !matches!(self, TileType::Air)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn air_has_no_color() {
        assert!(TileType::Air.color().is_none());
    }

    #[test]
    fn solid_tiles_have_colors() {
        assert!(TileType::Grass.color().is_some());
        assert!(TileType::Dirt.color().is_some());
        assert!(TileType::Stone.color().is_some());
    }

    #[test]
    fn air_is_not_solid() {
        assert!(!TileType::Air.is_solid());
    }

    #[test]
    fn non_air_is_solid() {
        assert!(TileType::Grass.is_solid());
        assert!(TileType::Dirt.is_solid());
        assert!(TileType::Stone.is_solid());
    }
}
```

**Step 2: Run tests**

Run: `cargo test -- tile`
Expected: all 4 tests pass.

**Step 3: Create world/mod.rs with constants and plugin stub**

```rust
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
```

**Step 4: Wire WorldPlugin into main.rs**

Add `mod world;` and `.add_plugins(world::WorldPlugin)` to App.

```rust
mod world;

use bevy::prelude::*;
use bevy_ecs_tilemap::prelude::*;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Starbeam".into(),
                        resolution: (1280.0, 720.0).into(),
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins(TilemapPlugin)
        .add_plugins(world::WorldPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}
```

**Step 5: Verify**

Run: `cargo test && cargo run`
Expected: tests pass, window opens.

**Step 6: Commit**

```bash
git add -A && git commit -m "feat: tile types with color mapping and world constants"
```

---

## Task 3: Terrain Generation

**Files:**
- Create: `src/world/terrain_gen.rs`
- Modify: `src/world/mod.rs`

**Step 1: Write tests for terrain generation**

In `src/world/terrain_gen.rs`:

```rust
use noise::{NoiseFn, Perlin};

use crate::world::{WORLD_HEIGHT_TILES, WORLD_WIDTH_TILES, CHUNK_SIZE};
use crate::world::tile::TileType;

const SURFACE_BASE: f64 = 0.7; // 70% from bottom
const SURFACE_AMPLITUDE: f64 = 40.0; // tiles of variation
const SURFACE_FREQUENCY: f64 = 0.02;
const CAVE_FREQUENCY: f64 = 0.07;
const CAVE_THRESHOLD: f64 = 0.3;
const DIRT_DEPTH: i32 = 4;

/// Get the surface height (in tile Y) at a given tile X coordinate.
pub fn surface_height(seed: u32, tile_x: i32) -> i32 {
    let perlin = Perlin::new(seed);
    let base = SURFACE_BASE * WORLD_HEIGHT_TILES as f64;
    let noise_val = perlin.get([tile_x as f64 * SURFACE_FREQUENCY, 0.0]);
    (base + noise_val * SURFACE_AMPLITUDE) as i32
}

/// Generate tile type at an absolute tile position.
pub fn generate_tile(seed: u32, tile_x: i32, tile_y: i32) -> TileType {
    if tile_x < 0 || tile_x >= WORLD_WIDTH_TILES || tile_y < 0 || tile_y >= WORLD_HEIGHT_TILES {
        return TileType::Air;
    }

    let surface_y = surface_height(seed, tile_x);

    if tile_y > surface_y {
        return TileType::Air;
    }
    if tile_y == surface_y {
        return TileType::Grass;
    }
    if tile_y > surface_y - DIRT_DEPTH {
        return TileType::Dirt;
    }

    // Below dirt layer: stone with caves
    let cave_perlin = Perlin::new(seed.wrapping_add(1));
    let cave_val = cave_perlin.get([tile_x as f64 * CAVE_FREQUENCY, tile_y as f64 * CAVE_FREQUENCY]);
    if cave_val.abs() < CAVE_THRESHOLD {
        TileType::Air // cave
    } else {
        TileType::Stone
    }
}

/// Generate all tiles for a chunk. Returns Vec of CHUNK_SIZE*CHUNK_SIZE tiles in row-major order.
/// Index = local_y * CHUNK_SIZE + local_x
pub fn generate_chunk_tiles(seed: u32, chunk_x: i32, chunk_y: i32) -> Vec<TileType> {
    let base_x = chunk_x * CHUNK_SIZE as i32;
    let base_y = chunk_y * CHUNK_SIZE as i32;
    let mut tiles = Vec::with_capacity((CHUNK_SIZE * CHUNK_SIZE) as usize);

    for local_y in 0..CHUNK_SIZE as i32 {
        for local_x in 0..CHUNK_SIZE as i32 {
            tiles.push(generate_tile(seed, base_x + local_x, base_y + local_y));
        }
    }

    tiles
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SEED: u32 = 42;

    #[test]
    fn surface_height_is_deterministic() {
        let h1 = surface_height(TEST_SEED, 100);
        let h2 = surface_height(TEST_SEED, 100);
        assert_eq!(h1, h2);
    }

    #[test]
    fn surface_height_is_within_bounds() {
        for x in 0..WORLD_WIDTH_TILES {
            let h = surface_height(TEST_SEED, x);
            assert!(h >= 0 && h < WORLD_HEIGHT_TILES, "surface at x={x} is {h}");
        }
    }

    #[test]
    fn above_surface_is_air() {
        let h = surface_height(TEST_SEED, 500);
        assert_eq!(generate_tile(TEST_SEED, 500, h + 1), TileType::Air);
        assert_eq!(generate_tile(TEST_SEED, 500, h + 10), TileType::Air);
    }

    #[test]
    fn surface_is_grass() {
        let h = surface_height(TEST_SEED, 500);
        assert_eq!(generate_tile(TEST_SEED, 500, h), TileType::Grass);
    }

    #[test]
    fn below_surface_is_dirt_then_stone() {
        let h = surface_height(TEST_SEED, 500);
        // Just below surface should be Dirt
        assert_eq!(generate_tile(TEST_SEED, 500, h - 1), TileType::Dirt);
        // Deep underground should be Stone or Air (cave)
        let deep_tile = generate_tile(TEST_SEED, 500, 10);
        assert!(matches!(deep_tile, TileType::Stone | TileType::Air));
    }

    #[test]
    fn chunk_generation_has_correct_size() {
        let tiles = generate_chunk_tiles(TEST_SEED, 0, 0);
        assert_eq!(tiles.len(), (CHUNK_SIZE * CHUNK_SIZE) as usize);
    }

    #[test]
    fn chunk_generation_is_deterministic() {
        let tiles1 = generate_chunk_tiles(TEST_SEED, 5, 10);
        let tiles2 = generate_chunk_tiles(TEST_SEED, 5, 10);
        assert_eq!(tiles1, tiles2);
    }

    #[test]
    fn out_of_bounds_is_air() {
        assert_eq!(generate_tile(TEST_SEED, -1, 500), TileType::Air);
        assert_eq!(generate_tile(TEST_SEED, WORLD_WIDTH_TILES, 500), TileType::Air);
    }
}
```

**Step 2: Add module to world/mod.rs**

Add `pub mod terrain_gen;` to `src/world/mod.rs`.

**Step 3: Run tests**

Run: `cargo test -- terrain_gen`
Expected: all 8 tests pass.

**Step 4: Commit**

```bash
git add -A && git commit -m "feat: terrain generation with Perlin noise and caves"
```

---

## Task 4: World Map Resource & Chunk Data

**Files:**
- Create: `src/world/chunk.rs`
- Modify: `src/world/mod.rs`

**Step 1: Implement ChunkData, WorldMap, LoadedChunks, coordinate helpers**

In `src/world/chunk.rs`:

```rust
use std::collections::HashMap;

use bevy::prelude::*;

use crate::world::{CHUNK_SIZE, TILE_SIZE, WORLD_WIDTH_TILES, WORLD_HEIGHT_TILES};
use crate::world::tile::TileType;
use crate::world::terrain_gen;

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
        self.chunks.entry((chunk_x, chunk_y)).or_insert_with(|| {
            ChunkData {
                tiles: terrain_gen::generate_chunk_tiles(self.seed, chunk_x, chunk_y),
            }
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
```

**Step 2: Add module to world/mod.rs**

Add `pub mod chunk;` to `src/world/mod.rs`.

**Step 3: Run tests**

Run: `cargo test -- chunk`
Expected: all 8 tests pass.

**Step 4: Commit**

```bash
git add -A && git commit -m "feat: world map resource with chunk data and coordinate helpers"
```

---

## Task 5: Chunk Spawning & Despawning

**Files:**
- Modify: `src/world/chunk.rs`
- Modify: `src/world/mod.rs`

**Step 1: Add white pixel texture creation to WorldPlugin**

In `src/world/mod.rs`, add a Startup system:

```rust
pub mod tile;
pub mod terrain_gen;
pub mod chunk;

use bevy::prelude::*;
use bevy::image::Image;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::world::chunk::{WorldMap, LoadedChunks, TilemapTextureHandle};

// ... (keep existing constants) ...

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldMap>()
            .init_resource::<LoadedChunks>()
            .add_systems(Startup, create_tilemap_texture);
    }
}

fn create_tilemap_texture(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let image = Image::new_fill(
        Extent3d {
            width: 1,
            height: 1,
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
```

**Step 2: Add spawn_chunk and despawn_chunk functions to chunk.rs**

Add these functions to `src/world/chunk.rs`:

```rust
use bevy_ecs_tilemap::prelude::*;
use bevy::image::Image;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

// ... (keep existing code) ...

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

    // Spawn tile entities as children (skip Air tiles)
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
        ChunkCoord { x: chunk_x, y: chunk_y },
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
```

**Step 3: Verify compilation**

Run: `cargo build`
Expected: compiles without errors.

**Step 4: Commit**

```bash
git add -A && git commit -m "feat: chunk spawn/despawn with bevy_ecs_tilemap colored tiles"
```

---

## Task 6: Chunk Loading System

**Files:**
- Modify: `src/world/chunk.rs`
- Modify: `src/world/mod.rs`

**Step 1: Implement chunk_loading_system in chunk.rs**

Add to `src/world/chunk.rs`:

```rust
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
            // Only load chunks within world bounds
            if cx >= 0 && cx < crate::world::WORLD_WIDTH_CHUNKS
                && cy >= 0 && cy < crate::world::WORLD_HEIGHT_CHUNKS
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
```

Add `use std::collections::HashSet;` to the top of chunk.rs (alongside the existing HashMap import).

**Step 2: Wire into WorldPlugin**

In `src/world/mod.rs`, update the plugin:

```rust
impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldMap>()
            .init_resource::<LoadedChunks>()
            .add_systems(Startup, create_tilemap_texture)
            .add_systems(Update, chunk::chunk_loading_system);
    }
}
```

**Step 3: Verify: cargo run shows terrain**

Run: `cargo run`
Expected: terrain is visible around camera position (0,0). Since camera starts at origin and world starts at origin, you should see the bottom-left corner of the world with stone/dirt.

To see the surface, temporarily set camera position in the setup function:

```rust
fn setup(mut commands: Commands) {
    // Spawn camera near surface (tile 1024, ~717 => pixel 32768, ~22944)
    commands.spawn((
        Camera2d,
        Transform::from_xyz(32768.0, 22944.0, 0.0),
    ));
}
```

You should see green grass line with dirt below and sky (black) above.

Revert the camera position change after verifying.

**Step 4: Commit**

```bash
git add -A && git commit -m "feat: camera-based chunk loading/unloading system"
```

---

## Task 7: Player Spawn & Movement

**Files:**
- Create: `src/player/mod.rs`
- Create: `src/player/movement.rs`
- Modify: `src/main.rs`

**Step 1: Create player components and constants**

In `src/player/mod.rs`:

```rust
pub mod movement;

use bevy::prelude::*;

use crate::world;
use crate::world::terrain_gen;

pub const PLAYER_SPEED: f32 = 200.0;
pub const JUMP_VELOCITY: f32 = 400.0;
pub const GRAVITY: f32 = 800.0;
pub const PLAYER_WIDTH: f32 = 24.0;
pub const PLAYER_HEIGHT: f32 = 48.0;

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct Velocity {
    pub x: f32,
    pub y: f32,
}

impl Default for Velocity {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

#[derive(Component)]
pub struct Grounded(pub bool);

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player)
            .add_systems(Update, (
                movement::player_input,
                movement::apply_gravity,
                movement::apply_velocity,
            ).chain());
    }
}

fn spawn_player(mut commands: Commands) {
    let spawn_tile_x = world::WORLD_WIDTH_TILES / 2;
    let surface_y = terrain_gen::surface_height(42, spawn_tile_x);
    let spawn_pixel_x = spawn_tile_x as f32 * world::TILE_SIZE + world::TILE_SIZE / 2.0;
    let spawn_pixel_y = (surface_y + 2) as f32 * world::TILE_SIZE + PLAYER_HEIGHT / 2.0;

    commands.spawn((
        Player,
        Velocity::default(),
        Grounded(false),
        Sprite::from_color(Color::srgb(0.2, 0.4, 0.9), Vec2::new(PLAYER_WIDTH, PLAYER_HEIGHT)),
        Transform::from_xyz(spawn_pixel_x, spawn_pixel_y, 1.0), // z=1 above tilemap
    ));
}
```

**Step 2: Implement movement systems**

In `src/player/movement.rs`:

```rust
use bevy::prelude::*;

use crate::player::{Player, Velocity, Grounded, PLAYER_SPEED, JUMP_VELOCITY, GRAVITY};

pub fn player_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut Velocity, &Grounded), With<Player>>,
) {
    for (mut vel, grounded) in &mut query {
        // Horizontal movement
        vel.x = 0.0;
        if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
            vel.x -= PLAYER_SPEED;
        }
        if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
            vel.x += PLAYER_SPEED;
        }

        // Jump
        if keys.just_pressed(KeyCode::Space) && grounded.0 {
            vel.y = JUMP_VELOCITY;
        }
    }
}

pub fn apply_gravity(
    time: Res<Time>,
    mut query: Query<&mut Velocity, With<Player>>,
) {
    let dt = time.delta_secs();
    for mut vel in &mut query {
        vel.y -= GRAVITY * dt;
    }
}

pub fn apply_velocity(
    time: Res<Time>,
    mut query: Query<(&Velocity, &mut Transform), With<Player>>,
) {
    let dt = time.delta_secs();
    for (vel, mut transform) in &mut query {
        transform.translation.x += vel.x * dt;
        transform.translation.y += vel.y * dt;
    }
}
```

**Step 3: Wire PlayerPlugin into main.rs**

```rust
mod world;
mod player;

// ... in main():
    .add_plugins(world::WorldPlugin)
    .add_plugins(player::PlayerPlugin)
```

**Step 4: Verify**

Run: `cargo run`
Expected: blue rectangle (player) appears on the surface and falls through the world (no collision yet). Can move left/right with A/D.

**Step 5: Commit**

```bash
git add -A && git commit -m "feat: player spawn with movement and gravity (no collision)"
```

---

## Task 8: Camera Follow

**Files:**
- Create: `src/camera/mod.rs`
- Create: `src/camera/follow.rs`
- Modify: `src/main.rs`

**Step 1: Implement camera follow system**

In `src/camera/mod.rs`:

```rust
pub mod follow;

use bevy::prelude::*;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, follow::camera_follow_player);
    }
}
```

In `src/camera/follow.rs`:

```rust
use bevy::prelude::*;

use crate::player::Player;
use crate::world::{WORLD_WIDTH_TILES, WORLD_HEIGHT_TILES, TILE_SIZE};

pub fn camera_follow_player(
    player_query: Query<&Transform, (With<Player>, Without<Camera2d>)>,
    mut camera_query: Query<&mut Transform, (With<Camera2d>, Without<Player>)>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let Ok(mut camera_transform) = camera_query.single_mut() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

    let half_w = window.width() / 2.0;
    let half_h = window.height() / 2.0;
    let world_w = WORLD_WIDTH_TILES as f32 * TILE_SIZE;
    let world_h = WORLD_HEIGHT_TILES as f32 * TILE_SIZE;

    let mut target = player_transform.translation;

    // Clamp camera so it doesn't show outside the world
    target.x = target.x.clamp(half_w, (world_w - half_w).max(half_w));
    target.y = target.y.clamp(half_h, (world_h - half_h).max(half_h));

    camera_transform.translation.x = target.x;
    camera_transform.translation.y = target.y;
}
```

**Step 2: Wire CameraPlugin into main.rs and remove the temporary camera position**

```rust
mod world;
mod player;
mod camera;

// In main():
    .add_plugins(world::WorldPlugin)
    .add_plugins(player::PlayerPlugin)
    .add_plugins(camera::CameraPlugin)

// Keep setup simple:
fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}
```

**Step 3: Verify**

Run: `cargo run`
Expected: camera follows the player. Terrain loads around the player. Player still falls through world.

**Step 4: Commit**

```bash
git add -A && git commit -m "feat: camera follow player with world bounds clamping"
```

---

## Task 9: Tile Collision

**Files:**
- Create: `src/player/collision.rs`
- Modify: `src/player/mod.rs`

**Step 1: Write tests for collision helpers**

In `src/player/collision.rs`:

```rust
use bevy::prelude::*;

use crate::player::{Player, Velocity, Grounded, PLAYER_WIDTH, PLAYER_HEIGHT};
use crate::world::{TILE_SIZE, WORLD_WIDTH_TILES, WORLD_HEIGHT_TILES};
use crate::world::chunk::WorldMap;

/// Player AABB from center position.
pub struct Aabb {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

impl Aabb {
    pub fn from_center(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            min_x: x - w / 2.0,
            max_x: x + w / 2.0,
            min_y: y - h / 2.0,
            max_y: y + h / 2.0,
        }
    }

    /// Tile coordinates that this AABB overlaps.
    pub fn overlapping_tiles(&self) -> impl Iterator<Item = (i32, i32)> {
        let min_tx = (self.min_x / TILE_SIZE).floor() as i32;
        let max_tx = ((self.max_x - 0.001) / TILE_SIZE).floor() as i32;
        let min_ty = (self.min_y / TILE_SIZE).floor() as i32;
        let max_ty = ((self.max_y - 0.001) / TILE_SIZE).floor() as i32;

        let mut tiles = Vec::new();
        for ty in min_ty..=max_ty {
            for tx in min_tx..=max_tx {
                tiles.push((tx, ty));
            }
        }
        tiles.into_iter()
    }
}

/// Tile AABB from tile coordinates.
pub fn tile_aabb(tx: i32, ty: i32) -> Aabb {
    Aabb {
        min_x: tx as f32 * TILE_SIZE,
        max_x: (tx + 1) as f32 * TILE_SIZE,
        min_y: ty as f32 * TILE_SIZE,
        max_y: (ty + 1) as f32 * TILE_SIZE,
    }
}

pub fn collision_system(
    time: Res<Time>,
    mut world_map: ResMut<WorldMap>,
    mut query: Query<(&mut Transform, &mut Velocity, &mut Grounded), With<Player>>,
) {
    let dt = time.delta_secs();

    for (mut transform, mut vel, mut grounded) in &mut query {
        let pos = &mut transform.translation;

        // --- Resolve X axis ---
        pos.x += vel.x * dt;
        let aabb = Aabb::from_center(pos.x, pos.y, PLAYER_WIDTH, PLAYER_HEIGHT);
        for (tx, ty) in aabb.overlapping_tiles() {
            if world_map.is_solid(tx, ty) {
                let tile = tile_aabb(tx, ty);
                // Check if actually overlapping
                let player = Aabb::from_center(pos.x, pos.y, PLAYER_WIDTH, PLAYER_HEIGHT);
                if player.max_x > tile.min_x && player.min_x < tile.max_x
                    && player.max_y > tile.min_y && player.min_y < tile.max_y
                {
                    if vel.x > 0.0 {
                        pos.x = tile.min_x - PLAYER_WIDTH / 2.0;
                    } else if vel.x < 0.0 {
                        pos.x = tile.max_x + PLAYER_WIDTH / 2.0;
                    }
                    vel.x = 0.0;
                }
            }
        }

        // --- Resolve Y axis ---
        pos.y += vel.y * dt;
        grounded.0 = false;
        let aabb = Aabb::from_center(pos.x, pos.y, PLAYER_WIDTH, PLAYER_HEIGHT);
        for (tx, ty) in aabb.overlapping_tiles() {
            if world_map.is_solid(tx, ty) {
                let tile = tile_aabb(tx, ty);
                let player = Aabb::from_center(pos.x, pos.y, PLAYER_WIDTH, PLAYER_HEIGHT);
                if player.max_x > tile.min_x && player.min_x < tile.max_x
                    && player.max_y > tile.min_y && player.min_y < tile.max_y
                {
                    if vel.y < 0.0 {
                        pos.y = tile.max_y + PLAYER_HEIGHT / 2.0;
                        grounded.0 = true;
                    } else if vel.y > 0.0 {
                        pos.y = tile.min_y - PLAYER_HEIGHT / 2.0;
                    }
                    vel.y = 0.0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aabb_from_center() {
        let aabb = Aabb::from_center(100.0, 200.0, 24.0, 48.0);
        assert_eq!(aabb.min_x, 88.0);
        assert_eq!(aabb.max_x, 112.0);
        assert_eq!(aabb.min_y, 176.0);
        assert_eq!(aabb.max_y, 224.0);
    }

    #[test]
    fn overlapping_tiles_single() {
        // Player centered in the middle of tile (3, 3)
        let center_x = 3.0 * TILE_SIZE + TILE_SIZE / 2.0;
        let center_y = 3.0 * TILE_SIZE + TILE_SIZE / 2.0;
        let aabb = Aabb::from_center(center_x, center_y, 20.0, 20.0);
        let tiles: Vec<_> = aabb.overlapping_tiles().collect();
        assert_eq!(tiles, vec![(3, 3)]);
    }

    #[test]
    fn overlapping_tiles_multiple() {
        // Player on boundary between tiles
        let aabb = Aabb::from_center(32.0, 32.0, 24.0, 48.0);
        let tiles: Vec<_> = aabb.overlapping_tiles().collect();
        // Should overlap 4 tiles around the boundary
        assert!(tiles.len() >= 2);
        assert!(tiles.contains(&(0, 0)));
        assert!(tiles.contains(&(1, 0)) || tiles.contains(&(0, 1)));
    }

    #[test]
    fn tile_aabb_basic() {
        let aabb = tile_aabb(3, 5);
        assert_eq!(aabb.min_x, 96.0);
        assert_eq!(aabb.max_x, 128.0);
        assert_eq!(aabb.min_y, 160.0);
        assert_eq!(aabb.max_y, 192.0);
    }
}
```

**Step 2: Update PlayerPlugin to use collision instead of simple velocity**

In `src/player/mod.rs`, replace the system ordering:

```rust
impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player)
            .add_systems(Update, (
                movement::player_input,
                movement::apply_gravity,
                collision::collision_system,
            ).chain());
    }
}
```

Remove `apply_velocity` from movement.rs (collision_system now handles position updates).

In `src/player/movement.rs`, remove the `apply_velocity` function (collision_system integrates velocity).

**Step 3: Run tests**

Run: `cargo test -- collision`
Expected: all 4 tests pass.

**Step 4: Verify in game**

Run: `cargo run`
Expected: player stands on the terrain surface, can walk left/right, can jump.

**Step 5: Commit**

```bash
git add -A && git commit -m "feat: AABB tile collision with grounded detection"
```

---

## Task 10: Block Interaction

**Files:**
- Create: `src/interaction/mod.rs`
- Create: `src/interaction/block_action.rs`
- Modify: `src/main.rs`

**Step 1: Implement block interaction system**

In `src/interaction/mod.rs`:

```rust
pub mod block_action;

use bevy::prelude::*;

pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, block_action::block_interaction_system);
    }
}
```

In `src/interaction/block_action.rs`:

```rust
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_ecs_tilemap::prelude::*;

use crate::player::{Player, PLAYER_WIDTH, PLAYER_HEIGHT};
use crate::world::TILE_SIZE;
use crate::world::tile::TileType;
use crate::world::chunk::{
    WorldMap, LoadedChunks, TilemapTextureHandle, ChunkCoord,
    world_to_tile, tile_to_chunk, tile_to_local,
};

const BLOCK_REACH: f32 = 5.0; // tiles

pub fn block_interaction_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    player_query: Query<&Transform, With<Player>>,
    mut world_map: ResMut<WorldMap>,
    loaded_chunks: Res<LoadedChunks>,
    texture_handle: Res<TilemapTextureHandle>,
    mut tilemap_query: Query<(&ChunkCoord, &mut TileStorage, Entity)>,
    tile_color_query: Query<Entity, With<TileColor>>,
) {
    let left_click = mouse.just_pressed(MouseButton::Left);
    let right_click = mouse.just_pressed(MouseButton::Right);
    if !left_click && !right_click {
        return;
    }

    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_gt)) = camera_query.single() else { return };
    let Ok(player_tf) = player_query.single() else { return };

    let Some(cursor_pos) = window.cursor_position() else { return };
    let Ok(world_pos) = camera.viewport_to_world_2d(camera_gt, cursor_pos) else { return };

    let (tile_x, tile_y) = world_to_tile(world_pos.x, world_pos.y);

    // Range check
    let player_tile_x = (player_tf.translation.x / TILE_SIZE).floor();
    let player_tile_y = (player_tf.translation.y / TILE_SIZE).floor();
    let dx = (tile_x as f32 - player_tile_x).abs();
    let dy = (tile_y as f32 - player_tile_y).abs();
    if dx > BLOCK_REACH || dy > BLOCK_REACH {
        return;
    }

    let (chunk_x, chunk_y) = tile_to_chunk(tile_x, tile_y);
    let (local_x, local_y) = tile_to_local(tile_x, tile_y);
    let tile_pos = TilePos::new(local_x, local_y);

    if left_click {
        // Break block
        let current = world_map.get_tile(tile_x, tile_y);
        if !current.is_solid() {
            return;
        }

        world_map.set_tile(tile_x, tile_y, TileType::Air);

        // Update ECS tilemap
        if let Some(&chunk_entity) = loaded_chunks.map.get(&(chunk_x, chunk_y)) {
            for (coord, mut storage, entity) in &mut tilemap_query {
                if coord.x == chunk_x && coord.y == chunk_y {
                    if let Some(tile_entity) = storage.remove(&tile_pos) {
                        commands.entity(tile_entity).despawn();
                    }
                    break;
                }
            }
        }
    } else if right_click {
        // Place block
        let current = world_map.get_tile(tile_x, tile_y);
        if current.is_solid() {
            return; // already solid
        }

        // Check player overlap: can't place where player is standing
        let half_w = PLAYER_WIDTH / 2.0;
        let half_h = PLAYER_HEIGHT / 2.0;
        let player_min_x = player_tf.translation.x - half_w;
        let player_max_x = player_tf.translation.x + half_w;
        let player_min_y = player_tf.translation.y - half_h;
        let player_max_y = player_tf.translation.y + half_h;
        let tile_min_x = tile_x as f32 * TILE_SIZE;
        let tile_max_x = tile_min_x + TILE_SIZE;
        let tile_min_y = tile_y as f32 * TILE_SIZE;
        let tile_max_y = tile_min_y + TILE_SIZE;
        if player_max_x > tile_min_x && player_min_x < tile_max_x
            && player_max_y > tile_min_y && player_min_y < tile_max_y
        {
            return; // overlaps player
        }

        let place_type = TileType::Dirt;
        world_map.set_tile(tile_x, tile_y, place_type);

        // Update ECS tilemap
        if let Some(&chunk_entity) = loaded_chunks.map.get(&(chunk_x, chunk_y)) {
            for (coord, mut storage, entity) in &mut tilemap_query {
                if coord.x == chunk_x && coord.y == chunk_y {
                    let tilemap_id = TilemapId(entity);
                    let color = place_type.color().unwrap();
                    let tile_entity = commands.spawn(TileBundle {
                        position: tile_pos,
                        tilemap_id,
                        texture_index: TileTextureIndex(0),
                        color: TileColor(color),
                        ..Default::default()
                    }).id();
                    commands.entity(entity).add_child(tile_entity);
                    storage.set(&tile_pos, tile_entity);
                    break;
                }
            }
        }
    }
}
```

**Step 2: Wire InteractionPlugin into main.rs**

```rust
mod world;
mod player;
mod camera;
mod interaction;

// In main():
    .add_plugins(world::WorldPlugin)
    .add_plugins(player::PlayerPlugin)
    .add_plugins(camera::CameraPlugin)
    .add_plugins(interaction::InteractionPlugin)
```

**Step 3: Verify**

Run: `cargo run`
Expected:
- LMB on a solid tile within range: tile disappears (becomes Air)
- RMB on an Air tile within range: brown Dirt tile appears
- Can't break Air, can't place on solid, can't place where standing
- Chunks load/unload as player moves

**Step 4: Commit**

```bash
git add -A && git commit -m "feat: block break (LMB) and place (RMB) interaction"
```

---

## Final Verification

Run the complete test suite:

```bash
cargo test
```

Expected: all tests pass (tile, terrain_gen, chunk, collision).

Run the game and verify all features:

```bash
cargo run
```

Checklist:
- [ ] Window opens at 1280×720 titled "Starbeam"
- [ ] Terrain generates with grass surface, dirt, stone, caves
- [ ] Player (blue rectangle) spawns on the surface
- [ ] A/D or arrows move player left/right
- [ ] Space jumps (only when grounded)
- [ ] Player collides with terrain (doesn't fall through)
- [ ] Camera follows player, doesn't go outside world
- [ ] Chunks load/unload as player moves (no pop-in at screen edges due to buffer)
- [ ] LMB breaks solid blocks within range
- [ ] RMB places Dirt blocks on empty space within range
- [ ] Can't place blocks where player is standing

```bash
git add -A && git commit -m "feat: minimum planet prototype complete"
```
