# World Persistence Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Persist world modifications (dirty chunks + dropped items) across planet warps, fix object entity leak on warp, migrate CelestialAddress to enum.

**Architecture:** In-memory `Universe` resource keyed by `CelestialAddress` (enum). On warp: save dirty chunks + dropped items → clear → pre-populate new world from save. Dropped items have 30-min frozen timer. All key types derive Serialize/Deserialize for future disk persistence.

**Tech Stack:** Rust, Bevy 0.18 (Message system, not Event), serde, serde with derive

**Design doc:** `docs/plans/2026-03-02-world-persistence-design.md`

---

### Task 1: Add serde derives to core data types

**Files:**
- Modify: `src/object/definition.rs` — add `Serialize` to `ObjectId`
- Modify: `src/object/placed.rs` — add `Serialize, Deserialize` to `PlacedObject`, `OccupancyRef`, `ObjectState`
- Modify: `src/registry/tile.rs` — add `Serialize, Deserialize` to `TileId`
- Modify: `src/world/chunk.rs` — add `Serialize, Deserialize` to `ChunkData`, `TileLayer`
- Modify: `src/inventory/components.rs` — add `Serialize, Deserialize` to `Stack`

**Step 1: Add serde derives**

In `src/object/definition.rs`, add `Serialize` import and derive:
```rust
use serde::{Deserialize, Serialize};  // Deserialize already imported

// ObjectId — add Serialize, Deserialize
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ObjectId(pub u16);
```

In `src/registry/tile.rs`, add `Serialize` to existing `serde::Deserialize` import:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct TileId(pub u16);
```

In `src/object/placed.rs`:
```rust
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OccupancyRef { ... }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum ObjectState { ... }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacedObject { ... }
```

In `src/world/chunk.rs`:
```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct TileLayer { ... }

#[derive(Serialize, Deserialize)]
pub struct ChunkData { ... }
```

In `src/inventory/components.rs`:
```rust
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stack { ... }
```

**Step 2: Build to verify**

Run: `cargo build 2>&1 | head -30`
Expected: compiles without errors. Serde derives on these types should work since all fields are basic types.

**Step 3: Run tests**

Run: `cargo test 2>&1 | tail -5`
Expected: all 229 tests pass (no behavior change).

**Step 4: Commit**

```
feat: add Serialize/Deserialize derives to core data types

Prepares ChunkData, TileLayer, TileId, ObjectId, PlacedObject,
OccupancyRef, ObjectState, and Stack for world persistence.
```

---

### Task 2: Migrate CelestialAddress from struct to enum

**Files:**
- Modify: `src/cosmos/address.rs` — rewrite `CelestialAddress` as enum + helper methods
- Modify: `src/cosmos/generation.rs` — update all `CelestialAddress { ... }` constructors
- Modify: `src/cosmos/warp.rs` — update field access (`.orbit` → method call)
- Modify: `src/registry/world.rs` — update test constructor
- Modify: `src/test_helpers.rs` — update test fixture constructor
- Modify: `src/ui/star_map.rs` — update `.address.orbit` access

**Step 1: Rewrite CelestialAddress**

In `src/cosmos/address.rs`, replace the struct with:

```rust
use serde::{Serialize, Deserialize};

#[derive(Clone, Hash, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum CelestialAddress {
    Planet { galaxy: IVec2, system: IVec2, orbit: u32 },
    Moon { galaxy: IVec2, system: IVec2, orbit: u32, satellite: u32 },
    Station { galaxy: IVec2, system: IVec2, station_id: u32 },
    Asteroid { galaxy: IVec2, system: IVec2, belt: u32, index: u32 },
    Ship { owner_id: u64 },
}

impl CelestialAddress {
    /// Convenience constructor for a planet (most common case).
    pub fn planet(galaxy: IVec2, system: IVec2, orbit: u32) -> Self {
        Self::Planet { galaxy, system, orbit }
    }

    /// Convenience constructor for a moon.
    pub fn moon(galaxy: IVec2, system: IVec2, orbit: u32, satellite: u32) -> Self {
        Self::Moon { galaxy, system, orbit, satellite }
    }

    /// Galaxy coordinates (None for Ship).
    pub fn galaxy(&self) -> Option<IVec2> {
        match self {
            Self::Planet { galaxy, .. }
            | Self::Moon { galaxy, .. }
            | Self::Station { galaxy, .. }
            | Self::Asteroid { galaxy, .. } => Some(*galaxy),
            Self::Ship { .. } => None,
        }
    }

