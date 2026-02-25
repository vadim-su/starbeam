---
source: Context7 API + docs.rs source
library: bevy_ecs_tilemap
package: bevy_ecs_tilemap
version: 0.18.1 (compatible with Bevy 0.18.x)
topic: Chunk-based sandbox patterns - dynamic spawn/despawn, tile modification
fetched: 2026-02-25T12:00:00Z
official_docs: https://docs.rs/bevy_ecs_tilemap/0.18.1/bevy_ecs_tilemap/
---

# Chunk-Based 2D Sandbox Patterns with bevy_ecs_tilemap 0.18.1

## Architecture Overview

bevy_ecs_tilemap uses an **entity-per-tile** approach:
- Each tile is a Bevy entity with `TileBundle` components
- Tiles are children of the tilemap entity
- The tilemap entity holds `TilemapBundle` with `TileStorage` for O(1) tile lookup
- The renderer groups tiles into **render chunks** (meshes) automatically via `TilemapRenderSettings.render_chunk_size`

**IMPORTANT**: The "chunks" in `TilemapRenderSettings` are **render chunks** (how tiles are batched into meshes), NOT game-logic chunks. For a sandbox game, you manage your own game-logic chunks (32x32 tile regions) as separate tilemap entities.

---

## Strategy: One Tilemap Entity Per Game Chunk

For a sandbox with dynamic chunk loading, create **one tilemap entity per 32x32 chunk**:

```rust
use bevy::prelude::*;
use bevy_ecs_tilemap::prelude::*;

const CHUNK_SIZE: u32 = 32;
const TILE_SIZE: f32 = 16.0;

/// Marker component for your game chunks
#[derive(Component)]
struct GameChunk {
    chunk_x: i32,
    chunk_y: i32,
}

/// Spawn a single chunk at chunk coordinates
fn spawn_chunk(
    commands: &mut Commands,
    chunk_x: i32,
    chunk_y: i32,
    // texture_handle: Handle<Image>,  // pass your texture atlas handle
) {
    let tilemap_size = TilemapSize::new(CHUNK_SIZE, CHUNK_SIZE);
    let mut tile_storage = TileStorage::empty(tilemap_size);

    // Calculate world position for this chunk
    let chunk_world_x = chunk_x as f32 * CHUNK_SIZE as f32 * TILE_SIZE;
    let chunk_world_y = chunk_y as f32 * CHUNK_SIZE as f32 * TILE_SIZE;

    let tilemap_entity = commands.spawn_empty().id();
    let tilemap_id = TilemapId(tilemap_entity);

    // Spawn tile entities as children
    commands.entity(tilemap_entity).with_children(|parent| {
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                let tile_pos = TilePos { x, y };
                let tile_entity = parent
                    .spawn(TileBundle {
                        position: tile_pos,
                        tilemap_id,
                        texture_index: TileTextureIndex(0), // your tile type
                        color: TileColor(Color::WHITE),
                        ..Default::default()
                    })
                    .id();
                tile_storage.set(&tile_pos, tile_entity);
            }
        }
    });

    // Insert the tilemap bundle onto the tilemap entity
    let tile_size = TilemapTileSize::new(TILE_SIZE, TILE_SIZE);
    let grid_size: TilemapGridSize = tile_size.into(); // auto-converts

    commands.entity(tilemap_entity).insert((
        TilemapBundle {
            grid_size,
            map_type: TilemapType::Square,
            size: tilemap_size,
            storage: tile_storage,
            texture: TilemapTexture::default(), // replace with your texture
            tile_size,
            transform: Transform::from_translation(Vec3::new(
                chunk_world_x,
                chunk_world_y,
                0.0,
            )),
            render_settings: TilemapRenderSettings {
                // Match render chunk to game chunk for best perf with dynamic tiles
                render_chunk_size: UVec2::new(CHUNK_SIZE, CHUNK_SIZE),
                y_sort: false,
            },
            ..Default::default()
        },
        GameChunk { chunk_x, chunk_y },
    ));
}
```

### Render Chunk Size Tuning

```rust
TilemapRenderSettings {
    // For a 32x32 game chunk, set render_chunk_size to 32x32
    // This means the entire chunk = 1 render mesh
    // Good when you modify tiles frequently (sandbox digging/placing)
    render_chunk_size: UVec2::new(32, 32),
    y_sort: false,
}

// Alternative: smaller render chunks for very frequent updates
TilemapRenderSettings {
    render_chunk_size: UVec2::new(16, 16), // 4 render meshes per 32x32 chunk
    y_sort: false,
}

// Default is 64x64 (CHUNK_SIZE_2D constant) - good for static maps
```

---

## Despawning a Chunk

