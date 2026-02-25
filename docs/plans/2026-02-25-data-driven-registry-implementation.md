# Data-Driven Registry Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace all hardcoded game definitions with data-driven RON assets loaded via Bevy's AssetLoader, making tiles, player, and world config fully configurable.

**Architecture:** New `src/registry/` module with TileId, TileRegistry, PlayerConfig, WorldConfig types. Generic `RonLoader<T>` for loading RON files. AppState (Loading → InGame) gates gameplay systems. Existing modules migrate from constants/enums to Resource lookups.

**Tech Stack:** Bevy 0.18, serde + ron for deserialization, thiserror for loader errors.

**Design doc:** `docs/plans/2026-02-25-data-driven-registry-design.md`

---

### Task 1: Add dependencies and create RON data files

**Files:**
- Modify: `Cargo.toml`
- Create: `assets/data/tiles.registry.ron`
- Create: `assets/data/player.def.ron`
- Create: `assets/data/world.config.ron`

**Step 1: Add serde, ron, thiserror to Cargo.toml**

```toml
[dependencies]
bevy = "0.18.0"
bevy_ecs_tilemap = "0.18"
noise = "0.9"
serde = { version = "1", features = ["derive"] }
ron = "0.10"
thiserror = "2"
```

**Step 2: Create `assets/data/tiles.registry.ron`**

```ron
(
  tiles: [
    ( id: "air",   texture_index: None,    solid: false, hardness: 0.0, friction: 0.0, viscosity: 0.0 ),
    ( id: "grass", texture_index: Some(0), solid: true,  hardness: 1.0, friction: 0.8, viscosity: 0.0 ),
    ( id: "dirt",  texture_index: Some(1), solid: true,  hardness: 2.0, friction: 0.7, viscosity: 0.0 ),
    ( id: "stone", texture_index: Some(2), solid: true,  hardness: 5.0, friction: 0.6, viscosity: 0.0 ),
  ]
)
```

**Step 3: Create `assets/data/player.def.ron`**

```ron
(
  speed: 200.0,
  jump_velocity: 400.0,
  gravity: 800.0,
  width: 64.0,
  height: 128.0,
)
```

**Step 4: Create `assets/data/world.config.ron`**

```ron
(
  width_tiles: 2048,
  height_tiles: 1024,
  chunk_size: 32,
  tile_size: 32.0,
  chunk_load_radius: 3,
  seed: 42,
)
```

**Step 5: Verify build**

Run: `cargo build`
Expected: compiles (serde/ron/thiserror downloaded, no code uses them yet)

**Step 6: Commit**

```
git add -A && git commit -m "chore: add serde/ron/thiserror deps and RON data files"
```

---

### Task 2: Create core data types — TileId, TileDef, TileRegistry

**Files:**
- Create: `src/registry/mod.rs`
- Create: `src/registry/tile.rs`
- Modify: `src/main.rs` (add `mod registry;`)

**Step 1: Create `src/registry/mod.rs`**

```rust
pub mod tile;
```

**Step 2: Add `mod registry;` to `src/main.rs`**

Add `mod registry;` after the existing `mod` declarations.

**Step 3: Write tests for TileId and TileRegistry in `src/registry/tile.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry() -> TileRegistry {
        TileRegistry::from_defs(vec![
            TileDef { id: "air".into(),   texture_index: None,    solid: false, hardness: 0.0, friction: 0.0, viscosity: 0.0 },
            TileDef { id: "grass".into(), texture_index: Some(0), solid: true,  hardness: 1.0, friction: 0.8, viscosity: 0.0 },
            TileDef { id: "dirt".into(),  texture_index: Some(1), solid: true,  hardness: 2.0, friction: 0.7, viscosity: 0.0 },
            TileDef { id: "stone".into(), texture_index: Some(2), solid: true,  hardness: 5.0, friction: 0.6, viscosity: 0.0 },
        ])
    }

    #[test]
    fn air_is_always_id_zero() {
        let reg = test_registry();
        assert_eq!(reg.by_name("air"), TileId::AIR);
        assert_eq!(TileId::AIR, TileId(0));
    }

    #[test]
    fn lookup_by_name() {
        let reg = test_registry();
        assert_eq!(reg.by_name("grass"), TileId(1));
        assert_eq!(reg.by_name("dirt"), TileId(2));
        assert_eq!(reg.by_name("stone"), TileId(3));
    }

    #[test]
    fn solid_check() {
        let reg = test_registry();
        assert!(!reg.is_solid(TileId::AIR));
        assert!(reg.is_solid(TileId(1))); // grass
        assert!(reg.is_solid(TileId(3))); // stone
    }

    #[test]
    fn texture_index() {
        let reg = test_registry();
        assert_eq!(reg.texture_index(TileId::AIR), None);
        assert_eq!(reg.texture_index(TileId(1)), Some(0)); // grass
        assert_eq!(reg.texture_index(TileId(3)), Some(2)); // stone
    }

    #[test]
    fn get_returns_full_def() {
        let reg = test_registry();
        let stone = reg.get(TileId(3));
        assert_eq!(stone.id, "stone");
        assert_eq!(stone.hardness, 5.0);
        assert_eq!(stone.friction, 0.6);
    }

    #[test]
    #[should_panic]
    fn by_name_panics_on_unknown() {
        let reg = test_registry();
        reg.by_name("lava");
    }
}
```