    /// System coordinates (None for Ship).
    pub fn system(&self) -> Option<IVec2> {
        match self {
            Self::Planet { system, .. }
            | Self::Moon { system, .. }
            | Self::Station { system, .. }
            | Self::Asteroid { system, .. } => Some(*system),
            Self::Ship { .. } => None,
        }
    }

    /// Orbit index (Planet, Moon only).
    pub fn orbit(&self) -> Option<u32> {
        match self {
            Self::Planet { orbit, .. } | Self::Moon { orbit, .. } => Some(*orbit),
            _ => None,
        }
    }

    /// Satellite index (Moon only).
    pub fn satellite(&self) -> Option<u32> {
        match self {
            Self::Moon { satellite, .. } => Some(*satellite),
            _ => None,
        }
    }
}
```

**Step 2: Update CelestialSeeds::derive**

Adapt the derive function to match on enum variants:

```rust
pub fn derive(universe_seed: u64, address: &CelestialAddress) -> Self {
    let (galaxy, system, orbit, satellite) = match address {
        CelestialAddress::Planet { galaxy, system, orbit } => (*galaxy, *system, *orbit, None),
        CelestialAddress::Moon { galaxy, system, orbit, satellite } => (*galaxy, *system, *orbit, Some(*satellite)),
        // For station/asteroid/ship — derive from available fields
        CelestialAddress::Station { galaxy, system, station_id } => (*galaxy, *system, *station_id, None),
        CelestialAddress::Asteroid { galaxy, system, belt, index } => (*galaxy, *system, *belt, Some(*index)),
        CelestialAddress::Ship { owner_id } => (IVec2::ZERO, IVec2::ZERO, *owner_id as u32, None),
    };

    let galaxy_seed = hash_combine(universe_seed, pack_coords(galaxy.x, galaxy.y));
    let system_seed = hash_combine(galaxy_seed, pack_coords(system.x, system.y));
    let star_seed = hash_tag(system_seed, "star");
    let planet_seed = hash_combine(system_seed, orbit as u64);

    let body_seed = match satellite {
        Some(sat) => hash_combine(planet_seed, sat as u64),
        None => planet_seed,
    };

    // ... rest unchanged
}
```

**Step 3: Update all construction sites**

In `src/cosmos/generation.rs`, replace:
```rust
// Old:
CelestialAddress { galaxy, system, orbit: 0, satellite: None }
// New:
CelestialAddress::planet(galaxy, system, 0)

// Old:
CelestialAddress { galaxy, system, orbit, satellite: None }
// New:
CelestialAddress::planet(galaxy, system, orbit)
```

In `src/cosmos/warp.rs`, replace field access:
```rust
// Old:
b.address.orbit == warp.orbit
// New:
b.address.orbit() == Some(warp.orbit)

// Old:
body.address.orbit
// New:
body.address.orbit().unwrap_or(0)
```

In `src/registry/world.rs` test:
```rust
// Old:
CelestialAddress { galaxy: IVec2::ZERO, system: IVec2::ZERO, orbit: 2, satellite: None }
// New:
CelestialAddress::planet(IVec2::ZERO, IVec2::ZERO, 2)
```

In `src/test_helpers.rs`:
```rust
// Same as above — use CelestialAddress::planet(...)
```

In `src/ui/star_map.rs`:
```rust
// Old:
active_world.address.orbit
body.address.orbit
// New:
active_world.address.orbit().unwrap_or(0)
body.address.orbit().unwrap_or(0)
```

**Step 4: Update tests in address.rs**

All `CelestialAddress { ... }` constructors in tests → `CelestialAddress::planet(...)` or `CelestialAddress::moon(...)`.

**Step 5: Build and run tests**

Run: `cargo build 2>&1 | head -30`
Run: `cargo test 2>&1 | tail -5`
Expected: all tests pass.

**Step 6: Commit**

```
refactor: migrate CelestialAddress from struct to enum

Support future location types (Station, Asteroid, Ship) alongside
Planet and Moon. Add convenience constructors and accessor methods.
```

---

### Task 3: Create persistence module with Universe, WorldSave, DirtyChunks

**Files:**
- Create: `src/cosmos/persistence.rs`
- Modify: `src/cosmos/mod.rs` — add `pub mod persistence;`

**Step 1: Write the persistence module**

Create `src/cosmos/persistence.rs`:

```rust
//! World persistence — saves and restores per-planet modifications across warps.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::address::CelestialAddress;
use crate::world::chunk::ChunkData;

