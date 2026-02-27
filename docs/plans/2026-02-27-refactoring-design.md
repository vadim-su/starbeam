# Refactoring Design — Prepare for Lighting, Inventory, Particle Physics

**Date:** 2026-02-27
**Status:** Approved
**Goal:** Full codebase refactoring to eliminate architectural debt and prepare shader/lighting pipeline

## Overview

Comprehensive refactoring of the Starbeam codebase after biome system completion. Addresses god parameter lists (13+ ECS params), mutable WorldMap for read operations, 738-line god file, string-based identities in hot paths, duplicated logic, and zero system-level test coverage. Prepares data flow for lighting (ChunkData → mesh → shader pipeline).

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| WorldContext approach | SystemParam + ref wrapper (both levels) | Clean API at system and method level |
| Read/write split | Single WorldMap, `&self` for reads | Simplest, enables `Res<WorldMap>` for collision/interaction |
| Registry split | By function (3 files) | Sufficient without overengineering |
| Test strategy | Combined: unit + App system tests | Community-recommended, balances speed and coverage |
| Lighting prep | Medium: light_level + ATTRIBUTE_LIGHT + shader input | Data flows through pipeline, minimal code to activate later |
| Next feature priority | Lighting and shaders | User decision |

## 1. Architectural Foundation

### 1.1 WorldCtx — `#[derive(SystemParam)]` + Reference Wrapper

Two-level solution for the "god parameter list" problem (13 `#[allow(clippy::too_many_arguments)]`).

**System level** — Bevy-native SystemParam:
```rust
#[derive(SystemParam)]
pub struct WorldCtx<'w> {
    pub config: Res<'w, WorldConfig>,
    pub biome_map: Res<'w, BiomeMap>,
    pub biome_registry: Res<'w, BiomeRegistry>,
    pub tile_registry: Res<'w, TileRegistry>,
    pub planet_config: Res<'w, PlanetConfig>,
}
```

**Method level** — lightweight ref wrapper for passing into WorldMap methods:
```rust
pub struct WorldCtxRef<'a> {
    pub config: &'a WorldConfig,
    pub biome_map: &'a BiomeMap,
    pub biome_registry: &'a BiomeRegistry,
    pub tile_registry: &'a TileRegistry,
    pub planet_config: &'a PlanetConfig,
}

impl WorldCtx<'_> {
    pub fn as_ref(&self) -> WorldCtxRef<'_> { ... }
}
```

**Effect:** `world_map.get_tile(x, y, &config, &biome_map, &biome_registry, &tile_registry, &planet_config)` → `world_map.get_tile(x, y, &ctx.as_ref())`

Files affected: `chunk.rs`, `terrain_gen.rs`, `block_action.rs`, `collision.rs`, `mesh_builder.rs`

### 1.2 Read/Write Split on WorldMap

- `get_tile(&self, x, y, ctx)` — reads only loaded chunks, returns `Option<TileId>` (None = unloaded)
- `is_solid(&self, x, y, ctx)` — same, `&self`
- `generate_chunk(&mut self, ...)` / `set_tile(&mut self, ...)` — remain `&mut self`
- `collision_system` and `block_action_system` switch to `Res<WorldMap>`
- `chunk_loading_system` keeps `ResMut<WorldMap>`

This enables Bevy to schedule collision and rendering systems in parallel.

### 1.3 Split registry/mod.rs (738 lines → 3 files)

| File | Contents |
|------|----------|
| `registry/loading.rs` | `LoadingAssets`, `LoadingBiomeAssets`, `LoadingAutotileAssets`, `check_loading()`, `check_biomes_loaded()`, `check_autotile_loaded()` |
| `registry/hot_reload.rs` | `BiomeHandles`, 6 hot-reload systems |
| `registry/mod.rs` | `RegistryPlugin`, `AppState`, `RegistryHandles`, `BiomeParallaxConfigs`, plugin setup |

## 2. Type Cleanup and Utilities