**Step 4: Run tests to verify they fail**

Run: `cargo test registry::tile`
Expected: FAIL — module doesn't have types yet

**Step 5: Implement TileId, TileDef, TileRegistry in `src/registry/tile.rs`**

```rust
use std::collections::HashMap;

use bevy::prelude::*;
use serde::Deserialize;

/// Compact tile identifier. Index into TileRegistry.defs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TileId(pub u16);

impl TileId {
    pub const AIR: TileId = TileId(0);
}

/// Properties of a single tile type, deserialized from RON.
#[derive(Debug, Clone, Deserialize)]
pub struct TileDef {
    pub id: String,
    pub texture_index: Option<u32>,
    pub solid: bool,
    pub hardness: f32,
    pub friction: f32,
    pub viscosity: f32,
}

/// Registry of all tile definitions. Inserted as a Resource after asset loading.
#[derive(Resource)]
pub struct TileRegistry {
    pub defs: Vec<TileDef>,
    name_to_id: HashMap<String, TileId>,
}

impl TileRegistry {
    /// Build registry from a list of TileDefs. Order = TileId index.
    pub fn from_defs(defs: Vec<TileDef>) -> Self {
        let name_to_id = defs
            .iter()
            .enumerate()
            .map(|(i, d)| (d.id.clone(), TileId(i as u16)))
            .collect();
        Self { defs, name_to_id }
    }

    pub fn get(&self, id: TileId) -> &TileDef {
        &self.defs[id.0 as usize]
    }

    pub fn is_solid(&self, id: TileId) -> bool {
        self.defs[id.0 as usize].solid
    }

    pub fn texture_index(&self, id: TileId) -> Option<u32> {
        self.defs[id.0 as usize].texture_index
    }

    pub fn by_name(&self, name: &str) -> TileId {
        *self.name_to_id.get(name).unwrap_or_else(|| panic!("Unknown tile: {name}"))
    }
}
```

**Step 6: Run tests to verify they pass**

Run: `cargo test registry::tile`
Expected: all 6 tests PASS

**Step 7: Commit**

```
git add -A && git commit -m "feat: TileId, TileDef, TileRegistry core types with tests"
```

---

### Task 3: Create PlayerConfig and WorldConfig types

**Files:**
- Create: `src/registry/player.rs`
- Create: `src/registry/world.rs`
- Modify: `src/registry/mod.rs` (add modules)

**Step 1: Add modules to `src/registry/mod.rs`**

```rust
pub mod player;
pub mod tile;
pub mod world;
```