/// A dropped item serialized for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedDroppedItem {
    pub item_id: String,
    pub count: u16,
    pub x: f32,
    pub y: f32,
    /// Remaining lifetime in seconds (max 1800.0 = 30 minutes).
    pub remaining_secs: f32,
}

/// Saved state of a single world (planet, moon, station, etc.).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct WorldSave {
    /// Only chunks modified by the player (dirty).
    pub chunks: HashMap<(i32, i32), ChunkData>,
    /// Dropped items on the ground.
    pub dropped_items: Vec<SavedDroppedItem>,
    /// Game time when the player left this world (for offline simulation).
    pub left_at: Option<f64>,
}

/// Global persistence store for all visited worlds.
#[derive(Resource, Debug, Default, Serialize, Deserialize)]
pub struct Universe {
    pub planets: HashMap<CelestialAddress, WorldSave>,
}

/// Tracks which chunks have been modified by the player on the current planet.
#[derive(Resource, Debug, Default)]
pub struct DirtyChunks(pub HashSet<(i32, i32)>);

/// Maximum lifetime for dropped items (30 minutes).
pub const DROPPED_ITEM_LIFETIME_SECS: f32 = 1800.0;
```

**Step 2: Register module**

In `src/cosmos/mod.rs`, add:
```rust
pub mod persistence;
```

**Step 3: Build and test**

Run: `cargo build 2>&1 | head -20`
Run: `cargo test 2>&1 | tail -5`
Expected: compiles and all tests pass.

**Step 4: Commit**

```
feat: add persistence module with Universe, WorldSave, DirtyChunks

Core data structures for per-planet world persistence. Universe is a
HashMap<CelestialAddress, WorldSave> holding dirty chunks and dropped
items. All types derive Serialize/Deserialize for future disk save.
```

---

### Task 4: Wire DirtyChunks tracking into modification points

**Files:**
- Modify: `src/world/chunk.rs` — `set_tile()` marks dirty
- Modify: `src/object/placement.rs` — `place_object()` and `remove_object()` mark dirty
- Modify: `src/world/mod.rs` — register `DirtyChunks` resource
- Modify: `src/interaction/block_action.rs` — pass `DirtyChunks` to systems, mark dirty on tile/object changes

**Step 1: Approach decision**

The cleanest approach: since `set_tile()` is called via `WorldMap` methods and `place_object`/`remove_object` are standalone functions in `placement.rs`, the most minimal change is to have the caller (`block_interaction_system`) mark chunks as dirty after mutations, rather than threading `DirtyChunks` through every utility function.

In `src/interaction/block_action.rs`, `block_interaction_system` already knows the chunk coords after every modification (it computes them for bitmask updates and `ChunkDirty` entity marking). Add `ResMut<DirtyChunks>` to the system signature and insert the data chunk coord after each `set_tile()`, `place_object()`, or `remove_object()`.

**Step 2: Register resource**

In `src/world/mod.rs`, add:
```rust
use crate::cosmos::persistence::DirtyChunks;
// In Plugin::build:
.init_resource::<DirtyChunks>()
```

**Step 3: Mark dirty in block_action.rs**

Add `mut dirty_chunks: ResMut<DirtyChunks>` to `block_interaction_system` signature.

After every `world_map.set_tile(tx, ty, ...)` call, add:
```rust
let data_cx = ctx_ref.config.wrap_chunk_x(tile_to_chunk(wrapped_tx, ty, ctx_ref.config.chunk_size).0);
dirty_chunks.0.insert((data_cx, chunk_cy));
```

After every `place_object()` / `remove_object()` call, add similar dirty marking for the anchor chunk.

**Step 4: Build and test**

Run: `cargo build 2>&1 | head -20`
Run: `cargo test 2>&1 | tail -5`
Expected: all tests pass. No behavior change — we're only tracking, not using the data yet.

**Step 5: Commit**

```
feat: wire DirtyChunks tracking into block/object modification points