```rust
fn despawn_chunk(
    commands: &mut Commands,
    chunk_query: &Query<(Entity, &GameChunk, &TileStorage)>,
    chunk_x: i32,
    chunk_y: i32,
) {
    for (entity, chunk, tile_storage) in chunk_query.iter() {
        if chunk.chunk_x == chunk_x && chunk.chunk_y == chunk_y {
            // Despawn all tile entities
            // NOTE: Since tiles are children, despawn_recursive handles this
            commands.entity(entity).despawn();
            break;
        }
    }
}
```

**Important**: When a `TileStorage` component is removed, the library's `on_remove_tilemap` observer fires and cleans up render state automatically.

---

## Dynamic Tile Modification (Break/Place Blocks)

### Removing a Tile (Breaking)

```rust
fn break_tile(
    commands: &mut Commands,
    tile_storage: &mut TileStorage,
    tile_pos: &TilePos,
) {
    // Remove from storage and get entity
    if let Some(tile_entity) = tile_storage.remove(tile_pos) {
        // Despawn the tile entity
        commands.entity(tile_entity).despawn();
    }
}
```

### Placing a Tile

```rust
fn place_tile(
    commands: &mut Commands,
    tilemap_entity: Entity,
    tile_storage: &mut TileStorage,
    tile_pos: TilePos,
    texture_index: u32,
) {
    // Only place if position is empty
    if tile_storage.get(&tile_pos).is_some() {
        return;
    }

    let tilemap_id = TilemapId(tilemap_entity);

    // Spawn new tile as child of tilemap
    let tile_entity = commands.entity(tilemap_entity).with_child(
        TileBundle {
            position: tile_pos,
            tilemap_id,
            texture_index: TileTextureIndex(texture_index),
            ..Default::default()
        }
    ).id();

    // Alternative approach - spawn then add as child:
    // let tile_entity = commands.spawn(TileBundle { ... }).id();
    // commands.entity(tilemap_entity).add_child(tile_entity);

    tile_storage.set(&tile_pos, tile_entity);
}
```

### Changing a Tile's Texture (e.g., damage states)

```rust
fn update_tile_texture(
    mut tile_query: Query<&mut TileTextureIndex>,
    tile_storage: &TileStorage,
    tile_pos: &TilePos,
    new_index: u32,
) {
    if let Some(tile_entity) = tile_storage.get(tile_pos) {
        if let Ok(mut texture_index) = tile_query.get_mut(tile_entity) {
            texture_index.0 = new_index;
        }
    }
}
```

### Changing a Tile's Color

```rust
fn update_tile_color(
    mut tile_query: Query<&mut TileColor>,
    tile_storage: &TileStorage,
    tile_pos: &TilePos,
    new_color: Color,
) {
    if let Some(tile_entity) = tile_storage.get(tile_pos) {
        if let Ok(mut color) = tile_query.get_mut(tile_entity) {
            color.0 = new_color;
        }
    }
}
```

### Hiding a Tile (without despawning)

```rust
fn hide_tile(
    mut tile_query: Query<&mut TileVisible>,
    tile_storage: &TileStorage,
    tile_pos: &TilePos,
) {
    if let Some(tile_entity) = tile_storage.get(tile_pos) {
        if let Ok(mut visible) = tile_query.get_mut(tile_entity) {
            visible.0 = false;
        }
    }
}
```

---

## Camera-Based Chunk Loading/Unloading

```rust
fn chunk_management_system(
    mut commands: Commands,
    camera_query: Query<&Transform, With<Camera2d>>,
    chunk_query: Query<(Entity, &GameChunk, &TileStorage)>,
    // ... your world data resource
) {
    let Ok(camera_transform) = camera_query.single() else { return };
    let camera_pos = camera_transform.translation.truncate();

    // Calculate which chunk the camera is in
    let current_chunk_x = (camera_pos.x / (CHUNK_SIZE as f32 * TILE_SIZE)).floor() as i32;
    let current_chunk_y = (camera_pos.y / (CHUNK_SIZE as f32 * TILE_SIZE)).floor() as i32;

    let load_radius = 3; // Load chunks within 3 chunk radius

    // Collect currently loaded chunks
    let loaded_chunks: HashSet<(i32, i32)> = chunk_query
        .iter()
        .map(|(_, chunk, _)| (chunk.chunk_x, chunk.chunk_y))
        .collect();

    // Spawn chunks that should be loaded
    for cx in (current_chunk_x - load_radius)..=(current_chunk_x + load_radius) {
        for cy in (current_chunk_y - load_radius)..=(current_chunk_y + load_radius) {
            if !loaded_chunks.contains(&(cx, cy)) {
                spawn_chunk(&mut commands, cx, cy);
            }
        }
    }

    // Despawn chunks that are too far
    let unload_radius = load_radius + 1;
    for (entity, chunk, _) in chunk_query.iter() {
        let dx = (chunk.chunk_x - current_chunk_x).abs();
        let dy = (chunk.chunk_y - current_chunk_y).abs();
        if dx > unload_radius || dy > unload_radius {
            commands.entity(entity).despawn();
        }
    }
}
```

