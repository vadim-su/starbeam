# Horizontal Wrap-Around Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the world horizontally wrap-around — walking past the right edge seamlessly brings you to the left edge and vice versa, with no visible seam in terrain.

**Architecture:** All tile X coordinates normalized via `wrap_tile_x(x) = x.rem_euclid(WORLD_WIDTH_TILES)` at data layer. Terrain uses cylindrical Perlin noise for seamless join. Chunk loading separates display position from data coords. Player teleports at world edge.

**Tech Stack:** Bevy 0.18.0, bevy_ecs_tilemap 0.18, noise 0.9 (Perlin 2D/3D)

---

### Task 1: `wrap_tile_x` and `wrap_chunk_x` helpers

**Files:**
- Modify: `src/world/mod.rs` — add helper functions + tests

**Step 1: Add helper functions**

Add after the constant declarations (after line 26, before `pub struct WorldPlugin`):

```rust
/// Wrap tile X coordinate for horizontal wrap-around.
pub fn wrap_tile_x(tile_x: i32) -> i32 {
    tile_x.rem_euclid(WORLD_WIDTH_TILES)
}

/// Wrap chunk X coordinate for horizontal wrap-around.
pub fn wrap_chunk_x(chunk_x: i32) -> i32 {
    chunk_x.rem_euclid(WORLD_WIDTH_CHUNKS)
}
```

**Step 2: Add tests**

Create a `#[cfg(test)] mod tests` block at the end of `src/world/mod.rs`:

```rust
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
```

**Step 3: Run tests**

Run: `cargo test --lib world::tests`
Expected: All 6 new tests PASS

**Step 4: Commit**

```bash
git add src/world/mod.rs
git commit -m "feat: add wrap_tile_x and wrap_chunk_x helpers for horizontal wrap-around"
```

---

### Task 2: Cylindrical noise terrain generation

**Files:**
- Modify: `src/world/terrain_gen.rs` — cylindrical Perlin noise + update tests

**Step 1: Update surface_height to use cylindrical noise**

Replace `surface_height` function:

```rust
/// Get the surface height (in tile Y) at a given tile X coordinate.
/// Uses cylindrical noise sampling for seamless horizontal wrap-around.
pub fn surface_height(seed: u32, tile_x: i32) -> i32 {
    let perlin = Perlin::new(seed);
    let base = SURFACE_BASE * WORLD_HEIGHT_TILES as f64;

    let angle = tile_x as f64 / WORLD_WIDTH_TILES as f64 * 2.0 * std::f64::consts::PI;
    let radius = WORLD_WIDTH_TILES as f64 * SURFACE_FREQUENCY / (2.0 * std::f64::consts::PI);
    let nx = radius * angle.cos();
    let ny = radius * angle.sin();
    let noise_val = perlin.get([nx, ny]);

    (base + noise_val * SURFACE_AMPLITUDE) as i32
}
```

**Step 2: Update generate_tile — wrap X, cylindrical cave noise (3D)**

Replace `generate_tile` function:

```rust
/// Generate tile type at an absolute tile position.
/// X coordinate wraps horizontally. Y is bounded [0, WORLD_HEIGHT_TILES).
pub fn generate_tile(seed: u32, tile_x: i32, tile_y: i32) -> TileType {
    if tile_y < 0 || tile_y >= WORLD_HEIGHT_TILES {
        return TileType::Air;
    }

    // Wrap X for cylindrical world
    let tile_x = crate::world::wrap_tile_x(tile_x);

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

    // Below dirt layer: stone with caves (cylindrical 3D noise)
    let cave_perlin = Perlin::new(seed.wrapping_add(1));
    let angle = tile_x as f64 / WORLD_WIDTH_TILES as f64 * 2.0 * std::f64::consts::PI;
    let radius = WORLD_WIDTH_TILES as f64 * CAVE_FREQUENCY / (2.0 * std::f64::consts::PI);
    let cave_val = cave_perlin.get([
        radius * angle.cos(),
        radius * angle.sin(),
        tile_y as f64 * CAVE_FREQUENCY,
    ]);
    if cave_val.abs() < CAVE_THRESHOLD {
        TileType::Air // cave
    } else {
        TileType::Stone
    }
}
```

**Step 3: Update tests**

Replace the `out_of_bounds_is_air` test and add wrap-around test:

