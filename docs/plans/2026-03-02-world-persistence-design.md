# World Persistence Design

**Date:** 2026-03-02
**Status:** Approved
**Scope:** In-memory persistence with Serialize/Deserialize ready for disk

## Overview

When the player warps between planets, modified chunk data and dropped items are preserved in a global `Universe` resource keyed by `CelestialAddress`. Upon returning to a previously visited planet, saved modifications are restored on top of the regenerated procedural terrain. Dropped items use a 30-minute frozen timer (paused while player is on another planet).

## 1. CelestialAddress Migration (struct → enum)

Current struct-based address is replaced with an enum to support future location types:

```rust
#[derive(Clone, Hash, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum CelestialAddress {
    Planet { galaxy: IVec2, system: IVec2, orbit: u32 },
    Moon { galaxy: IVec2, system: IVec2, orbit: u32, satellite: u32 },
    Station { galaxy: IVec2, system: IVec2, station_id: u32 },
    Asteroid { galaxy: IVec2, system: IVec2, belt: u32, index: u32 },
    Ship { owner_id: u64 },
}
```

Convenience methods migrate to enum impl:
- `galaxy()`, `system()` — return `Option<IVec2>` (Ship has no galaxy)
- `orbit()` — return `Option<u32>`
- Seed derivation adapts to match on variant

Only `Planet` and `Moon` are used now. Other variants are defined but not constructed.

## 2. Core Data Structures

```rust
/// Saved state of a single world (planet, moon, station, etc.)
#[derive(Default, Serialize, Deserialize)]
pub struct WorldSave {
    /// Only chunks modified by the player (dirty).
    /// Unmodified chunks are regenerated from seed.
    pub chunks: HashMap<(i32, i32), ChunkData>,

    /// Dropped items on the ground.
    pub dropped_items: Vec<SavedDroppedItem>,

    /// Game time when the player left this world.
    /// Used for offline simulation (plant growth, mechanisms, dropped item timers).
    pub left_at: Option<f64>,
}

#[derive(Serialize, Deserialize)]
pub struct SavedDroppedItem {
    pub item_id: String,
    pub count: u16,
    pub x: f32,
    pub y: f32,
    pub remaining_secs: f32,  // max 1800.0 (30 minutes)
}

/// Global resource — persistence store for all visited worlds.
#[derive(Resource, Default, Serialize, Deserialize)]
pub struct Universe {
    pub planets: HashMap<CelestialAddress, WorldSave>,
}
```

## 3. Dirty Tracking

```rust
/// Set of chunk coordinates modified by the player on the current planet.
#[derive(Resource, Default)]
pub struct DirtyChunks(pub HashSet<(i32, i32)>);
```

Mark dirty at:
- `WorldMap::set_tile()` — tile dig/place (fg and bg)
- `place_object()` — object placement
- `remove_object()` — object removal
- Future: container contents change, block damage

Do NOT mark dirty on:
- Initial generation (`get_or_generate_chunk` with `or_insert_with`)
- Bitmask recalculation, lighting rebuild (rendering, not data)

## 4. Warp Flow with Persistence

```
InGame → WarpToBody message received
  │
  ├─ 1. SAVE current world
  │     a. For each coord in DirtyChunks:
  │        copy ChunkData from WorldMap → Universe.planets[old_address].chunks
  │     b. Collect all DroppedItem entities:
  │        serialize position, item_id, count, remaining timer
  │        → Universe.planets[old_address].dropped_items
  │     c. Set left_at = current game time
  │     d. Clear DirtyChunks
  │
  ├─ 2. DESPAWN everything
  │     a. Chunk tile entities (Query<Entity, With<ChunkCoord>>)
  │     b. Object entities (Query<Entity, With<PlacedObjectEntity>>)  ← NEW
  │     c. Dropped item entities (Query<Entity, With<DroppedItem>>)   ← NEW
  │     d. Parallax entities
  │     e. Clear WorldMap.chunks, LoadedChunks.map
  │
  ├─ 3. LOAD new world
  │     a. Insert ActiveWorld, TerrainNoiseCache, DayNight, etc.
  │     b. If Universe.planets[new_address] exists:
  │        - Pre-populate WorldMap with saved dirty chunks
  │        - Copy chunk coords into DirtyChunks
  │        - Compute elapsed = now - left_at
  │        - Filter dropped_items: remaining -= elapsed, discard if ≤ 0
  │     c. Insert LoadingBiomeAssets for new planet type
  │
  ├─ 4. State transition → LoadingBiomes → LoadingAutotile → InGame
  │
  └─ 5. OnEnter(InGame)
        a. Respawn saved dropped items as entities (with updated timers)
        b. chunk_loading_system generates fresh chunks via get_or_generate_chunk
           (pre-populated dirty chunks are found in HashMap, not regenerated)
        c. Player respawns on surface (NeedsRespawn)
```