### 2.1 BiomeId Newtype

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BiomeId(pub u16);
```

Replaces `String` biome identity everywhere:
- `BiomeRegistry`: `HashMap<BiomeId, BiomeDef>` + `name_to_id: HashMap<String, BiomeId>`
- `BiomeMap` returns `BiomeId` instead of `String`
- `ParallaxLayer`, `CurrentBiome`, `BiomeParallaxConfigs` — all switch to `BiomeId`
- String lookup remains for RON loading: `BiomeRegistry::id_by_name(&str) -> BiomeId`
- Hot path `generate_tile()` copies `u16` instead of `String::clone()`

### 2.2 Perlin Caching

```rust
#[derive(Resource)]
pub struct TerrainNoiseCache {
    pub surface: Perlin,      // Perlin::new(seed)
    pub cave: Perlin,         // Perlin::new(seed.wrapping_add(1))
}
```

Created once at world load, stored as `Resource`. `generate_tile()` and `surface_height()` receive `&TerrainNoiseCache` instead of `seed: u32`.

### 2.3 Shared AABB Utility

New `src/math.rs`:
```rust
pub struct Aabb { pub min: Vec2, pub max: Vec2 }

impl Aabb {
    pub fn from_center(center: Vec2, half: Vec2) -> Self;
    pub fn overlaps(&self, other: &Aabb) -> bool;
    pub fn overlapping_tiles(&self, tile_size: f32) -> impl Iterator<Item = (i32, i32)>;
}

pub fn tile_aabb(tx: i32, ty: i32, tile_size: f32) -> Aabb;
```

- `collision.rs` and `block_action.rs` import from `math`
- `overlapping_tiles()` returns iterator instead of `Vec` (no allocations)

### 2.4 Shared Test Fixtures

```rust
// src/test_helpers.rs
#[cfg(test)]
pub mod fixtures {
    pub fn test_world_config() -> WorldConfig;
    pub fn test_biome_map() -> BiomeMap;
    pub fn test_biome_registry() -> BiomeRegistry;
    pub fn test_tile_registry() -> TileRegistry;
    pub fn test_planet_config() -> PlanetConfig;
    pub fn test_world_ctx_ref() -> (...);
    pub fn test_app() -> App;  // mini App with base resources for system tests
}
```

Eliminates ~200 lines of duplication across `chunk.rs` and `terrain_gen.rs`.

### 2.5 Visibility Narrowing

| Field | Change |
|-------|--------|
| `WorldMap.chunks` | `pub` → `pub(crate)` + accessor `chunk(&self, cx, cy)` |
| `TileRegistry.defs` | `pub` → `pub(crate)` |
| `BiomeRegistry.biomes` | `pub` → `pub(crate)` |
| `AutotileRegistry.entries` | `pub` → `pub(crate)` |
| `LoadedChunks.map` | `pub` → `pub(crate)` |

## 3. Bevy Patterns and Modularity

### 3.1 Camera into CameraPlugin

Camera spawn moves from `main.rs` into `CameraPlugin`. `Material2dPlugin::<TileMaterial>` moves into `WorldPlugin`. `main.rs` becomes pure plugin registration.

```rust
const CAMERA_SCALE: f32 = 0.7;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
           .add_systems(Update, camera_follow_player.run_if(in_state(AppState::InGame)));
    }
}
```

### 3.2 Split ParallaxLayer → Config + State

```rust
#[derive(Component)]
pub struct ParallaxConfig {
    pub speed_x: f32,
    pub speed_y: f32,
    pub repeat_x: bool,
    pub repeat_y: bool,
    pub biome_id: BiomeId,
}

#[derive(Component, Default)]
pub struct ParallaxState {
    pub texture_size: Vec2,
    pub initialized: bool,
}
```

### 3.3 SystemSets for Cross-Module Ordering

```rust
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameSet {
    Input,
    Physics,
    WorldUpdate,
    Camera,
    Parallax,
    Ui,
}
```

Modules no longer import each other's functions for `.after()` / `.before()`.

### 3.4 Data-Driven WorldLayer Boundaries

Layer depth ratios move from hardcoded `0.12/0.37/0.70` in `biome.rs` to `garden.planet.ron`:

```ron
(
    layers: (
        surface: ( biome: "meadow", depth_ratio: 0.30 ),
        underground: ( biome: "underground_dirt", depth_ratio: 0.25 ),
        deep_underground: ( biome: "underground_rock", depth_ratio: 0.33 ),
        core: ( biome: "core_magma", depth_ratio: 0.12 ),
    ),
)
```

`PlanetConfig` computes boundary Y coordinates. `WorldLayer::from_tile_y()` uses `PlanetConfig` instead of magic numbers.

## 4. Lighting Preparation

### 4.1 Light Level in ChunkData

```rust
pub struct ChunkData {
    pub tiles: Vec<TileId>,
    pub light_levels: Vec<u8>,  // 0 = full dark, 255 = full light
}
```

Initialized to `255` (full light) — world looks identical to current state. 1024 bytes per chunk overhead.

### 4.2 ATTRIBUTE_LIGHT in Mesh Builder

```rust
pub const ATTRIBUTE_LIGHT: MeshVertexAttribute =
    MeshVertexAttribute::new("Light", 988_540_917, VertexFormat::Float32);