**Step 2: Write WorldConfig tests in `src/registry/world.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> WorldConfig {
        WorldConfig {
            width_tiles: 2048,
            height_tiles: 1024,
            chunk_size: 32,
            tile_size: 32.0,
            chunk_load_radius: 3,
            seed: 42,
        }
    }

    #[test]
    fn computed_chunk_dimensions() {
        let c = test_config();
        assert_eq!(c.width_chunks(), 64);
        assert_eq!(c.height_chunks(), 32);
    }

    #[test]
    fn wrap_tile_x_identity() {
        let c = test_config();
        assert_eq!(c.wrap_tile_x(0), 0);
        assert_eq!(c.wrap_tile_x(100), 100);
    }

    #[test]
    fn wrap_tile_x_overflow() {
        let c = test_config();
        assert_eq!(c.wrap_tile_x(2048), 0);
        assert_eq!(c.wrap_tile_x(2049), 1);
    }

    #[test]
    fn wrap_tile_x_negative() {
        let c = test_config();
        assert_eq!(c.wrap_tile_x(-1), 2047);
    }

    #[test]
    fn wrap_chunk_x_overflow() {
        let c = test_config();
        assert_eq!(c.wrap_chunk_x(64), 0);
        assert_eq!(c.wrap_chunk_x(-1), 63);
    }

    #[test]
    fn world_pixel_width() {
        let c = test_config();
        assert_eq!(c.world_pixel_width(), 2048.0 * 32.0);
    }
}
```

**Step 3: Run tests to verify they fail**

Run: `cargo test registry::world`
Expected: FAIL

**Step 4: Implement WorldConfig in `src/registry/world.rs`**

```rust
use bevy::prelude::*;
use serde::Deserialize;

/// World parameters loaded from RON.
#[derive(Resource, Debug, Clone, Deserialize)]
pub struct WorldConfig {
    pub width_tiles: i32,
    pub height_tiles: i32,
    pub chunk_size: u32,
    pub tile_size: f32,
    pub chunk_load_radius: i32,
    pub seed: u32,
}

impl WorldConfig {
    pub fn width_chunks(&self) -> i32 {
        self.width_tiles / self.chunk_size as i32
    }

    pub fn height_chunks(&self) -> i32 {
        self.height_tiles / self.chunk_size as i32
    }

    pub fn wrap_tile_x(&self, tile_x: i32) -> i32 {
        tile_x.rem_euclid(self.width_tiles)
    }

    pub fn wrap_chunk_x(&self, chunk_x: i32) -> i32 {
        chunk_x.rem_euclid(self.width_chunks())
    }

    pub fn world_pixel_width(&self) -> f32 {
        self.width_tiles as f32 * self.tile_size
    }

    pub fn world_pixel_height(&self) -> f32 {
        self.height_tiles as f32 * self.tile_size
    }
}
```

**Step 5: Implement PlayerConfig in `src/registry/player.rs`**

```rust
use bevy::prelude::*;
use serde::Deserialize;

/// Player parameters loaded from RON.
#[derive(Resource, Debug, Clone, Deserialize)]
pub struct PlayerConfig {
    pub speed: f32,
    pub jump_velocity: f32,
    pub gravity: f32,
    pub width: f32,
    pub height: f32,
}
```

**Step 6: Run tests**

Run: `cargo test registry::`
Expected: all registry tests PASS

**Step 7: Commit**

```
git add -A && git commit -m "feat: PlayerConfig and WorldConfig types with tests"
```

---

### Task 4: Generic RonLoader, Asset types, AppState, and loading plugin

**Files:**
- Create: `src/registry/loader.rs`
- Create: `src/registry/assets.rs`
- Modify: `src/registry/mod.rs` (add modules, RegistryPlugin)

**Step 1: Create Asset wrapper types in `src/registry/assets.rs`**

These are the intermediate Asset types that RON files deserialize into.

```rust
use bevy::prelude::*;
use bevy::reflect::TypePath;
use serde::Deserialize;

use super::tile::TileDef;

/// Asset loaded from tiles.registry.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct TileRegistryAsset {
    pub tiles: Vec<TileDef>,
}

/// Asset loaded from player.def.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct PlayerDefAsset {
    pub speed: f32,
    pub jump_velocity: f32,
    pub gravity: f32,
    pub width: f32,
    pub height: f32,
}

/// Asset loaded from world.config.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct WorldConfigAsset {
    pub width_tiles: i32,
    pub height_tiles: i32,
    pub chunk_size: u32,
    pub tile_size: f32,
    pub chunk_load_radius: i32,
    pub seed: u32,
}
```

**Step 2: Create generic RonLoader in `src/registry/loader.rs`**

