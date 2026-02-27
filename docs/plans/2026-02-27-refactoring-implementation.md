# Refactoring Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Full codebase refactoring — eliminate god parameter lists, split read/write WorldMap, break up god file, introduce type-safe IDs, Bevy-idiomatic patterns, and prepare shader pipeline for lighting.

**Architecture:** Phased approach: foundation first (WorldCtx, read/write split, registry split), then type cleanup (BiomeId, Perlin cache, shared AABB), then Bevy patterns (SystemSets, camera plugin, parallax split), finally lighting prep (light_level, ATTRIBUTE_LIGHT, shader).

**Tech Stack:** Rust 2024 edition, Bevy 0.18, noise 0.9, RON configs, WGSL shaders.

**Verification:** After each task: `cargo test`, `cargo clippy -D warnings`, `cargo run` (visual check).

---

## Task 1: Shared Test Fixtures Module

Extract duplicated test helpers from `chunk.rs` and `terrain_gen.rs` into a shared module. This goes first because every subsequent task will need test infrastructure.

**Files:**
- Create: `src/test_helpers.rs`
- Modify: `src/main.rs` (add `mod test_helpers`)
- Modify: `src/world/chunk.rs` (remove duplicate fixtures, import shared)
- Modify: `src/world/terrain_gen.rs` (remove duplicate fixtures, import shared)

**Step 1: Create `src/test_helpers.rs` with shared fixtures**

```rust
//! Shared test fixtures for unit and system tests.

#[cfg(test)]
pub mod fixtures {
    use crate::registry::biome::{BiomeDef, BiomeRegistry, LayerConfig, LayerConfigs, PlanetConfig};
    use crate::registry::tile::{TileDef, TileId, TileRegistry};
    use crate::registry::world::WorldConfig;
    use crate::world::biome_map::BiomeMap;

    pub fn test_world_config() -> WorldConfig {
        WorldConfig {
            width_tiles: 2048,
            height_tiles: 1024,
            chunk_size: 32,
            tile_size: 32.0,
            chunk_load_radius: 3,
            seed: 42,
            planet_type: "garden".into(),
        }
    }

    pub fn test_biome_map() -> BiomeMap {
        BiomeMap::generate("meadow", &["forest", "rocky"], 42, 2048, 300, 600, 0.6)
    }

    pub fn test_biome_registry() -> BiomeRegistry {
        let mut reg = BiomeRegistry::default();
        for (id, surface, subsurface, depth, fill, cave) in [
            ("meadow", TileId(1), TileId(2), 4, TileId(3), 0.3),
            ("forest", TileId(1), TileId(2), 4, TileId(3), 0.3),
            ("rocky", TileId(3), TileId(3), 2, TileId(3), 0.3),
            ("underground_dirt", TileId(3), TileId(3), 0, TileId(3), 0.3),
            ("underground_rock", TileId(3), TileId(3), 0, TileId(3), 0.25),
            ("core_magma", TileId(3), TileId(3), 0, TileId(3), 0.15),
        ] {
            reg.biomes.insert(
                id.into(),
                BiomeDef {
                    id: id.into(),
                    surface_block: surface,
                    subsurface_block: subsurface,
                    subsurface_depth: depth,
                    fill_block: fill,
                    cave_threshold: cave,
                    parallax_path: None,
                },
            );
        }
        reg
    }

    pub fn test_tile_registry() -> TileRegistry {
        TileRegistry::from_defs(vec![
            TileDef { id: "air".into(), autotile: None, solid: false, hardness: 0.0, friction: 0.0, viscosity: 0.0, damage_on_contact: 0.0, effects: vec![] },
            TileDef { id: "grass".into(), autotile: Some("grass".into()), solid: true, hardness: 1.0, friction: 0.8, viscosity: 0.0, damage_on_contact: 0.0, effects: vec![] },
            TileDef { id: "dirt".into(), autotile: Some("dirt".into()), solid: true, hardness: 2.0, friction: 0.7, viscosity: 0.0, damage_on_contact: 0.0, effects: vec![] },
            TileDef { id: "stone".into(), autotile: Some("stone".into()), solid: true, hardness: 5.0, friction: 0.6, viscosity: 0.0, damage_on_contact: 0.0, effects: vec![] },
        ])
    }

    pub fn test_planet_config() -> PlanetConfig {
        PlanetConfig {
            id: "garden".into(),
            primary_biome: "meadow".into(),
            secondary_biomes: vec!["forest".into(), "rocky".into()],
            layers: LayerConfigs {
                surface: LayerConfig { primary_biome: None, terrain_frequency: 0.02, terrain_amplitude: 40.0 },
                underground: LayerConfig { primary_biome: Some("underground_dirt".into()), terrain_frequency: 0.07, terrain_amplitude: 1.0 },
                deep_underground: LayerConfig { primary_biome: Some("underground_rock".into()), terrain_frequency: 0.05, terrain_amplitude: 1.0 },
                core: LayerConfig { primary_biome: Some("core_magma".into()), terrain_frequency: 0.04, terrain_amplitude: 1.0 },
            },
            region_width_min: 300,
            region_width_max: 600,
            primary_region_ratio: 0.6,
        }
    }
}
```