```rust
#[test]
fn out_of_bounds_y_is_air() {
    assert_eq!(generate_tile(TEST_SEED, 500, -1), TileType::Air);
    assert_eq!(generate_tile(TEST_SEED, 500, WORLD_HEIGHT_TILES), TileType::Air);
}

#[test]
fn x_wraps_around() {
    // tile_x = -1 should equal tile_x = WORLD_WIDTH_TILES - 1
    let t1 = generate_tile(TEST_SEED, -1, 500);
    let t2 = generate_tile(TEST_SEED, WORLD_WIDTH_TILES - 1, 500);
    assert_eq!(t1, t2);

    // tile_x = WORLD_WIDTH_TILES should equal tile_x = 0
    let t3 = generate_tile(TEST_SEED, WORLD_WIDTH_TILES, 500);
    let t4 = generate_tile(TEST_SEED, 0, 500);
    assert_eq!(t3, t4);
}

#[test]
fn surface_height_wraps_seamlessly() {
    // Surface at x=0 should equal surface at x=WORLD_WIDTH_TILES (wrapped)
    let h0 = surface_height(TEST_SEED, 0);
    let h_wrap = surface_height(TEST_SEED, WORLD_WIDTH_TILES);
    assert_eq!(h0, h_wrap);

    // Also check negative
    let h_neg = surface_height(TEST_SEED, -1);
    let h_pos = surface_height(TEST_SEED, WORLD_WIDTH_TILES - 1);
    assert_eq!(h_neg, h_pos);
}
```

Remove the old `out_of_bounds_is_air` test (it tested X bounds returning Air, which no longer applies).

**Step 4: Run tests**

Run: `cargo test --lib world::terrain_gen::tests`
Expected: All terrain tests PASS (some existing tests may produce different values due to noise change — `surface_height_is_deterministic`, `surface_height_is_within_bounds`, `above_surface_is_air`, `surface_is_grass`, `below_surface_is_dirt_then_stone` should still pass since they test structural properties, not exact values).

**Step 5: Commit**

```bash
git add src/world/terrain_gen.rs
git commit -m "feat: cylindrical Perlin noise for seamless horizontal terrain wrap"
```

---

### Task 3: WorldMap wrap X in get_tile / set_tile

**Files:**
- Modify: `src/world/chunk.rs` — wrap X in WorldMap methods, update tests

**Step 1: Update WorldMap::get_tile**

Replace the function:

```rust
/// Get tile type at absolute tile coordinates.
/// X wraps horizontally. Y is bounded (below=Stone, above=Air).
pub fn get_tile(&mut self, tile_x: i32, tile_y: i32) -> TileType {
    if tile_y < 0 {
        return TileType::Stone; // bedrock
    }
    if tile_y >= WORLD_HEIGHT_TILES {
        return TileType::Air; // sky
    }
    let wrapped_x = crate::world::wrap_tile_x(tile_x);
    let (cx, cy) = tile_to_chunk(wrapped_x, tile_y);
    let (lx, ly) = tile_to_local(wrapped_x, tile_y);
    self.get_or_generate_chunk(cx, cy).get(lx, ly)
}
```

**Step 2: Update WorldMap::set_tile**

Replace the function:

```rust
/// Set tile type at absolute tile coordinates.
/// X wraps horizontally. Y out of bounds is ignored.
pub fn set_tile(&mut self, tile_x: i32, tile_y: i32, tile: TileType) {
    if tile_y < 0 || tile_y >= WORLD_HEIGHT_TILES {
        return;
    }
    let wrapped_x = crate::world::wrap_tile_x(tile_x);
    let (cx, cy) = tile_to_chunk(wrapped_x, tile_y);
    let (lx, ly) = tile_to_local(wrapped_x, tile_y);
    self.get_or_generate_chunk(cx, cy);
    self.chunks.get_mut(&(cx, cy)).unwrap().set(lx, ly, tile);
}
```

**Step 3: Update tests**

Replace the `worldmap_out_of_bounds` test:

```rust
#[test]
fn worldmap_y_out_of_bounds() {
    let mut map = WorldMap::default();
    // Above world is Air
    assert_eq!(map.get_tile(0, WORLD_HEIGHT_TILES), TileType::Air);
    // Below world is Stone
    assert_eq!(map.get_tile(0, -1), TileType::Stone);
}

#[test]
fn worldmap_x_wraps() {
    let mut map = WorldMap::default();
    let t1 = map.get_tile(-1, 500);
    let t2 = map.get_tile(WORLD_WIDTH_TILES - 1, 500);
    assert_eq!(t1, t2);

    let t3 = map.get_tile(WORLD_WIDTH_TILES, 500);
    let t4 = map.get_tile(0, 500);
    assert_eq!(t3, t4);
}

#[test]
fn worldmap_set_tile_wraps() {
    let mut map = WorldMap::default();
    map.set_tile(-1, 500, TileType::Air);
    assert_eq!(map.get_tile(WORLD_WIDTH_TILES - 1, 500), TileType::Air);
}
```

Remove the old `worldmap_out_of_bounds` test.

**Step 4: Run tests**

Run: `cargo test --lib world::chunk::tests`
Expected: All chunk tests PASS

**Step 5: Commit**