```rust
use std::marker::PhantomData;

use bevy::asset::{io::Reader, AssetLoader, LoadContext};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RonLoaderError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("RON parse error: {0}")]
    Ron(#[from] ron::error::SpannedError),
}

pub struct RonLoader<T> {
    extensions: Vec<String>,
    _phantom: PhantomData<T>,
}

impl<T> RonLoader<T> {
    pub fn new(extensions: &[&str]) -> Self {
        Self {
            extensions: extensions.iter().map(|s| s.to_string()).collect(),
            _phantom: PhantomData,
        }
    }
}

impl<T> AssetLoader for RonLoader<T>
where
    T: Asset + TypePath + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    type Asset = T;
    type Settings = ();
    type Error = RonLoaderError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let asset = ron::de::from_bytes::<T>(&bytes)?;
        Ok(asset)
    }

    fn extensions(&self) -> &[&str] {
        // SAFETY: self lives as long as the returned slice
        // We need to return &[&str] but we store Vec<String>
        // Use a leaked ref approach or store &'static str
        // For simplicity, we'll use a different pattern
        &[]
    }
}
```

NOTE: The `extensions()` method in Bevy's AssetLoader returns `&[&str]`. Since we store `Vec<String>`, we have a lifetime issue. Two solutions:

**Option A:** Store `Vec<&'static str>` (requires leaked strings or static slices). Since our extensions are known at compile time, pass `&'static str` slices.

**Option B:** Make separate loader types per asset. Since we only have 3, this is simpler.

Go with **Option A** — change `extensions` field to `Vec<&'static str>`:

```rust
pub struct RonLoader<T> {
    extensions: Vec<&'static str>,
    _phantom: PhantomData<T>,
}

impl<T> RonLoader<T> {
    pub fn new(extensions: &[&'static str]) -> Self {
        Self {
            extensions: extensions.to_vec(),
            _phantom: PhantomData,
        }
    }
}

// In AssetLoader impl:
fn extensions(&self) -> &[&str] {
    &self.extensions
}
```

**Step 3: Create AppState and RegistryPlugin in `src/registry/mod.rs`**

```rust
pub mod assets;
pub mod loader;
pub mod player;
pub mod tile;
pub mod world;

use bevy::prelude::*;

use assets::{PlayerDefAsset, TileRegistryAsset, WorldConfigAsset};
use loader::RonLoader;
use player::PlayerConfig;
use tile::TileRegistry;
use world::WorldConfig;

/// Application state: Loading waits for assets, InGame runs gameplay.
#[derive(States, Default, Debug, Clone, Eq, PartialEq, Hash)]
pub enum AppState {
    #[default]
    Loading,
    InGame,
}

/// Handles for assets being loaded.
#[derive(Resource)]
struct LoadingAssets {
    tiles: Handle<TileRegistryAsset>,
    player: Handle<PlayerDefAsset>,
    world_config: Handle<WorldConfigAsset>,
}

pub struct RegistryPlugin;

impl Plugin for RegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .init_asset::<TileRegistryAsset>()
            .init_asset::<PlayerDefAsset>()
            .init_asset::<WorldConfigAsset>()
            .register_asset_loader(RonLoader::<TileRegistryAsset>::new(&["tiles.ron"]))
            .register_asset_loader(RonLoader::<PlayerDefAsset>::new(&["player.ron"]))
            .register_asset_loader(RonLoader::<WorldConfigAsset>::new(&["world.ron"]))
            .add_systems(Startup, start_loading)
            .add_systems(Update, check_loading.run_if(in_state(AppState::Loading)));
    }
}

fn start_loading(mut commands: Commands, asset_server: Res<AssetServer>) {
    let tiles = asset_server.load::<TileRegistryAsset>("data/tiles.registry.ron");
    let player = asset_server.load::<PlayerDefAsset>("data/player.def.ron");
    let world_config = asset_server.load::<WorldConfigAsset>("data/world.config.ron");
    commands.insert_resource(LoadingAssets { tiles, player, world_config });
}

fn check_loading(
    mut commands: Commands,
    loading: Res<LoadingAssets>,
    tile_assets: Res<Assets<TileRegistryAsset>>,
    player_assets: Res<Assets<PlayerDefAsset>>,
    world_assets: Res<Assets<WorldConfigAsset>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let (Some(tiles), Some(player), Some(world)) = (
        tile_assets.get(&loading.tiles),
        player_assets.get(&loading.player),
        world_assets.get(&loading.world_config),
    ) else {
        return; // not loaded yet
    };

    // Build resources from loaded assets
    commands.insert_resource(TileRegistry::from_defs(tiles.tiles.clone()));
    commands.insert_resource(PlayerConfig {
        speed: player.speed,
        jump_velocity: player.jump_velocity,
        gravity: player.gravity,
        width: player.width,
        height: player.height,
    });
    commands.insert_resource(WorldConfig {
        width_tiles: world.width_tiles,
        height_tiles: world.height_tiles,
        chunk_size: world.chunk_size,
        tile_size: world.tile_size,
        chunk_load_radius: world.chunk_load_radius,
        seed: world.seed,
    });

    commands.remove_resource::<LoadingAssets>();
    next_state.set(AppState::InGame);
    info!("All assets loaded, entering InGame state");
}
```