**Step 2: Add `mod test_helpers` to `src/main.rs`**

Add `#[cfg(test)] mod test_helpers;` after the existing module declarations.

**Step 3: Update `src/world/chunk.rs` tests** — replace local `test_wc()`, `test_biome_map()`, `test_biome_registry()`, `test_tile_registry()`, `test_planet_config()` with imports from `crate::test_helpers::fixtures::*`.

**Step 4: Update `src/world/terrain_gen.rs` tests** — same replacement.

**Step 5: Run tests**

```bash
cargo test
```

Expected: All 67 tests pass.

**Step 6: Commit**

```
refactor: extract shared test fixtures into test_helpers module
```

---

## Task 2: Shared AABB Utility (`src/math.rs`)

Extract `Aabb` and `tile_aabb` from `player/collision.rs` into `src/math.rs`. Replace Vec allocation in `overlapping_tiles()` with a no-alloc iterator.

**Files:**
- Create: `src/math.rs`
- Modify: `src/main.rs` (add `pub mod math`)
- Modify: `src/player/collision.rs` (remove Aabb/tile_aabb, import from math)
- Modify: `src/interaction/block_action.rs` (use shared Aabb for overlap check)

**Step 1: Create `src/math.rs`**

```rust
//! Shared math utilities: AABB collision, tile coordinate helpers.

/// Axis-aligned bounding box with min/max corners.
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

    /// Check if two AABBs overlap.
    pub fn overlaps(&self, other: &Aabb) -> bool {
        self.max_x > other.min_x
            && self.min_x < other.max_x
            && self.max_y > other.min_y
            && self.min_y < other.max_y
    }

    /// Iterate tile coordinates overlapping this AABB (no allocation).
    pub fn overlapping_tiles(&self, tile_size: f32) -> TileIterator {
        let min_tx = (self.min_x / tile_size).floor() as i32;
        let max_tx = ((self.max_x - 0.001) / tile_size).floor() as i32;
        let min_ty = (self.min_y / tile_size).floor() as i32;
        let max_ty = ((self.max_y - 0.001) / tile_size).floor() as i32;
        TileIterator { min_tx, max_tx, min_ty, max_ty, current_x: min_tx, current_y: min_ty }
    }
}

/// No-alloc iterator over tile coordinates.
pub struct TileIterator {
    min_tx: i32,
    max_tx: i32,
    #[allow(dead_code)]
    min_ty: i32,
    max_ty: i32,
    current_x: i32,
    current_y: i32,
}

impl Iterator for TileIterator {
    type Item = (i32, i32);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_y > self.max_ty {
            return None;
        }
        let result = (self.current_x, self.current_y);
        self.current_x += 1;
        if self.current_x > self.max_tx {
            self.current_x = self.min_tx;
            self.current_y += 1;
        }
        Some(result)
    }
}

/// AABB for a single tile at grid coordinates.
pub fn tile_aabb(tx: i32, ty: i32, tile_size: f32) -> Aabb {
    Aabb {
        min_x: tx as f32 * tile_size,
        max_x: (tx + 1) as f32 * tile_size,
        min_y: ty as f32 * tile_size,
        max_y: (ty + 1) as f32 * tile_size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TS: f32 = 32.0;

    #[test]
    fn aabb_from_center() {
        let aabb = Aabb::from_center(100.0, 200.0, 24.0, 48.0);
        assert_eq!(aabb.min_x, 88.0);
        assert_eq!(aabb.max_x, 112.0);
        assert_eq!(aabb.min_y, 176.0);
        assert_eq!(aabb.max_y, 224.0);
    }

    #[test]
    fn overlaps_true() {
        let a = Aabb::from_center(10.0, 10.0, 10.0, 10.0);
        let b = Aabb::from_center(14.0, 10.0, 10.0, 10.0);
        assert!(a.overlaps(&b));
    }

    #[test]
    fn overlaps_false() {
        let a = Aabb::from_center(0.0, 0.0, 4.0, 4.0);
        let b = Aabb::from_center(100.0, 100.0, 4.0, 4.0);
        assert!(!a.overlaps(&b));
    }

    #[test]
    fn overlapping_tiles_single() {
        let center_x = 3.0 * TS + TS / 2.0;
        let center_y = 3.0 * TS + TS / 2.0;
        let aabb = Aabb::from_center(center_x, center_y, 20.0, 20.0);
        let tiles: Vec<_> = aabb.overlapping_tiles(TS).collect();
        assert_eq!(tiles, vec![(3, 3)]);
    }

    #[test]
    fn overlapping_tiles_multiple() {
        let aabb = Aabb::from_center(32.0, 32.0, 24.0, 48.0);
        let tiles: Vec<_> = aabb.overlapping_tiles(TS).collect();
        assert!(tiles.len() >= 2);
        assert!(tiles.contains(&(0, 0)));
    }

    #[test]
    fn tile_aabb_basic() {
        let aabb = tile_aabb(3, 5, TS);
        assert_eq!(aabb.min_x, 96.0);
        assert_eq!(aabb.max_x, 128.0);
        assert_eq!(aabb.min_y, 160.0);
        assert_eq!(aabb.max_y, 192.0);
    }
}
```