---

## Helper Functions (from `bevy_ecs_tilemap::helpers::filling`)

### `fill_tilemap` - Fill entire tilemap with one texture

```rust
pub fn fill_tilemap(
    texture_index: TileTextureIndex,
    size: TilemapSize,
    tilemap_id: TilemapId,
    commands: &mut Commands,
    tile_storage: &mut TileStorage,
)
```

Spawns tile entities as children of `tilemap_id.0` for every position in `size`.

### `fill_tilemap_rect` - Fill a rectangular region

```rust
pub fn fill_tilemap_rect(
    texture_index: TileTextureIndex,
    origin: TilePos,
    size: TilemapSize,
    tilemap_id: TilemapId,
    commands: &mut Commands,
    tile_storage: &mut TileStorage,
)
```

---

## Color-Only Tiles (No Texture Atlas)

The `TilemapTexture` defaults to `Single(Handle::default())` which is an invalid/empty handle. For color-only tiles, you need a 1x1 white pixel texture:

```rust
fn setup_color_only_tilemap(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
) {
    // Create a 1x1 white pixel image
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
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
    let texture_handle = images.add(image);

    let tilemap_size = TilemapSize::new(32, 32);
    let mut tile_storage = TileStorage::empty(tilemap_size);
    let tilemap_entity = commands.spawn_empty().id();
    let tilemap_id = TilemapId(tilemap_entity);

    commands.entity(tilemap_entity).with_children(|parent| {
        for x in 0..32u32 {
            for y in 0..32u32 {
                let tile_pos = TilePos { x, y };
                let tile_entity = parent
                    .spawn(TileBundle {
                        position: tile_pos,
                        tilemap_id,
                        texture_index: TileTextureIndex(0),
                        color: TileColor(Color::srgb(
                            x as f32 / 32.0,
                            y as f32 / 32.0,
                            0.5,
                        )),
                        ..Default::default()
                    })
                    .id();
                tile_storage.set(&tile_pos, tile_entity);
            }
        }
    });

    let tile_size = TilemapTileSize::new(16.0, 16.0);
    commands.entity(tilemap_entity).insert(TilemapBundle {
        grid_size: tile_size.into(),
        map_type: TilemapType::Square,
        size: tilemap_size,
        storage: tile_storage,
        texture: TilemapTexture::Single(texture_handle),
        tile_size,
        ..Default::default()
    });
}
```

**Key insight**: `TileColor` tints the texture. With a white 1x1 texture, the tile renders as the exact color you set. Each tile can have a different `TileColor`.

---

## Sparse Tilemaps (Not All Positions Filled)

You don't have to fill every position. `TileStorage` starts with all `None`:

```rust
let mut tile_storage = TileStorage::empty(TilemapSize::new(32, 32));
// Only spawn tiles where needed - empty positions render nothing

// Check if a position has a tile:
if tile_storage.get(&TilePos::new(5, 10)).is_none() {
    // No tile here - it's "air"
}
```

---

## Draining All Tiles (Clearing a Chunk)

```rust
fn clear_chunk(
    commands: &mut Commands,
    tile_storage: &mut TileStorage,
) {
    for entity in tile_storage.drain() {
        commands.entity(entity).despawn();
    }
}
```

---

## Complete Minimal Example

```rust
use bevy::prelude::*;
use bevy_ecs_tilemap::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(TilemapPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
    commands.spawn(Camera2d);

    let texture_handle: Handle<Image> = asset_server.load("tiles.png");

    let tilemap_size = TilemapSize::new(32, 32);
    let mut tile_storage = TileStorage::empty(tilemap_size);
    let tilemap_entity = commands.spawn_empty().id();
    let tilemap_id = TilemapId(tilemap_entity);

    // Use the helper to fill the entire map
    fill_tilemap(
        TileTextureIndex(0),
        tilemap_size,
        tilemap_id,
        &mut commands,
        &mut tile_storage,
    );

    let tile_size = TilemapTileSize::new(16.0, 16.0);
    let grid_size: TilemapGridSize = tile_size.into();

    commands.entity(tilemap_entity).insert(TilemapBundle {
        grid_size,
        map_type: TilemapType::Square,
        size: tilemap_size,
        storage: tile_storage,
        texture: TilemapTexture::Single(texture_handle),
        tile_size,
        transform: Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
        ..Default::default()
    });
}
```