```bash
git add src/world/chunk.rs
git commit -m "feat: WorldMap wraps X coordinate horizontally"
```

---

### Task 4: Chunk loading with display position wrapping

**Files:**
- Modify: `src/world/chunk.rs` — update `spawn_chunk` and `chunk_loading_system`

**Step 1: Update `spawn_chunk` to accept display coords**

Replace `spawn_chunk`:

```rust
pub fn spawn_chunk(
    commands: &mut Commands,
    world_map: &mut WorldMap,
    loaded_chunks: &mut LoadedChunks,
    texture_handle: &Handle<Image>,
    display_chunk_x: i32,
    chunk_y: i32,
) {
    if loaded_chunks.map.contains_key(&(display_chunk_x, chunk_y)) {
        return; // already loaded at this display position
    }

    // Wrap X for data access
    let data_chunk_x = crate::world::wrap_chunk_x(display_chunk_x);
    let chunk_data = world_map.get_or_generate_chunk(data_chunk_x, chunk_y);
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

    // Display position uses display_chunk_x (may be outside [0, WORLD_WIDTH_CHUNKS))
    let display_position = Vec3::new(
        display_chunk_x as f32 * CHUNK_SIZE as f32 * TILE_SIZE,
        chunk_y as f32 * CHUNK_SIZE as f32 * TILE_SIZE,
        0.0,
    );

    let grid_size: TilemapGridSize = tile_size.into();
    commands.entity(tilemap_entity).insert((
        TilemapBundle {
            grid_size,
            map_type: TilemapType::Square,
            size: tilemap_size,
            storage: tile_storage,
            texture: TilemapTexture::Single(texture_handle.clone()),
            tile_size,
            transform: Transform::from_translation(display_position),
            render_settings: TilemapRenderSettings {
                render_chunk_size: UVec2::new(CHUNK_SIZE, CHUNK_SIZE),
                y_sort: false,
            },
            anchor: TilemapAnchor::BottomLeft,
            ..Default::default()
        },
        ChunkCoord {
            x: display_chunk_x,
            y: chunk_y,
        },
    ));

    loaded_chunks
        .map
        .insert((display_chunk_x, chunk_y), tilemap_entity);
}
```

**Step 2: Update `chunk_loading_system` — remove X bounds check**