**Step 2: Add `pub mod math;` to `src/main.rs`**

**Step 3: Update `src/player/collision.rs`** — remove `Aabb`, `tile_aabb`, and their tests. Import from `crate::math`. Use `aabb.overlaps(&tile)` in collision resolution.

**Step 4: Update `src/interaction/block_action.rs`** — replace inlined player-tile overlap check (lines 106-122) with `Aabb::from_center` + `aabb.overlaps(&tile_aabb(...))`.

**Step 5: Run tests**

```bash
cargo test
cargo clippy -D warnings
```

**Step 6: Commit**

```
refactor: extract shared AABB utility into math module
```

---

## Task 3: WorldCtx SystemParam + WorldCtxRef

Introduce the two-level context wrapper to eliminate god parameter lists.

**Files:**
- Create: `src/world/ctx.rs`
- Modify: `src/world/mod.rs` (add `pub mod ctx`)
- Modify: `src/world/chunk.rs` (use WorldCtxRef in all WorldMap methods, free functions)
- Modify: `src/world/terrain_gen.rs` (use WorldCtxRef)
- Modify: `src/world/mesh_builder.rs` (minor — some params stay separate)
- Modify: `src/player/collision.rs` (use WorldCtx SystemParam)
- Modify: `src/interaction/block_action.rs` (use WorldCtx SystemParam)
- Modify: `src/test_helpers.rs` (add `test_world_ctx_ref()` helper)

**Step 1: Create `src/world/ctx.rs`**

```rust
//! World context bundles for reducing parameter count.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::registry::biome::{BiomeRegistry, PlanetConfig};
use crate::registry::tile::TileRegistry;
use crate::registry::world::WorldConfig;
use crate::world::biome_map::BiomeMap;

/// Bevy SystemParam that bundles the 5 read-only world resources.
/// Use in systems: `fn my_system(ctx: WorldCtx, ...)`
#[derive(SystemParam)]
pub struct WorldCtx<'w> {
    pub config: Res<'w, WorldConfig>,
    pub biome_map: Res<'w, BiomeMap>,
    pub biome_registry: Res<'w, BiomeRegistry>,
    pub tile_registry: Res<'w, TileRegistry>,
    pub planet_config: Res<'w, PlanetConfig>,
}

impl WorldCtx<'_> {
    /// Create a lightweight reference wrapper for passing into functions.
    pub fn as_ref(&self) -> WorldCtxRef<'_> {
        WorldCtxRef {
            config: &self.config,
            biome_map: &self.biome_map,
            biome_registry: &self.biome_registry,
            tile_registry: &self.tile_registry,
            planet_config: &self.planet_config,
        }
    }
}

/// Lightweight reference bundle for passing world context into methods.
/// Use in functions: `fn get_tile(&self, x: i32, y: i32, ctx: &WorldCtxRef) -> TileId`
pub struct WorldCtxRef<'a> {
    pub config: &'a WorldConfig,
    pub biome_map: &'a BiomeMap,
    pub biome_registry: &'a BiomeRegistry,
    pub tile_registry: &'a TileRegistry,
    pub planet_config: &'a PlanetConfig,
}
```

**Step 2: Update `src/world/mod.rs`** — add `pub mod ctx;`

**Step 3: Update `WorldMap` methods in `src/world/chunk.rs`**

Change all method signatures to use `WorldCtxRef`. Example:

```rust
// Before:
pub fn get_tile(&mut self, tile_x: i32, tile_y: i32, wc: &WorldConfig, biome_map: &BiomeMap, biome_registry: &BiomeRegistry, tile_registry: &TileRegistry, planet_config: &PlanetConfig) -> TileId

// After:
pub fn get_tile(&mut self, tile_x: i32, tile_y: i32, ctx: &WorldCtxRef) -> TileId
```

Apply same pattern to: `get_or_generate_chunk`, `get_tile`, `get_tile_if_loaded`, `set_tile`, `is_solid`, `update_bitmasks_around`, `init_chunk_bitmasks`, `spawn_chunk`.

**Step 4: Update `src/world/terrain_gen.rs`** — `generate_tile()` and `generate_chunk_tiles()` take `ctx: &WorldCtxRef` instead of 5 separate params. `surface_height()` still takes individual params (it only needs `seed`, `wc`, `frequency`, `amplitude`).

**Step 5: Update ECS systems** — `collision_system`, `block_interaction_system`, `chunk_loading_system` use `WorldCtx` SystemParam.