**Step 4: Register RegistryPlugin in `src/main.rs`**

Add `.add_plugins(registry::RegistryPlugin)` in the App builder **before** other game plugins.

**Step 5: Verify build**

Run: `cargo build`
Expected: compiles (new code exists but doesn't connect to old code yet)

**Step 6: Commit**

```
git add -A && git commit -m "feat: RonLoader, asset types, AppState, RegistryPlugin"
```

---

### Task 5: Migrate world module

**Files:**
- Modify: `src/world/tile.rs` — replace TileType enum with re-export of TileId
- Modify: `src/world/terrain_gen.rs` — use TileId + WorldConfig params
- Modify: `src/world/chunk.rs` — use TileId, TileRegistry, WorldConfig
- Modify: `src/world/mod.rs` — remove constants, use WorldConfig from resources

This is the largest task. Order matters: tile.rs first, then terrain_gen, then chunk, then mod.rs.

**Step 1: Replace `src/world/tile.rs`**

Delete the old TileType enum. Replace with a thin re-export module:

```rust
// Re-export TileId from registry for backward compat in world module
pub use crate::registry::tile::TileId;
```

**Step 2: Migrate `src/world/terrain_gen.rs`**

Changes:
- Replace `TileType` with `TileId`
- Functions now take `world_width: i32` and `world_height: i32` parameters (or a `&WorldConfig`) instead of using constants
- `surface_height()` takes `(seed, tile_x, world_width, world_height)` — needs world dimensions for cylindrical noise
- `generate_tile()` takes `(seed, tile_x, tile_y, world_width, world_height, tile_names: &TileNames)` where `TileNames` carries the TileIds
- Introduce a small struct to carry tile IDs needed by terrain gen:

```rust
use crate::registry::tile::TileId;
use crate::registry::world::WorldConfig;

/// Tile IDs used by terrain generation, looked up from TileRegistry at init.
pub struct TerrainTiles {
    pub air: TileId,
    pub grass: TileId,
    pub dirt: TileId,
    pub stone: TileId,
}
```

Updated function signatures:
```rust
pub fn surface_height(seed: u32, tile_x: i32, wc: &WorldConfig) -> i32
pub fn generate_tile(seed: u32, tile_x: i32, tile_y: i32, wc: &WorldConfig, tt: &TerrainTiles) -> TileId
pub fn generate_chunk_tiles(seed: u32, chunk_x: i32, chunk_y: i32, wc: &WorldConfig, tt: &TerrainTiles) -> Vec<TileId>
```

- Replace `WORLD_WIDTH_TILES` → `wc.width_tiles`
- Replace `WORLD_HEIGHT_TILES` → `wc.height_tiles`
- Replace `CHUNK_SIZE` → `wc.chunk_size`
- Replace `TileType::Air` → `tt.air`, `TileType::Grass` → `tt.grass`, etc.
- Replace `crate::world::wrap_tile_x(tile_x)` → `wc.wrap_tile_x(tile_x)`

Update tests: each test constructs a `WorldConfig` and `TerrainTiles` inline:

```rust
fn test_config() -> WorldConfig {
    WorldConfig { width_tiles: 2048, height_tiles: 1024, chunk_size: 32, tile_size: 32.0, chunk_load_radius: 3, seed: 42 }
}

fn test_tiles() -> TerrainTiles {
    TerrainTiles { air: TileId(0), grass: TileId(1), dirt: TileId(2), stone: TileId(3) }
}
```

**Step 3: Migrate `src/world/chunk.rs`**

Changes:
- `ChunkData.tiles: Vec<TileType>` → `Vec<TileId>` (returns `TileId` from `get()`)
- `WorldMap.seed` removed (comes from `WorldConfig`)
- `WorldMap` methods take `&WorldConfig` and `&TileRegistry` where needed:
  - `get_or_generate_chunk(&mut self, cx, cy, wc: &WorldConfig, tt: &TerrainTiles)` — passes wc+tt to terrain_gen
  - `get_tile(&mut self, tx, ty, wc: &WorldConfig, tt: &TerrainTiles) -> TileId`
  - `set_tile(&mut self, tx, ty, tile: TileId, wc: &WorldConfig, tt: &TerrainTiles)`
  - `is_solid(&mut self, tx, ty, wc: &WorldConfig, tt: &TerrainTiles, reg: &TileRegistry) -> bool`
- `TilemapTextureHandle` stays the same
- `spawn_chunk()` takes `&TileRegistry` + `&WorldConfig`:
  - Uses `registry.texture_index(tile_id)` instead of `tile_type.texture_index()`
  - Uses `wc.chunk_size`, `wc.tile_size` instead of `CHUNK_SIZE`, `TILE_SIZE`
- `chunk_loading_system()` reads `Res<WorldConfig>`, `Res<TileRegistry>`
- Remove `use crate::world::{CHUNK_SIZE, TILE_SIZE, ...}` constants

Also add `TerrainTiles` as a Resource (inserted during check_loading alongside TileRegistry):

```rust
#[derive(Resource)]
pub struct TerrainTiles { ... }
```

Move `TerrainTiles` to `src/registry/tile.rs` so it's accessible from both terrain_gen and the loading system.

Update tests: construct WorldConfig + TerrainTiles inline, pass to functions.

**Step 4: Migrate `src/world/mod.rs`**

Changes:
- Remove all `pub const` declarations (WORLD_WIDTH_TILES, WORLD_HEIGHT_TILES, CHUNK_SIZE, TILE_SIZE, WORLD_WIDTH_CHUNKS, WORLD_HEIGHT_CHUNKS, CHUNK_LOAD_RADIUS)
- Remove `wrap_tile_x()` and `wrap_chunk_x()` free functions (now on `WorldConfig`)
- Remove `create_tilemap_texture` / `load_tile_atlas` — move to WorldPlugin's systems that run `OnEnter(AppState::InGame)`
- WorldPlugin now schedules systems in `AppState::InGame`:

```rust
impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldMap>()
            .init_resource::<LoadedChunks>()
            .add_systems(OnEnter(AppState::InGame), load_tile_atlas)
            .add_systems(Update, chunk::chunk_loading_system.run_if(in_state(AppState::InGame)));
    }
}
```

- Remove old `wrap_*` tests (they now live in `registry::world` tests)

**Step 5: Verify build and tests**

Run: `cargo build`
Run: `cargo test`
Expected: all compile, all tests pass

**Step 6: Commit**

```
git add -A && git commit -m "refactor: migrate world module to data-driven TileId + WorldConfig"
```

---

### Task 6: Migrate player module

**Files:**
- Modify: `src/player/mod.rs` — remove constants, use PlayerConfig
- Modify: `src/player/movement.rs` — read Res<PlayerConfig>
- Modify: `src/player/collision.rs` — read Res<PlayerConfig>, Res<TileRegistry>, Res<WorldConfig>
- Modify: `src/player/wrap.rs` — read Res<WorldConfig>

**Step 1: Modify `src/player/mod.rs`**

- Remove `pub const PLAYER_SPEED`, `JUMP_VELOCITY`, `GRAVITY`, `PLAYER_WIDTH`, `PLAYER_HEIGHT`
- Keep `MAX_DELTA_SECS` (physics constant, not a game design parameter)
- Keep `Player`, `Velocity`, `Grounded` components
- Move gameplay systems to `AppState::InGame`:

```rust
impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_player)
            .add_systems(
                Update,
                (
                    movement::player_input,
                    movement::apply_gravity,
                    collision::collision_system,
                    wrap::player_wrap_system,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
```

- `spawn_player` reads `Res<PlayerConfig>` and `Res<WorldConfig>`:
  - `player_config.width` / `player_config.height` for sprite size
  - `WorldConfig.seed` for surface_height, `WorldConfig.tile_size` for pixel position

**Step 2: Modify `src/player/movement.rs`**

```rust
use crate::registry::player::PlayerConfig;

pub fn player_input(
    keys: Res<ButtonInput<KeyCode>>,
    player_config: Res<PlayerConfig>,
    mut query: Query<(&mut Velocity, &Grounded), With<Player>>,
) {
    for (mut vel, grounded) in &mut query {
        vel.x = 0.0;
        if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
            vel.x -= player_config.speed;
        }
        if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
            vel.x += player_config.speed;
        }
        if keys.just_pressed(KeyCode::Space) && grounded.0 {
            vel.y = player_config.jump_velocity;
        }
    }
}

pub fn apply_gravity(
    time: Res<Time>,
    player_config: Res<PlayerConfig>,
    mut query: Query<&mut Velocity, With<Player>>,
) {
    let dt = time.delta_secs().min(MAX_DELTA_SECS);
    for mut vel in &mut query {
        vel.y -= player_config.gravity * dt;
    }
}
```

**Step 3: Modify `src/player/collision.rs`**

- Replace `PLAYER_WIDTH` / `PLAYER_HEIGHT` with `player_config.width` / `player_config.height`
- Replace `TILE_SIZE` with `world_config.tile_size`
- Replace `world_map.is_solid(tx, ty)` with `tile_registry.is_solid(world_map.get_tile(tx, ty, &world_config, &terrain_tiles))`

System signature becomes:
```rust
pub fn collision_system(
    time: Res<Time>,
    player_config: Res<PlayerConfig>,
    world_config: Res<WorldConfig>,
    terrain_tiles: Res<TerrainTiles>,
    tile_registry: Res<TileRegistry>,
    mut world_map: ResMut<WorldMap>,
    mut query: Query<(&mut Transform, &mut Velocity, &mut Grounded), With<Player>>,
)
```

Update AABB methods to take `tile_size: f32` parameter instead of using the constant.

**Step 4: Modify `src/player/wrap.rs`**

```rust
use crate::registry::world::WorldConfig;

pub fn player_wrap_system(
    world_config: Res<WorldConfig>,
    mut query: Query<&mut Transform, With<Player>>,
) {
    let world_w = world_config.world_pixel_width();
    // rest stays the same
}
```

**Step 5: Verify build and tests**

Run: `cargo build`
Run: `cargo test`
Expected: compiles, tests pass

**Step 6: Commit**

```
git add -A && git commit -m "refactor: migrate player module to data-driven PlayerConfig + WorldConfig"
```

---

### Task 7: Migrate remaining modules

**Files:**
- Modify: `src/interaction/block_action.rs`
- Modify: `src/camera/mod.rs`
- Modify: `src/camera/follow.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/ui/debug_hud.rs`
- Modify: `src/main.rs`

**Step 1: Migrate `src/interaction/block_action.rs`**

- Replace `TILE_SIZE` → `world_config.tile_size`
- Replace `CHUNK_SIZE` → `world_config.chunk_size`
- Replace `WORLD_WIDTH_TILES` → `world_config.width_tiles`
- Replace `wrap_chunk_x(x)` → `world_config.wrap_chunk_x(x)`
- Replace `world_map.get_tile(tx, ty)` → `world_map.get_tile(tx, ty, &world_config, &terrain_tiles)`
- Replace `TileType::Air` → `TileId::AIR`
- Replace `TileType::Dirt` → `tile_registry.by_name("dirt")`
- Replace `place_type.texture_index().unwrap()` → `tile_registry.texture_index(place_id).unwrap()`
- System reads: `Res<WorldConfig>`, `Res<TileRegistry>`, `Res<TerrainTiles>`

**Step 2: Migrate `src/camera/mod.rs` and `src/camera/follow.rs`**

- Camera systems move to `AppState::InGame`:

```rust
impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            follow::camera_follow_player
                .after(player_wrap_system)
                .run_if(in_state(AppState::InGame)),
        );
    }
}
```

- `camera_follow_player` reads `Res<WorldConfig>`:
  - Replace `WORLD_HEIGHT_TILES as f32 * TILE_SIZE` → `world_config.world_pixel_height()`

**Step 3: Migrate `src/ui/mod.rs` and `src/ui/debug_hud.rs`**

- UI systems: `spawn_debug_hud` moves to `OnEnter(AppState::InGame)`, `update_debug_hud` runs `in_state(AppState::InGame)`
- `update_debug_hud` reads `Res<WorldConfig>`:
  - Replace `TILE_SIZE` → `world_config.tile_size`

**Step 4: Update `src/main.rs`**

- Add `mod registry;`
- Add `.add_plugins(registry::RegistryPlugin)` BEFORE other game plugins
- Remove `world::WorldPlugin` init_resource calls if they moved to WorldPlugin
- Camera spawn stays in Startup (camera exists before InGame)

Final `main.rs` structure:
```rust
mod camera;
mod interaction;
mod player;
mod registry;
mod ui;
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
                        resolution: (1280, 720).into(),
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins(TilemapPlugin)
        .add_plugins(registry::RegistryPlugin)
        .add_plugins(world::WorldPlugin)
        .add_plugins(player::PlayerPlugin)
        .add_plugins(camera::CameraPlugin)
        .add_plugins(interaction::InteractionPlugin)
        .add_plugins(ui::UiPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: 2.0,
            ..OrthographicProjection::default_2d()
        }),
    ));
}
```

**Step 5: Migrate InteractionPlugin to AppState::InGame**

```rust
impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, block_action::block_interaction_system.run_if(in_state(AppState::InGame)));
    }
}
```

**Step 6: Verify build and tests**

Run: `cargo build`
Run: `cargo test`
Expected: compiles, all tests pass

**Step 7: Commit**

```
git add -A && git commit -m "refactor: migrate interaction/camera/ui/main to data-driven resources"
```

---

### Task 8: Final cleanup and verification

**Files:**
- Potentially: remove dead code, fix warnings
- Verify: all tests pass, game runs correctly

**Step 1: Run full test suite**

Run: `cargo test`
Expected: all tests pass (count should be similar to 34, some tests moved/renamed)

**Step 2: Check for warnings**

Run: `cargo build 2>&1 | grep warning`
Fix any unused import/dead code warnings.

**Step 3: Run the game**

Run: `cargo run`
Expected: game loads RON files, enters InGame, renders tilemap with textures, player moves/jumps/collides normally. Identical behavior to before but data-driven.

**Step 4: Test hot-reload (optional)**

While game runs, edit `assets/data/player.def.ron` — change `speed: 400.0`. 
Note: hot-reload won't auto-update Resources (we'd need an AssetEvent watcher for that). This is a future enhancement, not in scope now.