```

Each vertex receives `light = chunk.light_levels[tile_index] as f32 / 255.0`. Mesh builder passes the attribute to the shader.

### 4.3 Shader Pipeline

`tile.wgsl` accepts `light: f32` as vertex input, passes through to fragment shader but does not use it yet:

```wgsl
@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(base_texture, base_sampler, in.uv);
    // TODO: return vec4(color.rgb * in.light, color.a);
    return color;
}
```

When lighting is implemented: uncomment one line + write light propagation system.

## 5. Testing Strategy

### Unit Tests (pure functions)
- `math.rs` — Aabb, overlapping_tiles iterator
- `biome_map.rs` — existing 11 tests + BiomeId adaptation
- `terrain_gen.rs` — tests with TerrainNoiseCache
- `WorldLayer::from_tile_y()` — data-driven boundaries

### System Tests (App + update)

Key systems to cover:

| System | Assertion |
|--------|-----------|
| `collision_system` | Uses `Res<WorldMap>` (not ResMut), correct collision |
| `chunk_loading_system` | Generates chunks around camera |
| `block_action_system` | Break/place blocks via `Res<WorldMap>` |
| `track_player_biome` | CurrentBiome switches on movement |
| `parallax_transition_system` | Crossfade alpha progression |
| `camera_follow_player` | Camera position follows player |

### Shared Fixtures
- `fixtures::test_app()` — App with base resources
- `fixtures::test_world_config()` etc — reusable across all test modules

## 6. Files Affected Summary

**New files:**
- `src/world/ctx.rs` — WorldCtx, WorldCtxRef
- `src/math.rs` — Aabb, tile_aabb
- `src/test_helpers.rs` — shared test fixtures
- `src/sets.rs` — GameSet enum
- `src/registry/loading.rs` — extracted loading logic
- `src/registry/hot_reload.rs` — extracted hot-reload systems

**Major changes:**
- `src/world/chunk.rs` — WorldCtxRef params, read/write split, light_levels
- `src/world/terrain_gen.rs` — TerrainNoiseCache, WorldCtxRef
- `src/world/mesh_builder.rs` — ATTRIBUTE_LIGHT
- `src/world/biome_map.rs` — BiomeId instead of String
- `src/interaction/block_action.rs` — WorldCtx, Res<WorldMap>, shared Aabb
- `src/player/collision.rs` — WorldCtx, Res<WorldMap>, Aabb to math.rs
- `src/parallax/spawn.rs` — ParallaxConfig + ParallaxState split
- `src/parallax/scroll.rs` — ParallaxConfig/ParallaxState queries
- `src/parallax/transition.rs` — BiomeId, ParallaxConfig
- `src/registry/mod.rs` — slimmed down, delegates to loading.rs/hot_reload.rs
- `src/registry/biome.rs` — BiomeId, data-driven layer boundaries
- `src/registry/assets.rs` — depth_ratio in LayerConfigAsset
- `src/camera/mod.rs` — spawn_camera, CAMERA_SCALE
- `src/main.rs` — cleanup, GameSet ordering, remove bare systems
- `assets/world/planet_types/garden.planet.ron` — depth_ratio fields
- `assets/world/tile.wgsl` — light vertex attribute

## 7. Out of Scope

- Actual light propagation algorithm (future lighting task)
- Inventory / hotbar system (future)
- Particle physics / fluid simulation (future)
- New planet types beyond garden
- Camera smoothing/lerp (nice-to-have, low priority)
- Data-driven animation frames (low priority)