**Step 6: Update `src/test_helpers.rs`** — add helper:

```rust
pub fn test_world_ctx_ref() -> (WorldConfig, BiomeMap, BiomeRegistry, TileRegistry, PlanetConfig) {
    (test_world_config(), test_biome_map(), test_biome_registry(), test_tile_registry(), test_planet_config())
}
```

**Step 7: Update all tests** to use `WorldCtxRef` for method calls.

**Step 8: Run tests**

```bash
cargo test
cargo clippy -D warnings
```

**Step 9: Commit**

```
refactor: introduce WorldCtx SystemParam and WorldCtxRef to reduce parameter count
```

---

## Task 4: Read/Write Split on WorldMap

Make `get_tile()` and `is_solid()` take `&self` (read-only). Collision and block_action switch to `Res<WorldMap>`.

**Files:**
- Modify: `src/world/chunk.rs` — split `get_tile` into read-only version, keep `get_tile_mut` for chunk_loading
- Modify: `src/player/collision.rs` — `Res<WorldMap>` instead of `ResMut<WorldMap>`
- Modify: `src/interaction/block_action.rs` — split into read check + write set_tile

**Step 1: Restructure `WorldMap` in `src/world/chunk.rs`**

- `get_tile(&self, x, y, ctx)` — calls `get_tile_if_loaded`, returns `Option<TileId>` (None = chunk not loaded)
- `is_solid(&self, x, y, ctx)` — `&self`, returns `false` for unloaded chunks
- `get_tile_mut(&mut self, x, y, ctx)` — old `get_tile` behavior (lazy generation)
- `set_tile(&mut self, ...)` — still calls `get_or_generate_chunk`
- `get_or_generate_chunk(&mut self, ...)` — unchanged

Remove old `get_tile(&mut self, ...)` to prevent confusion.

**Step 2: Update `collision_system`** — change `mut world_map: ResMut<WorldMap>` to `world_map: Res<WorldMap>`. Use `is_solid(&self, ...)` which returns `false` for unloaded.

**Step 3: Update `block_interaction_system`** — read checks use `world_map.get_tile(...)` (`&self`), write operations use separate `ResMut<WorldMap>` access. This system still needs `ResMut` for `set_tile`, but the read path is now `&self`.

NOTE: `block_interaction_system` needs both read and write — it reads to check current tile, then writes to change it. Keep `ResMut<WorldMap>` but use the read-only methods for the check portion. The key win is `collision_system` becoming `Res<WorldMap>`.

**Step 4: Update `update_bitmasks_around` and `init_chunk_bitmasks`** — these use `get_tile` internally for neighbor solidity. They need `&mut self` because they modify bitmask data. Keep using `get_tile_mut` (the mutating version) since they are called from `chunk_loading_system` and `block_interaction_system` which already hold `ResMut`.

**Step 5: Update all tests**

**Step 6: Run tests**

```bash
cargo test
cargo clippy -D warnings
```

**Step 7: Commit**

```
refactor: split WorldMap into read-only and mutating access paths
```

---

## Task 5: Split `registry/mod.rs` into 3 files

**Files:**
- Create: `src/registry/loading.rs`
- Create: `src/registry/hot_reload.rs`
- Modify: `src/registry/mod.rs` (slim down to plugin + types)

**Step 1: Create `src/registry/loading.rs`**

Move from `mod.rs`:
- `LoadingAssets` struct
- `LoadingBiomeAssets` struct
- `LoadingAutotileAssets` struct
- `start_loading()`
- `check_loading()`
- `check_biomes_loaded()`
- `start_autotile_loading()`
- `check_autotile_loading()`

Make all types and functions `pub(crate)`.

**Step 2: Create `src/registry/hot_reload.rs`**

Move from `mod.rs`:
- `BiomeHandles` struct
- `hot_reload_player()`
- `hot_reload_world()`
- `hot_reload_tiles()`
- `hot_reload_biomes()`
- `hot_reload_planet_type()`
- `hot_reload_biome_parallax()`

Make all `pub(crate)`.

**Step 3: Slim down `src/registry/mod.rs`**

Keep:
- Module declarations (`pub mod assets, biome, loader, player, tile, world, loading, hot_reload`)
- `RegistryHandles`
- `AppState`
- `BiomeParallaxConfigs`
- `RegistryPlugin` impl (delegating to loading/hot_reload functions)

**Step 4: Run tests**

```bash
cargo test
cargo clippy -D warnings
```

**Step 5: Commit**

```
refactor: split registry/mod.rs into loading.rs and hot_reload.rs
```

---

## Task 6: BiomeId Newtype

Replace `String` biome identity with a `u16`-based `BiomeId`.