**Step 5: Final commit**

```
git add -A && git commit -m "refactor: cleanup warnings and dead code after data-driven migration"
```

---

## Summary of all new/modified files

### New files:
- `assets/data/tiles.registry.ron`
- `assets/data/player.def.ron`
- `assets/data/world.config.ron`
- `src/registry/mod.rs` (RegistryPlugin, AppState, loading systems)
- `src/registry/tile.rs` (TileId, TileDef, TileRegistry, TerrainTiles)
- `src/registry/player.rs` (PlayerConfig)
- `src/registry/world.rs` (WorldConfig)
- `src/registry/assets.rs` (TileRegistryAsset, PlayerDefAsset, WorldConfigAsset)
- `src/registry/loader.rs` (RonLoader<T>)

### Modified files:
- `Cargo.toml` (add serde, ron, thiserror)
- `src/main.rs` (add mod registry, RegistryPlugin)
- `src/world/tile.rs` (enum → re-export TileId)
- `src/world/mod.rs` (remove constants, InGame state)
- `src/world/terrain_gen.rs` (WorldConfig + TerrainTiles params)
- `src/world/chunk.rs` (TileId, WorldConfig, TileRegistry params)
- `src/player/mod.rs` (remove constants, InGame state)
- `src/player/movement.rs` (Res<PlayerConfig>)
- `src/player/collision.rs` (Res<PlayerConfig>, Res<WorldConfig>, Res<TileRegistry>)
- `src/player/wrap.rs` (Res<WorldConfig>)
- `src/interaction/mod.rs` (InGame state)
- `src/interaction/block_action.rs` (Res<WorldConfig>, Res<TileRegistry>)
- `src/camera/mod.rs` (InGame state)
- `src/camera/follow.rs` (Res<WorldConfig>)
- `src/ui/mod.rs` (InGame state)
- `src/ui/debug_hud.rs` (Res<WorldConfig>)
