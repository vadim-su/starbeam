# Data-Driven Registry Design

## Summary

Replace all hardcoded game definitions (tiles, player, world) with data-driven assets loaded from RON files at startup via Bevy's custom `AssetLoader`. Properties become Bevy Resources accessible to all systems.

## Approach

**Approach C (hybrid):** One file for tile registry (compact, many entries), separate files for player and world config (complex, single entry each). Custom generic `RonLoader<T>` — no external dependencies, full control, ~30 lines of boilerplate.

## Data Files

### `assets/data/tiles.registry.ron`

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

Array order = numeric `TileId`. Air is always index 0.

### `assets/data/player.def.ron`

```ron
(
  speed: 200.0,
  jump_velocity: 400.0,
  gravity: 800.0,
  width: 64.0,
  height: 128.0,
)
```

### `assets/data/world.config.ron`

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

## Rust Types

### TileId

Replaces `enum TileType`. Compact `u16`, index into registry array.

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TileId(pub u16);

impl TileId {
    pub const AIR: TileId = TileId(0); // always 0
}
```

### TileDef

Properties of a single tile type (deserialized from RON):

```rust
#[derive(Deserialize)]
pub struct TileDef {
    pub id: String,
    pub texture_index: Option<u32>,
    pub solid: bool,
    pub hardness: f32,
    pub friction: f32,
    pub viscosity: f32,
}
```

### TileRegistry (Resource)

Built from loaded asset. Provides lookup by `TileId` or name.

```rust
#[derive(Resource)]
pub struct TileRegistry {
    pub defs: Vec<TileDef>,
    pub name_to_id: HashMap<String, TileId>,
}

impl TileRegistry {
    pub fn get(&self, id: TileId) -> &TileDef;
    pub fn is_solid(&self, id: TileId) -> bool;
    pub fn texture_index(&self, id: TileId) -> Option<u32>;
    pub fn by_name(&self, name: &str) -> TileId;
}
```

### PlayerConfig (Resource)

```rust
#[derive(Resource, Deserialize)]
pub struct PlayerConfig {
    pub speed: f32,
    pub jump_velocity: f32,
    pub gravity: f32,
    pub width: f32,
    pub height: f32,
}
```

### WorldConfig (Resource)

```rust
#[derive(Resource, Deserialize)]
pub struct WorldConfig {
    pub width_tiles: i32,
    pub height_tiles: i32,
    pub chunk_size: u32,
    pub tile_size: f32,
    pub chunk_load_radius: i32,
    pub seed: u32,
}
```

`WorldConfig` also provides computed properties and helper methods:
- `width_chunks()`, `height_chunks()` — derived from tiles / chunk_size
- `wrap_tile_x()`, `wrap_chunk_x()` — moved from free functions

## Asset Loading

### Generic RonLoader

One generic `AssetLoader` for all RON types:

```rust
struct RonLoader<T> {
    extensions: Vec<String>,
    _phantom: PhantomData<T>,
}

impl<T: Asset + for<'de> Deserialize<'de>> AssetLoader for RonLoader<T> {
    type Asset = T;
    type Settings = ();
    type Error = RonLoaderError;

    async fn load(&self, reader, _settings, _load_context) -> Result<T, Self::Error> {
        let bytes = reader.read_to_end().await?;
        Ok(ron::de::from_bytes::<T>(&bytes)?)
    }

    fn extensions(&self) -> &[&str] { ... }
}
```

### Asset Types

```rust
#[derive(Asset, TypePath, Deserialize)]
pub struct TileRegistryAsset { pub tiles: Vec<TileDef> }

#[derive(Asset, TypePath, Deserialize)]
pub struct PlayerDefAsset { pub speed: f32, pub jump_velocity: f32, ... }

#[derive(Asset, TypePath, Deserialize)]
pub struct WorldConfigAsset { pub width_tiles: i32, pub height_tiles: i32, ... }
```

### Registration

```rust
app.init_asset::<TileRegistryAsset>()
   .init_asset::<PlayerDefAsset>()
   .init_asset::<WorldConfigAsset>()
   .register_asset_loader(RonLoader::<TileRegistryAsset>::new(&["tiles.ron"]))
   .register_asset_loader(RonLoader::<PlayerDefAsset>::new(&["player.ron"]))
   .register_asset_loader(RonLoader::<WorldConfigAsset>::new(&["world.ron"]));
```

## Lifecycle — AppState

```rust
#[derive(States, Default, Clone, Eq, PartialEq, Debug, Hash)]
pub enum AppState {
    #[default]
    Loading,
    InGame,
}
```

- **Loading**: Startup system calls `asset_server.load()` for 3 files, stores handles. An `Update` system polls `AssetServer::is_loaded_with_dependencies()`. When all loaded — builds `Res<TileRegistry>`, `Res<PlayerConfig>`, `Res<WorldConfig>` from the asset data and transitions to `InGame`.
- **InGame**: All gameplay systems run here. They read properties from Resources instead of constants.

## Migration — What Changes in Existing Code

| File | Change |
|------|--------|
| `world/tile.rs` | `enum TileType` → `TileId(u16)`. Remove `is_solid()`, `texture_index()` methods. |
| `world/chunk.rs` | `ChunkData.tiles: Vec<TileId>`. `WorldMap.seed` removed (lives in `WorldConfig`). Functions accept `&TileRegistry` / `&WorldConfig` as needed. |
| `world/mod.rs` | Remove `WORLD_WIDTH_TILES`, `CHUNK_SIZE`, `TILE_SIZE` etc. constants. `wrap_tile_x` / `wrap_chunk_x` move to `WorldConfig` methods. |
| `world/terrain_gen.rs` | Use `registry.by_name("grass")` instead of `TileType::Grass`. Seed from `WorldConfig`. |
| `player/mod.rs` | Remove `PLAYER_SPEED`, `GRAVITY` etc. constants. |
| `player/movement.rs` | Read `Res<PlayerConfig>` for speed/jump/gravity. |
| `player/collision.rs` | Read `Res<PlayerConfig>` for width/height. Use `registry.is_solid(tile_id)`. |
| `interaction/block_action.rs` | Use `registry.texture_index(id)`, `registry.by_name("dirt")`. |
| `camera/follow.rs` | Read `Res<WorldConfig>` for world bounds. |
| All gameplay systems | Move from `Update` to `in_state(AppState::InGame)`. |
| Tests | Create `TileRegistry` / `WorldConfig` manually, pass to functions. |

## Dependencies

New crates needed:
- `serde` (derive `Deserialize`)
- `ron` (RON parser)
- `thiserror` (for `RonLoaderError`)