**Files:**
- Modify: `src/registry/biome.rs` — add BiomeId, update BiomeRegistry
- Modify: `src/world/biome_map.rs` — BiomeRegion uses BiomeId, biome_at() returns BiomeId
- Modify: `src/world/terrain_gen.rs` — use BiomeId
- Modify: `src/parallax/spawn.rs` — ParallaxLayer uses BiomeId
- Modify: `src/parallax/transition.rs` — CurrentBiome, ParallaxTransition use BiomeId
- Modify: `src/registry/mod.rs` (BiomeParallaxConfigs)
- Modify: `src/registry/loading.rs` — build BiomeRegistry with BiomeId
- Modify: `src/registry/hot_reload.rs` — update hot-reload
- Modify: `src/ui/debug_panel.rs` — display BiomeId
- Modify: `src/test_helpers.rs` — update fixtures

**Step 1: Add BiomeId to `src/registry/biome.rs`**

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BiomeId(pub u16);

impl std::fmt::Display for BiomeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BiomeId({})", self.0)
    }
}
```

Update `BiomeRegistry`:
```rust
pub struct BiomeRegistry {
    biomes: HashMap<BiomeId, BiomeDef>,
    name_to_id: HashMap<String, BiomeId>,
    id_to_name: HashMap<BiomeId, String>,
    next_id: u16,
}

impl BiomeRegistry {
    pub fn insert(&mut self, name: &str, def: BiomeDef) -> BiomeId { ... }
    pub fn get(&self, id: BiomeId) -> &BiomeDef { ... }
    pub fn id_by_name(&self, name: &str) -> BiomeId { ... }
    pub fn name_of(&self, id: BiomeId) -> &str { ... }
}
```

**Step 2: Update `BiomeMap`** — `BiomeRegion.biome_id: BiomeId`, `biome_at() -> BiomeId`. The `generate()` function takes a `&BiomeRegistry` to resolve names to IDs.

**Step 3: Update all consumers** — terrain_gen, parallax, transition, debug_panel, loading, hot_reload, test_helpers.

**Step 4: Run tests**

```bash
cargo test
cargo clippy -D warnings
```

**Step 5: Commit**

```
refactor: replace String biome identity with BiomeId newtype
```

---

## Task 7: TerrainNoiseCache Resource

Cache Perlin instances instead of creating per-tile.

**Files:**
- Modify: `src/world/terrain_gen.rs` — add TerrainNoiseCache, update functions
- Modify: `src/world/chunk.rs` — pass TerrainNoiseCache to generate functions
- Modify: `src/world/ctx.rs` — optionally add to WorldCtxRef
- Modify: `src/registry/loading.rs` — create TerrainNoiseCache after WorldConfig is loaded
- Modify: `src/world/mod.rs` — init resource
- Modify: `src/player/mod.rs` — surface_height for spawn still needs it
- Modify: `src/test_helpers.rs` — add fixture

**Step 1: Add `TerrainNoiseCache` to `src/world/terrain_gen.rs`**

```rust
use bevy::prelude::Resource;

#[derive(Resource)]
pub struct TerrainNoiseCache {
    pub surface: Perlin,
    pub cave: Perlin,
}

impl TerrainNoiseCache {
    pub fn new(seed: u32) -> Self {
        Self {
            surface: Perlin::new(seed),
            cave: Perlin::new(seed.wrapping_add(1)),
        }
    }
}
```

**Step 2: Update `surface_height()` and `generate_tile()`** to take `&TerrainNoiseCache` instead of `seed: u32`.

**Step 3: Update callers** — `generate_chunk_tiles()`, `WorldMap::get_or_generate_chunk()`, `chunk_loading_system`, `spawn_player`.

**Step 4: Create resource** in `registry/loading.rs` after seed is known.

**Step 5: Run tests**

```bash
cargo test
cargo clippy -D warnings
```

**Step 6: Commit**

```
refactor: cache Perlin noise instances in TerrainNoiseCache resource
```

---

## Task 8: Visibility Narrowing (`pub` → `pub(crate)`)

**Files:**
- Modify: `src/world/chunk.rs` — `WorldMap.chunks`, `LoadedChunks.map`
- Modify: `src/registry/tile.rs` — `TileRegistry.defs`
- Modify: `src/registry/biome.rs` — `BiomeRegistry.biomes` (now private with accessor methods from Task 6)
- Modify: `src/world/autotile.rs` — `AutotileRegistry.entries`

**Step 1: Change `pub` → `pub(crate)` on each field**

Add accessor methods where needed:
- `WorldMap::chunk(&self, cx: i32, cy: i32) -> Option<&ChunkData>`
- `WorldMap::chunk_mut(&mut self, cx: i32, cy: i32) -> Option<&mut ChunkData>`
- `AutotileRegistry::get(&self, name: &str) -> Option<&AutotileEntry>`

**Step 2: Update all direct field accesses** to use new accessors.

**Step 3: Run tests**

```bash
cargo test
cargo clippy -D warnings
```

**Step 4: Commit**

```
refactor: narrow pub fields to pub(crate) with accessor methods
```

---

## Task 9: GameSet SystemSets + Cross-Module Ordering

**Files:**
- Create: `src/sets.rs`
- Modify: `src/main.rs` — add `pub mod sets`, configure set ordering
- Modify: `src/player/mod.rs` — use GameSet::Physics
- Modify: `src/camera/mod.rs` — use GameSet::Camera
- Modify: `src/parallax/mod.rs` — use GameSet::Parallax
- Modify: `src/world/mod.rs` — use GameSet::WorldUpdate
- Modify: `src/interaction/mod.rs` — use GameSet::Input
- Modify: `src/ui/mod.rs` — use GameSet::Ui

**Step 1: Create `src/sets.rs`**

```rust
use bevy::prelude::*;