block_interaction_system now marks chunk coordinates in DirtyChunks
whenever tiles are dug/placed or objects are placed/removed.
```

---

### Task 5: Implement save_current_world and load_world_save

**Files:**
- Modify: `src/cosmos/persistence.rs` — add save/load functions
- Modify: `src/item/dropped_item.rs` — change lifetime to 1800s, add public constant

**Step 1: Update dropped item lifetime**

In `src/item/dropped_item.rs`, change the Timer in tests and find where `300.0` is used for lifetime. Search for `300` in `block_action.rs` where `DroppedItem` is created and change to `DROPPED_ITEM_LIFETIME_SECS` (1800.0).

**Step 2: Add save/load functions to persistence.rs**

```rust
use crate::item::dropped_item::DroppedItem;
use crate::world::chunk::WorldMap;
use crate::world::day_night::WorldTime;

/// Save the current world's dirty chunks and dropped items into Universe.
pub fn save_current_world(
    universe: &mut Universe,
    address: &CelestialAddress,
    world_map: &WorldMap,
    dirty_chunks: &DirtyChunks,
    dropped_items: &[(String, u16, f32, f32, f32)], // (item_id, count, x, y, remaining_secs)
    game_time: f64,
) {
    let save = universe.planets.entry(address.clone()).or_default();

    // Save dirty chunks
    save.chunks.clear();
    for &(cx, cy) in &dirty_chunks.0 {
        if let Some(chunk_data) = world_map.chunks.get(&(cx, cy)) {
            save.chunks.insert((cx, cy), chunk_data.clone());
        }
    }

    // Save dropped items
    save.dropped_items = dropped_items
        .iter()
        .map(|(id, count, x, y, remaining)| SavedDroppedItem {
            item_id: id.clone(),
            count: *count,
            x: *x,
            y: *y,
            remaining_secs: *remaining,
        })
        .collect();

    save.left_at = Some(game_time);
}

/// Pre-populate WorldMap with saved dirty chunks for a world.
/// Returns saved dropped items (with elapsed time subtracted) to spawn later.
pub fn load_world_save(
    universe: &Universe,
    address: &CelestialAddress,
    world_map: &mut WorldMap,
    dirty_chunks: &mut DirtyChunks,
    current_game_time: f64,
) -> Vec<SavedDroppedItem> {
    dirty_chunks.0.clear();

    let Some(save) = universe.planets.get(address) else {
        return Vec::new();
    };

    // Pre-populate dirty chunks into WorldMap
    for (&coords, chunk_data) in &save.chunks {
        world_map.chunks.insert(coords, chunk_data.clone());
        dirty_chunks.0.insert(coords);
    }

    // Compute elapsed time and filter dropped items
    let elapsed = save
        .left_at
        .map(|t| (current_game_time - t) as f32)
        .unwrap_or(0.0)
        .max(0.0);

    save.dropped_items
        .iter()
        .filter_map(|item| {
            let remaining = item.remaining_secs - elapsed;
            if remaining > 0.0 {
                Some(SavedDroppedItem {
                    remaining_secs: remaining,
                    ..item.clone()
                })
            } else {
                None
            }
        })
        .collect()
}
```

**Step 3: Add unit tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::IVec2;

    fn test_address() -> CelestialAddress {
        CelestialAddress::planet(IVec2::ZERO, IVec2::ZERO, 2)
    }

    #[test]
    fn save_and_load_dirty_chunks() {
        let mut universe = Universe::default();
        let mut world_map = WorldMap::default();
        let mut dirty_chunks = DirtyChunks::default();
        let addr = test_address();

        // Simulate a dirty chunk by inserting data
        // (In real code, get_or_generate_chunk + set_tile would do this)
        // We'll just test the save/load flow with an empty ChunkData-like structure
        // ... (test the round-trip)
    }

    #[test]
    fn load_subtracts_elapsed_from_dropped_items() {
        let mut universe = Universe::default();
        let addr = test_address();
        universe.planets.insert(addr.clone(), WorldSave {
            chunks: HashMap::new(),
            dropped_items: vec![
                SavedDroppedItem { item_id: "dirt".into(), count: 5, x: 100.0, y: 200.0, remaining_secs: 600.0 },
                SavedDroppedItem { item_id: "stone".into(), count: 1, x: 50.0, y: 50.0, remaining_secs: 100.0 },
            ],
            left_at: Some(1000.0),
        });

        let mut world_map = WorldMap::default();
        let mut dirty = DirtyChunks::default();

        // 200 seconds elapsed
        let items = load_world_save(&universe, &addr, &mut world_map, &mut dirty, 1200.0);
        assert_eq!(items.len(), 1); // stone expired (100 - 200 < 0)
        assert_eq!(items[0].item_id, "dirt");
        assert!((items[0].remaining_secs - 400.0).abs() < 0.1);
    }

    #[test]
    fn load_nonexistent_world_returns_empty() {
        let universe = Universe::default();
        let addr = test_address();
        let mut world_map = WorldMap::default();
        let mut dirty = DirtyChunks::default();
        let items = load_world_save(&universe, &addr, &mut world_map, &mut dirty, 0.0);
        assert!(items.is_empty());
        assert!(dirty.0.is_empty());
    }
}
```