### Key invariant

`get_or_generate_chunk` uses `entry().or_insert_with()` — if a saved chunk is already in `WorldMap`, generation is skipped. Saved modifications win over procedural baseline.

## 5. Dropped Items

### Changes from current behavior
- Timer: 5 minutes → **30 minutes** (1800 seconds)
- Timer **freezes** when player leaves the planet (not stored in ECS, stored as `remaining_secs`)
- Timer **resumes** on return: `remaining -= elapsed_since_left`
- Items with `remaining ≤ 0` after elapsed are discarded (never spawned)

### Serialization
Dropped items are free-floating ECS entities (not in ChunkData). On save:
1. Query all `(Entity, &DroppedItem, &Transform)` 
2. Map to `SavedDroppedItem { item_id, count, x, y, remaining_secs }`
3. Store in `WorldSave.dropped_items`

On load (OnEnter(InGame)):
1. Read `WorldSave.dropped_items`
2. Spawn entities with `DroppedItem`, `Transform`, physics components
3. Items spawn grounded (no velocity/bounce — they already settled)

## 6. Serde on Existing Types

Add `#[derive(Serialize, Deserialize)]` to:
- `ChunkData`, `TileLayer` — chunk tile data
- `PlacedObject`, `OccupancyRef`, `ObjectState` — placed objects
- `TileId`, `ObjectId` — newtype IDs
- `InventorySlot` — container contents
- `CelestialAddress` — persistence key (new enum)

All are simple structs/enums/newtypes — no custom serde logic needed.

## 7. Object Entity Despawn Fix

Current bug: `clear_stale_chunks` and `handle_warp` only despawn `ChunkCoord` entities. Object entities (`PlacedObjectEntity`) and dropped items (`DroppedItem`) survive warp and render on the new planet.

Fix: Add queries for `PlacedObjectEntity` and `DroppedItem` to the despawn step (step 2 above).

## 8. `left_at` and Offline Simulation

`left_at: Option<f64>` stores game time (`WorldTime.elapsed`) when the player leaves.

On return, `elapsed = current_time - left_at` is available for any system that needs offline simulation:
- **Now:** Dropped item timer countdown
- **Future:** Plant growth stages, mechanism output accumulation, NPC behavior

The offline simulation systems are NOT implemented now — only `left_at` is stored and `elapsed` is used for dropped item timers.

## 9. What Does NOT Persist

- **Unmodified terrain** — regenerated from seed (deterministic)
- **Bitmasks** — recomputed on chunk load
- **Lighting data** — recomputed each frame by RC pipeline
- **Player position** — always respawn on surface
- **Chunk rendering entities** — respawned by chunk_loading_system

## 10. Disk Serialization (Future)

All key types derive `Serialize + Deserialize`. Future disk save:
```rust
// Save
let bytes = bincode::serialize(&universe)?;
std::fs::write("save/universe.bin", bytes)?;

// Load
let bytes = std::fs::read("save/universe.bin")?;
let universe: Universe = bincode::deserialize(&bytes)?;
```

Not implemented in this phase. Architecture is ready.

## 11. Module Location

New file: `src/cosmos/persistence.rs`
- `Universe`, `WorldSave`, `SavedDroppedItem` types
- `DirtyChunks` resource
- `save_current_world()` system/function
- `load_world_save()` system/function
- `respawn_saved_dropped_items()` system (OnEnter(InGame))

Integrates with existing `src/cosmos/warp.rs` (`handle_warp`).