/// Top-level system ordering sets for the game loop.
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

**Step 2: Configure ordering in `src/main.rs`**

```rust
app.configure_sets(
    Update,
    (
        GameSet::Input,
        GameSet::Physics,
        GameSet::WorldUpdate,
        GameSet::Camera,
        GameSet::Parallax,
        GameSet::Ui,
    ).chain()
);
```

**Step 3: Update each plugin** to use `.in_set(GameSet::X)` instead of `.after(specific_function)`.

Remove all direct function imports used for ordering (e.g., `use crate::player::wrap::player_wrap_system` in camera/mod.rs, `use crate::camera::follow::camera_follow_player` in parallax/mod.rs).

**Step 4: Run tests**

```bash
cargo test
cargo clippy -D warnings
```

**Step 5: Commit**

```
refactor: introduce GameSet system sets for decoupled cross-module ordering
```

---

## Task 10: Camera into CameraPlugin + Material2dPlugin into WorldPlugin

**Files:**
- Modify: `src/camera/mod.rs` — add `spawn_camera` system, `CAMERA_SCALE` constant
- Modify: `src/camera/follow.rs` — no changes needed
- Modify: `src/world/mod.rs` — add `Material2dPlugin::<TileMaterial>` registration
- Modify: `src/main.rs` — remove `setup()` function, remove Material2dPlugin line

**Step 1: Update `src/camera/mod.rs`**

```rust
pub mod follow;

use bevy::prelude::*;
use crate::registry::AppState;
use crate::sets::GameSet;

const CAMERA_SCALE: f32 = 0.7;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
           .add_systems(
               Update,
               follow::camera_follow_player
                   .in_set(GameSet::Camera)
                   .run_if(in_state(AppState::InGame)),
           );
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: CAMERA_SCALE,
            ..OrthographicProjection::default_2d()
        }),
    ));
}
```

**Step 2: Update `src/world/mod.rs`** — add Material2dPlugin registration.

**Step 3: Clean up `src/main.rs`** — remove `setup()` function, remove `Material2dPlugin` line, remove `use bevy::sprite_render::Material2dPlugin`.

**Step 4: Run and visual check**

```bash
cargo test
cargo clippy -D warnings
cargo run  # visual check — game should look identical
```

**Step 5: Commit**

```
refactor: move camera spawn into CameraPlugin, Material2dPlugin into WorldPlugin
```

---

## Task 11: Split ParallaxLayer → ParallaxConfig + ParallaxState

**Files:**
- Modify: `src/parallax/spawn.rs` — split component
- Modify: `src/parallax/scroll.rs` — update queries
- Modify: `src/parallax/transition.rs` — update spawn and queries

**Step 1: Update `src/parallax/spawn.rs`**

```rust
use bevy::prelude::*;
use crate::registry::biome::BiomeId;

#[derive(Component)]
pub struct ParallaxLayerConfig {
    pub biome_id: BiomeId,
    pub speed_x: f32,
    pub speed_y: f32,
    pub repeat_x: bool,
    pub repeat_y: bool,
}

#[derive(Component, Default)]
pub struct ParallaxLayerState {
    pub texture_size: Vec2,
    pub initialized: bool,
}

#[derive(Component)]
pub struct ParallaxTile;
```

**Step 2: Update `src/parallax/scroll.rs`** — queries use `(&ParallaxLayerConfig, &mut ParallaxLayerState, ...)`.

**Step 3: Update `src/parallax/transition.rs`** — spawn commands and queries.

**Step 4: Run tests**

```bash
cargo test
cargo clippy -D warnings
```

**Step 5: Commit**

```
refactor: split ParallaxLayer into ParallaxLayerConfig and ParallaxLayerState
```

---

## Task 12: Data-Driven WorldLayer Boundaries

Move hardcoded layer boundary ratios into planet type RON config.

**Files:**
- Modify: `src/registry/assets.rs` — add `depth_ratio` to `LayerConfigAsset`
- Modify: `src/registry/biome.rs` — add `depth_ratio` to `LayerConfig`, compute boundaries in `PlanetConfig`
- Modify: `src/registry/loading.rs` — pass depth_ratio through
- Modify: `src/registry/hot_reload.rs` — same
- Modify: `assets/world/planet_types/garden.planet.ron` — add depth_ratio fields
- Modify: `src/test_helpers.rs` — update fixtures

**Step 1: Add `depth_ratio` to RON**