Replace `chunk_loading_system`:

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

    // Determine which chunks should be loaded (display coords, may be outside [0, WORLD_WIDTH_CHUNKS))
    let mut desired: HashSet<(i32, i32)> = HashSet::new();
    let load_radius = crate::world::CHUNK_LOAD_RADIUS;
    for display_cx in (cam_chunk_x - load_radius)..=(cam_chunk_x + load_radius) {
        for cy in (cam_chunk_y - load_radius)..=(cam_chunk_y + load_radius) {
            // Y is still bounded, X wraps (handled in spawn_chunk)
            if cy >= 0 && cy < crate::world::WORLD_HEIGHT_CHUNKS {
                desired.insert((display_cx, cy));
            }
        }
    }

    // Spawn missing chunks
    for &(display_cx, cy) in &desired {
        if !loaded_chunks.map.contains_key(&(display_cx, cy)) {
            spawn_chunk(
                &mut commands,
                &mut world_map,
                &mut loaded_chunks,
                &texture_handle.0,
                display_cx,
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

**Step 3: Run tests + compile check**

Run: `cargo test --lib`
Expected: All tests PASS

Run: `cargo build`
Expected: Compiles cleanly

**Step 4: Commit**

```bash
git add src/world/chunk.rs
git commit -m "feat: chunk loading with display position wrapping for horizontal wrap-around"
```

---

### Task 5: Player wrap system

**Files:**
- Create: `src/player/wrap.rs`
- Modify: `src/player/mod.rs` — register new system

**Step 1: Create `src/player/wrap.rs`**

```rust
use bevy::prelude::*;

use crate::player::Player;
use crate::world::{TILE_SIZE, WORLD_WIDTH_TILES};

/// Teleport player when they cross the horizontal world boundary.
pub fn player_wrap_system(mut query: Query<&mut Transform, With<Player>>) {
    let world_w = WORLD_WIDTH_TILES as f32 * TILE_SIZE;
    for mut transform in &mut query {
        let pos = &mut transform.translation;
        if pos.x < 0.0 {
            pos.x += world_w;
        } else if pos.x >= world_w {
            pos.x -= world_w;
        }
    }
}
```

**Step 2: Register module and system in `src/player/mod.rs`**

Add module declaration at top:

```rust
pub mod wrap;
```

Add `wrap::player_wrap_system` to the system chain, **after** `collision::collision_system`:

```rust
app.add_systems(Startup, spawn_player).add_systems(
    Update,
    (
        movement::player_input,
        movement::apply_gravity,
        collision::collision_system,
        wrap::player_wrap_system,
    )
        .chain(),
);
```

**Step 3: Run tests + compile**

Run: `cargo test --lib`
Expected: All tests PASS

Run: `cargo build`
Expected: Compiles cleanly

**Step 4: Commit**

```bash
git add src/player/wrap.rs src/player/mod.rs
git commit -m "feat: player wrap system — teleport at horizontal world edge"
```

---

### Task 6: Camera follow — remove horizontal clamp

**Files:**
- Modify: `src/camera/follow.rs`

**Step 1: Remove X clamping, keep Y clamping**

Replace `camera_follow_player`:

```rust
pub fn camera_follow_player(
    player_query: Query<&Transform, (With<Player>, Without<Camera2d>)>,
    mut camera_query: Query<(&mut Transform, &Projection), (With<Camera2d>, Without<Player>)>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let Ok((mut camera_transform, projection)) = camera_query.single_mut() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

    let proj_scale = match projection {
        Projection::Orthographic(ortho) => ortho.scale,
        _ => 1.0,
    };
    let half_h = window.height() / 2.0 * proj_scale;
    let world_h = WORLD_HEIGHT_TILES as f32 * TILE_SIZE;

    let mut target = player_transform.translation;

    // X follows player freely (world wraps horizontally)
    // Y is still clamped to world bounds
    target.y = target.y.clamp(half_h, (world_h - half_h).max(half_h));

    camera_transform.translation.x = target.x;
    camera_transform.translation.y = target.y;
}
```

Remove the unused import `WORLD_WIDTH_TILES` from the use statement (only `TILE_SIZE` and `WORLD_HEIGHT_TILES` are needed now).

**Step 2: Run tests + compile**

Run: `cargo test --lib`
Expected: All tests PASS

Run: `cargo build`
Expected: Compiles cleanly

**Step 3: Commit**

```bash
git add src/camera/follow.rs
git commit -m "feat: camera follows player freely on X axis for horizontal wrap-around"
```

---

### Task 7: Block interaction — wrap-aware reach and chunk lookup

**Files:**
- Modify: `src/interaction/block_action.rs`

**Step 1: Add wrap-aware reach distance**

In `block_interaction_system`, replace the reach check section (around lines 48-54):

```rust
    // Range check (wrap-aware on X axis)
    let player_tile_x = (player_tf.translation.x / TILE_SIZE).floor();
    let player_tile_y = (player_tf.translation.y / TILE_SIZE).floor();
    let raw_dx = (tile_x as f32 - player_tile_x).abs();
    let dx = raw_dx.min(WORLD_WIDTH_TILES as f32 - raw_dx); // shortest distance on ring
    let dy = (tile_y as f32 - player_tile_y).abs();
    if dx > BLOCK_REACH || dy > BLOCK_REACH {
        return;
    }
```

Add import at the top of the file:

```rust
use crate::world::WORLD_WIDTH_TILES;
```

**Step 2: Update chunk coord lookup for display position**

The `tile_to_chunk(tile_x, tile_y)` call currently uses the raw (possibly unwrapped) tile_x from cursor world position. This gives the **display** chunk coordinate, which matches the ChunkCoord stored on tilemap entities. This is correct behavior — no change needed for the chunk lookup.

The `world_map.set_tile(tile_x, tile_y, ...)` and `world_map.get_tile(tile_x, tile_y)` calls now wrap internally (Task 3), so they work correctly with any tile_x. No change needed.

**Step 3: Run tests + compile**

Run: `cargo test --lib`
Expected: All tests PASS

Run: `cargo build`
Expected: Compiles cleanly

**Step 4: Commit**

```bash
git add src/interaction/block_action.rs
git commit -m "feat: wrap-aware block reach distance for horizontal wrap-around"
```

---

## Summary

7 tasks total, each focused on one system:

| Task | What | Files |
|------|------|-------|
| 1 | `wrap_tile_x` / `wrap_chunk_x` helpers | `src/world/mod.rs` |
| 2 | Cylindrical noise terrain gen | `src/world/terrain_gen.rs` |
| 3 | WorldMap wraps X in get/set | `src/world/chunk.rs` |
| 4 | Chunk loading with display coords | `src/world/chunk.rs` |
| 5 | Player wrap system | `src/player/wrap.rs`, `src/player/mod.rs` |
| 6 | Camera — remove X clamp | `src/camera/follow.rs` |
| 7 | Block interaction wrap reach | `src/interaction/block_action.rs` |

After all tasks: run `cargo test` (all pass) then `cargo run` and verify:
- Walk right past world edge → seamlessly appear on left
- Walk left past world edge → seamlessly appear on right  
- Terrain has no visible seam at the boundary
- Can break/place blocks near boundary
- Camera shows chunks from the other side at the edge