**Step 4: Build and test**

Run: `cargo test cosmos::persistence 2>&1`
Run: `cargo test 2>&1 | tail -5`

**Step 5: Commit**

```
feat: implement save_current_world and load_world_save

Saves dirty chunks and dropped items into Universe on warp-out.
Restores dirty chunks into WorldMap on warp-in, subtracts elapsed
time from dropped item timers, discards expired items.
```

---

### Task 6: Integrate persistence into handle_warp + fix entity despawn bugs

**Files:**
- Modify: `src/cosmos/warp.rs` — add save/load calls, despawn objects + dropped items
- Modify: `src/ui/mod.rs` — register `Universe` resource
- Modify: `src/world/mod.rs` — register `Universe` if not already done
- Modify: `src/world/chunk.rs` — `clear_stale_chunks` also despawns `PlacedObjectEntity` and `DroppedItem`

**Step 1: Update handle_warp**

Add to the system signature:
```rust
mut universe: ResMut<Universe>,
dirty_chunks: Res<DirtyChunks>,
dropped_item_query: Query<(&DroppedItem, &Transform)>,
object_entities: Query<Entity, With<PlacedObjectEntity>>,
dropped_entities: Query<Entity, With<DroppedItem>>,
world_time: Res<WorldTime>,
active_world: Res<ActiveWorld>,
```

Before clearing world data (step 3 in current code), add:
```rust
// --- SAVE current world ---
let game_time = world_time.elapsed;
let dropped_items: Vec<_> = dropped_item_query
    .iter()
    .map(|(item, transform)| {
        (
            item.item_id.clone(),
            item.count,
            transform.translation.x,
            transform.translation.y,
            item.lifetime.remaining_secs(),
        )
    })
    .collect();
save_current_world(
    &mut universe,
    &active_world.address,
    &world_map,
    &dirty_chunks,
    &dropped_items,
    game_time,
);
```

After clearing world data, before inserting new ActiveWorld:
```rust
// --- Despawn object entities (BUG FIX) ---
for entity in &object_entities {
    commands.entity(entity).despawn();
}
// --- Despawn dropped item entities ---
for entity in &dropped_entities {
    commands.entity(entity).despawn();
}
```

After inserting new ActiveWorld, add load step. NOTE: we cannot pre-populate WorldMap here because the new ActiveWorld is inserted via deferred commands. Instead, store the loaded items in a resource for OnEnter(InGame) to spawn. The dirty chunks can be loaded in `clear_stale_chunks` (which runs OnEnter(LoadingBiomes) after deferred commands applied).

Better approach: create a `PendingWorldLoad` resource:
```rust
#[derive(Resource)]
pub struct PendingWorldLoad {
    pub address: CelestialAddress,
    pub dropped_items: Vec<SavedDroppedItem>,
}
```

Insert it during handle_warp. Then in `clear_stale_chunks` (or a new system on OnEnter(LoadingBiomes)), load the dirty chunks. In OnEnter(InGame), spawn dropped items.

**Step 2: Update clear_stale_chunks**

In `src/world/chunk.rs`, expand `clear_stale_chunks`:
```rust
pub fn clear_stale_chunks(
    mut commands: Commands,
    mut world_map: ResMut<WorldMap>,
    mut loaded_chunks: ResMut<LoadedChunks>,
    chunk_entities: Query<Entity, With<ChunkCoord>>,
    object_entities: Query<Entity, With<PlacedObjectEntity>>,
    dropped_entities: Query<Entity, With<DroppedItem>>,
    universe: Res<Universe>,
    active_world: Res<ActiveWorld>,
    mut dirty_chunks: ResMut<DirtyChunks>,
) {
    world_map.chunks.clear();
    loaded_chunks.map.clear();
    dirty_chunks.0.clear();

    for entity in &chunk_entities {
        commands.entity(entity).despawn();
    }
    for entity in &object_entities {
        commands.entity(entity).despawn();
    }
    for entity in &dropped_entities {
        commands.entity(entity).despawn();
    }

    // Pre-populate WorldMap with saved dirty chunks for the new world
    if let Some(save) = universe.planets.get(&active_world.address) {
        for (&coords, chunk_data) in &save.chunks {
            world_map.chunks.insert(coords, chunk_data.clone());
            dirty_chunks.0.insert(coords);
        }
    }
}
```