```ron
// garden.planet.ron
(
    id: "garden",
    primary_biome: "meadow",
    secondary_biomes: ["forest", "rocky"],
    layers: (
        surface: (
            primary_biome: None,
            terrain_frequency: 0.02,
            terrain_amplitude: 40.0,
            depth_ratio: 0.30,
        ),
        underground: (
            primary_biome: Some("underground_dirt"),
            terrain_frequency: 0.07,
            terrain_amplitude: 1.0,
            depth_ratio: 0.25,
        ),
        deep_underground: (
            primary_biome: Some("underground_rock"),
            terrain_frequency: 0.05,
            terrain_amplitude: 1.0,
            depth_ratio: 0.33,
        ),
        core: (
            primary_biome: Some("core_magma"),
            terrain_frequency: 0.04,
            terrain_amplitude: 1.0,
            depth_ratio: 0.12,
        ),
    ),
    region_width_min: 300,
    region_width_max: 600,
    primary_region_ratio: 0.6,
)
```

**Step 2: Add to asset/runtime types**

```rust
// assets.rs — LayerConfigAsset
pub struct LayerConfigAsset {
    pub primary_biome: Option<String>,
    pub terrain_frequency: f64,
    pub terrain_amplitude: f64,
    #[serde(default = "default_depth_ratio")]
    pub depth_ratio: f64,
}

// biome.rs — LayerConfig
pub struct LayerConfig {
    pub primary_biome: Option<String>,
    pub terrain_frequency: f64,
    pub terrain_amplitude: f64,
    pub depth_ratio: f64,
}

// biome.rs — PlanetConfig
pub struct PlanetConfig {
    // ... existing fields ...
    /// Computed Y boundaries for each layer (from bottom: core_top, deep_top, underground_top)
    pub layer_boundaries: LayerBoundaries,
}

pub struct LayerBoundaries {
    pub core_top: i32,
    pub deep_underground_top: i32,
    pub underground_top: i32,
}
```

**Step 3: Update `WorldLayer::from_tile_y()`** to use `PlanetConfig.layer_boundaries` instead of hardcoded ratios.

```rust
impl WorldLayer {
    pub fn from_tile_y(tile_y: i32, planet_config: &PlanetConfig) -> Self {
        let b = &planet_config.layer_boundaries;
        if tile_y < b.core_top {
            WorldLayer::Core
        } else if tile_y < b.deep_underground_top {
            WorldLayer::DeepUnderground
        } else if tile_y < b.underground_top {
            WorldLayer::Underground
        } else {
            WorldLayer::Surface
        }
    }
}
```

**Step 4: Compute boundaries** in loading.rs when building PlanetConfig from asset:

```rust
fn compute_layer_boundaries(layers: &LayerConfigs, world_height: i32) -> LayerBoundaries {
    let core_top = (layers.core.depth_ratio * world_height as f64) as i32;
    let deep_top = core_top + (layers.deep_underground.depth_ratio * world_height as f64) as i32;
    let underground_top = deep_top + (layers.underground.depth_ratio * world_height as f64) as i32;
    LayerBoundaries { core_top, deep_underground_top: deep_top, underground_top }
}
```

**Step 5: Update tests**

**Step 6: Run tests**

```bash
cargo test
cargo clippy -D warnings
```

**Step 7: Commit**

```
refactor: make WorldLayer boundaries data-driven via planet type depth_ratio
```

---

## Task 13: Light Level in ChunkData + ATTRIBUTE_LIGHT in Mesh Builder

Prepare the data pipeline for lighting: ChunkData stores light levels, mesh builder passes them to shader.

**Files:**
- Modify: `src/world/chunk.rs` — add `light_levels: Vec<u8>` to ChunkData
- Modify: `src/world/mesh_builder.rs` — add ATTRIBUTE_LIGHT, pass light values, add to MeshBuildBuffers
- Modify: `src/world/tile_renderer.rs` — add vertex shader for light attribute
- Modify: `assets/shaders/tile.wgsl` — accept light input, pass through (don't use yet)

**Step 1: Update `ChunkData` in `src/world/chunk.rs`**

```rust
pub struct ChunkData {
    pub tiles: Vec<TileId>,
    pub bitmasks: Vec<u8>,
    pub light_levels: Vec<u8>,  // 0 = full dark, 255 = full light
    #[allow(dead_code)]
    pub damage: Vec<u8>,
}
```

Initialize `light_levels` to `vec![255; len]` in `get_or_generate_chunk`.

**Step 2: Update `MeshBuildBuffers` and `build_chunk_mesh` in `src/world/mesh_builder.rs`**

```rust
use bevy::render::mesh::MeshVertexAttribute;
use bevy::render::render_resource::VertexFormat;

pub const ATTRIBUTE_LIGHT: MeshVertexAttribute =
    MeshVertexAttribute::new("Light", 988_540_917, VertexFormat::Float32);

pub struct MeshBuildBuffers {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub lights: Vec<f32>,  // NEW
    pub indices: Vec<u32>,
}
```

In `build_chunk_mesh`, add `light_levels: &[u8]` parameter. For each tile quad, push `light_levels[idx] as f32 / 255.0` four times (one per vertex). Add `mesh.insert_attribute(ATTRIBUTE_LIGHT, buffers.lights.clone());`.

**Step 3: Update callers** — pass `&chunk_data.light_levels` to `build_chunk_mesh`.

**Step 4: Update `assets/shaders/tile.wgsl`**

The current shader uses Bevy's `mesh2d_vertex_output::VertexOutput` which auto-maps standard attributes. For custom attributes, we need a custom vertex shader or use the `@location` approach. Since Bevy 0.18 Material2d automatically handles standard mesh attributes, we need to check if custom attributes pass through.

For now, add light to the mesh but don't modify the shader (Bevy ignores extra vertex attributes gracefully). The shader change will be done when implementing lighting.

**Step 5: Run tests**

```bash
cargo test
cargo clippy -D warnings
cargo run  # visual check — should look identical (all lights = 255 = full bright)
```

**Step 6: Commit**

```
feat: add light_level to ChunkData and ATTRIBUTE_LIGHT to mesh builder
```

---

## Task 14: System Tests

Add key ECS system tests using the Bevy `App` pattern.

**Files:**
- Modify: `src/test_helpers.rs` — add `test_app()` builder
- Create tests in relevant modules (inline `#[cfg(test)]` modules)

**Step 1: Add `test_app()` to `src/test_helpers.rs`**

```rust
/// Create a minimal Bevy App with all world resources for system tests.
pub fn test_app() -> App {
    let wc = test_world_config();
    let bm = test_biome_map();
    let br = test_biome_registry();
    let tr = test_tile_registry();
    let pc = test_planet_config();

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(wc);
    app.insert_resource(bm);
    app.insert_resource(br);
    app.insert_resource(tr);
    app.insert_resource(pc);
    app.init_resource::<WorldMap>();
    app
}
```

**Step 2: Add collision system test** to `src/player/collision.rs`

```rust
#[cfg(test)]
mod system_tests {
    use super::*;
    use crate::test_helpers::fixtures;
    use bevy::app::App;

    #[test]
    fn collision_system_uses_read_only_world_map() {
        let mut app = fixtures::test_app();
        // Pre-generate a chunk so collision has data
        // ...setup player entity with position...
        app.add_systems(Update, collision_system);
        app.update();
        // Assert player didn't fall through, no new chunks generated
    }
}
```

**Step 3: Add camera follow test** to `src/camera/follow.rs`

**Step 4: Run tests**

```bash
cargo test
```

**Step 5: Commit**

```
test: add ECS system tests for collision, camera follow
```

---

## Task 15: Final Cleanup and Verification

Remove all remaining `#[allow(clippy::too_many_arguments)]` that are no longer needed. Run full verification.

**Files:**
- All files with `#[allow(clippy::too_many_arguments)]` — remove annotations where param count is now ≤7
- Any remaining cleanup

**Step 1: Remove unnecessary clippy allows**

Search for `#[allow(clippy::too_many_arguments)]` — remove each one where the function now has ≤7 parameters thanks to WorldCtx/WorldCtxRef.

**Step 2: Full verification**

```bash
cargo test
cargo clippy -D warnings
cargo run  # play test: walk around, break/place blocks, cross biome boundaries, check parallax
```

**Step 3: Commit**

```
refactor: remove unnecessary clippy::too_many_arguments allows after WorldCtx cleanup
```

---

## Task Dependencies

```
Task 1 (test fixtures)
├── Task 2 (AABB) — uses fixtures
├── Task 3 (WorldCtx) — uses fixtures
│   └── Task 4 (read/write split) — needs WorldCtx
│       └── Task 7 (Perlin cache) — needs WorldCtxRef
├── Task 5 (registry split) — independent
├── Task 6 (BiomeId) — independent, but easier after Task 3
│   └── Task 11 (ParallaxLayer split) — needs BiomeId
├── Task 8 (pub→pub(crate)) — after Tasks 3,6
├── Task 9 (SystemSets) — independent
│   └── Task 10 (camera plugin) — needs SystemSets
├── Task 12 (WorldLayer boundaries) — after Task 6
├── Task 13 (light prep) — independent
├── Task 14 (system tests) — after Tasks 1-4
└── Task 15 (final cleanup) — last
```

**Parallel batches:**
- Batch 1: Task 1
- Batch 2: Tasks 2, 5, 9 (parallel — independent)
- Batch 3: Tasks 3, 6 (parallel — both need fixtures but touch different files)
- Batch 4: Tasks 4, 8, 10, 11 (sequential or careful parallel)
- Batch 5: Tasks 7, 12, 13 (parallel — independent subsystems)
- Batch 6: Task 14 (system tests)
- Batch 7: Task 15 (final cleanup)