**Step 3: Create system to respawn saved dropped items OnEnter(InGame)**

Add to `src/cosmos/persistence.rs`:
```rust
/// Resource holding dropped items to respawn after warp.
#[derive(Resource, Default)]
pub struct PendingDroppedItems(pub Vec<SavedDroppedItem>);

/// System: spawns saved dropped items on world entry.
pub fn respawn_saved_dropped_items(
    mut commands: Commands,
    pending: Option<Res<PendingDroppedItems>>,
    // ... sprite/mesh resources needed for spawning
) {
    let Some(pending) = pending else { return; };
    for item in &pending.0 {
        // Spawn each item as a DroppedItem entity at saved position
        // with remaining timer, grounded (no velocity)
    }
    commands.remove_resource::<PendingDroppedItems>();
}
```

The actual spawn logic should reuse the existing dropped item spawning code from `block_action.rs::spawn_tile_drops`, but spawning grounded (no velocity, no bounce).

**Step 4: Register systems and resources**

In `src/ui/mod.rs` or `src/cosmos/mod.rs` (wherever appropriate):
```rust
app.init_resource::<Universe>()
   .init_resource::<DirtyChunks>()
   .add_systems(OnEnter(AppState::InGame), respawn_saved_dropped_items);
```

**Step 5: Store PendingDroppedItems during handle_warp**

In handle_warp, after saving current world and computing new world data:
```rust
let pending_items = if let Some(save) = universe.planets.get(&new_address) {
    let elapsed = save.left_at
        .map(|t| (world_time.elapsed - t) as f32)
        .unwrap_or(0.0)
        .max(0.0);
    save.dropped_items.iter().filter_map(|item| {
        let remaining = item.remaining_secs - elapsed;
        if remaining > 0.0 { Some(SavedDroppedItem { remaining_secs: remaining, ..item.clone() }) }
        else { None }
    }).collect()
} else {
    Vec::new()
};
commands.insert_resource(PendingDroppedItems(pending_items));
```

**Step 6: Build and test**

Run: `cargo build 2>&1 | head -30`
Run: `cargo test 2>&1 | tail -5`

**Step 7: Commit**

```
feat: integrate persistence into warp flow + fix entity despawn bugs

- Save dirty chunks + dropped items to Universe before warp
- Despawn object entities and dropped items during warp (fixes leak)
- Pre-populate WorldMap with saved chunks in clear_stale_chunks
- Respawn saved dropped items OnEnter(InGame) with elapsed time applied
```

---

### Task 7: Implement respawn_saved_dropped_items system

**Files:**
- Modify: `src/cosmos/persistence.rs` — implement the actual spawn logic
- Modify: `src/interaction/block_action.rs` — extract dropped item spawn helper (if not already shared)

**Step 1: Implement respawn system**

The system needs to create DroppedItem entities at saved positions. Unlike fresh drops (which have velocity/bounce), saved items spawn grounded and stationary.

```rust
pub fn respawn_saved_dropped_items(
    mut commands: Commands,
    pending: Option<Res<PendingDroppedItems>>,
    item_registry: Res<ItemRegistry>,
    object_sprites: Option<Res<ObjectSpriteMaterials>>,
    quad: Option<Res<SharedLitQuad>>,
    mut lit_materials: ResMut<Assets<LitSpriteMaterial>>,
) {
    let Some(pending) = pending else { return; };
    if pending.0.is_empty() {
        commands.remove_resource::<PendingDroppedItems>();
        return;
    }

    for saved in &pending.0 {
        // Create entity with DroppedItem + Transform + physics (grounded)
        let position = Vec3::new(saved.x, saved.y, 1.0);
        commands.spawn((
            DroppedItem {
                item_id: saved.item_id.clone(),
                count: saved.count,
                lifetime: Timer::from_seconds(saved.remaining_secs, TimerMode::Once),
            },
            Transform::from_translation(position),
            Visibility::default(),
            // Physics — grounded, no velocity
            Grounded(true),
            Friction(0.9),
            TileCollider { width: 4.0, height: 4.0 },
            Gravity(400.0),
            Velocity(Vec2::ZERO),
            Bounce(0.3),
            // Sprite rendering — use same approach as spawn_tile_drops
        ));
    }

    commands.remove_resource::<PendingDroppedItems>();
}
```

Note: The exact sprite setup depends on how `spawn_tile_drops` builds the visual. Look at `block_action.rs` for the full component bundle and mirror it (minus the random velocity).

**Step 2: Register OnEnter(InGame)**

Already covered in Task 6 registration step, but verify it's wired up.

**Step 3: Build and manual test**

Run: `cargo build 2>&1 | head -20`
Manual test: place torch on planet A, warp to planet B, warp back to A — torch should be there. Drop item, warp away, warp back — item should be there with reduced timer.

**Step 4: Commit**

```
feat: implement respawn_saved_dropped_items system

Spawns persisted dropped items as grounded entities on world entry,
with remaining timer from save data (elapsed time already subtracted).
```

---

### Task 8: Update dropped item lifetime to 30 minutes

**Files:**
- Modify: `src/interaction/block_action.rs` — change Timer::from_seconds(300.0, ...) to use constant
- Modify: `src/item/dropped_item.rs` — update test to use 1800

**Step 1: Find and replace lifetime**

Search for `300.0` in block_action.rs (the DroppedItem creation site) and replace with `crate::cosmos::persistence::DROPPED_ITEM_LIFETIME_SECS`.

In `dropped_item.rs` test, update:
```rust
lifetime: Timer::from_seconds(1800.0, TimerMode::Once),
```

**Step 2: Build and test**

Run: `cargo test 2>&1 | tail -5`

**Step 3: Commit**

```
feat: increase dropped item lifetime from 5 to 30 minutes

Timer freezes while player is on another planet (handled by
persistence system subtracting only real elapsed time on return).
```

---

### Task 9: Write integration tests

**Files:**
- Modify: `src/cosmos/persistence.rs` — add comprehensive tests

**Step 1: Write tests**

```rust
#[test]
fn round_trip_dirty_chunks() {
    // Create world, modify a chunk, save, clear, load — verify chunk restored
}

#[test]
fn save_preserves_placed_objects_in_chunks() {
    // Place an object in a chunk, mark dirty, save, load — verify object exists
}

#[test]
fn dropped_items_timer_frozen_while_away() {
    // Save with remaining=600, left_at=1000. Load at time=1000 (0 elapsed).
    // Verify remaining still 600.
}

#[test]
fn dropped_items_expire_after_elapsed() {
    // Save with remaining=100, left_at=1000. Load at time=1200 (200 elapsed).
    // Verify item is gone.
}

#[test]
fn multiple_worlds_independent() {
    // Save world A, save world B, load world A — verify only A's data restored
}

#[test]
fn unvisited_world_returns_empty() {
    // Load a world that was never saved — empty, no crash
}

#[test]
fn dirty_chunks_cleared_on_load() {
    // Load a world — dirty_chunks should contain only the saved chunk coords
}
```

**Step 2: Run tests**

Run: `cargo test cosmos::persistence 2>&1`
Run: `cargo test 2>&1 | tail -5`

**Step 3: Commit**

```
test: add comprehensive tests for world persistence

Tests round-trip save/load, dropped item timer behavior, multi-world
independence, and edge cases.
```

---

## Summary of tasks

| Task | Description | Dependencies |
|------|-------------|-------------|
| 1 | Serde derives on core types | None |
| 2 | CelestialAddress struct→enum migration | None |
| 3 | Persistence module (Universe, WorldSave, DirtyChunks) | 1, 2 |
| 4 | Wire DirtyChunks tracking | 3 |
| 5 | save_current_world + load_world_save functions | 3 |
| 6 | Integrate into handle_warp + fix entity despawn | 4, 5 |
| 7 | respawn_saved_dropped_items system | 6 |
| 8 | Update dropped item lifetime to 30 min | 3 |
| 9 | Integration tests | 7, 8 |

Tasks 1 and 2 can run in parallel. Tasks 4 and 5 can run in parallel. Task 8 is independent after task 3.
