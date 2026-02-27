# Biome System Design

**Date:** 2026-02-27
**Status:** Approved
**Inspired by:** Starbound's biome architecture (full model, adapted to RON)

## Overview

Data-driven biome system with planet types, horizontal region distribution, 4 vertical layers, per-biome parallax backgrounds with crossfade transitions. MVP scope: tile selection + parallax switching.

## 1. Data Model

### Hierarchy

```
PlanetType → defines which biomes appear
  ├── primary_biome (dominant, ~60% of surface)
  ├── secondary_biomes (remaining ~40%)
  └── layers (4 vertical layers, each with its own biome)
        ├── surface (biome from BiomeMap by X)
        ├── underground
        ├── deep_underground
        └── core

Biome → defines terrain content + visuals
  ├── tile palette (surface_block, subsurface_block, fill_block)
  ├── parallax config (per-biome background layers)
  └── future: weather, music, placeables, monsters (Option fields)
```

### File Structure

```
assets/world/
├── world.config.ron                    # + planet_type: "garden"
├── tiles.registry.ron                  # unchanged — shared tile pool
├── terrain/                            # unchanged — autotile spritesheets
├── planet_types/
│   └── garden.planet.ron
└── biomes/
    ├── meadow/
    │   ├── meadow.biome.ron
    │   ├── parallax.ron
    │   └── backgrounds/                # parallax PNGs
    ├── forest/
    │   ├── forest.biome.ron
    │   ├── parallax.ron
    │   └── backgrounds/
    ├── rocky/
    │   ├── rocky.biome.ron
    │   ├── parallax.ron
    │   └── backgrounds/
    ├── underground_dirt/
    │   └── underground_dirt.biome.ron  # no parallax (underground)
    ├── underground_rock/
    │   └── underground_rock.biome.ron
    └── core_magma/
        └── core_magma.biome.ron
```

### Planet Type Format (`garden.planet.ron`)

```ron
PlanetType(
    id: "garden",
    primary_biome: "meadow",
    secondary_biomes: ["forest", "rocky"],
    layers: Layers(
        surface: LayerConfig(
            primary_biome: None,  // inherits from planet primary/secondary
            terrain_frequency: 0.02,
            terrain_amplitude: 40.0,
        ),
        underground: LayerConfig(
            primary_biome: Some("underground_dirt"),
            terrain_frequency: 0.07,
            terrain_amplitude: 1.0,
        ),
        deep_underground: LayerConfig(
            primary_biome: Some("underground_rock"),
            terrain_frequency: 0.05,
            terrain_amplitude: 1.0,
        ),
        core: LayerConfig(
            primary_biome: Some("core_magma"),
            terrain_frequency: 0.04,
            terrain_amplitude: 1.0,
        ),
    ),
    region_width_min: 300,
    region_width_max: 600,
    primary_region_ratio: 0.6,
)
```

### Biome Format (`meadow.biome.ron`)

```ron
Biome(
    id: "meadow",
    surface_block: "grass",
    subsurface_block: "dirt",
    subsurface_depth: 4,
    fill_block: "stone",
    cave_threshold: 0.3,
    parallax: "biomes/meadow/parallax.ron",
    // Future fields — Option, not implemented in MVP
    weather: None,
    music: None,
    ambient: None,
    placeables: None,
    monsters: None,
    status_effects: None,
)
```

### Biome Parallax Format (`biomes/meadow/parallax.ron`)

Same format as existing `parallax.ron`, now per-biome:

```ron
ParallaxConfig(
    layers: [
        (name: "sky", image: "biomes/meadow/backgrounds/sky.png", speed_x: 0.0, speed_y: 0.0, repeat_x: false, repeat_y: false, z_order: -100),
        (name: "far_hills", image: "biomes/meadow/backgrounds/far_hills.png", speed_x: 0.1, speed_y: 0.05, repeat_x: true, repeat_y: false, z_order: -90),
        (name: "near_hills", image: "biomes/meadow/backgrounds/near_hills.png", speed_x: 0.3, speed_y: 0.15, repeat_x: true, repeat_y: false, z_order: -80),
    ],
)
```

## 2. Region Generation

### Algorithm

Biomes are distributed horizontally as contiguous **regions** of random width:

```
|<-- meadow (450) -->|<-- forest (320) -->|<-- meadow (380) -->|<-- rocky (500) -->|<-- meadow (398) -->|
0                   450                  770                 1150                1650                2048
```

Steps:
1. Create deterministic RNG from world seed
2. Compute region count: `width_tiles / avg_region_width`
3. Allocate slots: ~60% → primary biome, rest → random secondary
4. Each region width = `rng.range(region_width_min..region_width_max)`
5. Adjust last region so total = `width_tiles`
6. No two adjacent regions share the same biome (shuffle if needed)
7. Cylindrical wrap: first and last region must differ

### Data Structures

```rust
pub struct BiomeMap {
    regions: Vec<BiomeRegion>,  // sorted by start_x
}

pub struct BiomeRegion {
    pub biome_id: String,
    pub start_x: u32,
    pub width: u32,
}

impl BiomeMap {
    /// O(log n) binary search by start_x
    pub fn biome_at(&self, tile_x: u32) -> &str;

    /// Generate from PlanetType + seed
    pub fn generate(planet_type: &PlanetType, seed: u64, world_width: u32) -> Self;
}
```

### Vertical Layers

Layer determined by `tile_y` relative to world height:

```
tile_y: 0 (bottom)                        tile_y: 1024 (top)
|<-- Core -->|<-- Deep UG -->|<-- Underground -->|<-- Surface -->|  air
0           128             384                 716            1024
     12%          25%              32%              ~30%
```

Layer boundaries defined in `PlanetType` (or default proportions).

Tile generation:
- Surface layer → biome from `BiomeMap` by `tile_x`
- Underground/Deep/Core → biome from `LayerConfig.primary_biome`

## 3. Parallax Switching

### Player Biome Tracking

```rust
pub struct CurrentBiome {
    pub biome_id: String,
    pub region_index: usize,
}
```

System `track_player_biome` (runs after `camera_follow_player`):
1. Get player `tile_x`
2. `biome_map.biome_at(tile_x)` → current biome
3. If biome changed → start transition

### Transition State

```rust
pub struct ParallaxTransition {
    pub from_biome: String,
    pub to_biome: String,
    pub progress: f32,      // 0.0 → 1.0
    pub duration: f32,       // seconds (1.5)
}
```

### Rendering During Transition

Two sets of parallax layers active simultaneously:
- `from` layers: alpha = `1.0 - progress`
- `to` layers: alpha = `progress`

When `progress >= 1.0`:
- Despawn `from` layers
- Remove `ParallaxTransition` resource
- `to` layers become current

### System Ordering

```
camera_follow_player
    → track_player_biome
    → parallax_transition
    → parallax_scroll
```

### Edge Case: Fast Travel

If player crosses 2+ biomes during one transition (teleport or small region):
- Cancel current transition
- Despawn `from` layers immediately
- Start new transition from current `to` layers to new biome

## 4. Asset Pipeline

### New Asset Types

| File | Asset struct | Loader extension |
|------|-------------|-----------------|
| `*.planet.ron` | `PlanetTypeAsset` | `.planet.ron` |
| `*.biome.ron` | `BiomeAsset` | `.biome.ron` |

Per-biome `parallax.ron` uses existing `ParallaxConfigAsset`.

### State Machine

```
Loading → LoadingBiomes → LoadingAutotile → InGame
```

**Loading** (existing):
- Load `world.config.ron` → read `planet_type`
- Load `planet_types/{planet_type}.planet.ron`
- Load `tiles.registry.ron` (unchanged)

**LoadingBiomes** (new):
1. Collect all biome IDs from PlanetType (primary + secondary + all layer biomes)
2. Load `biomes/{id}/{id}.biome.ron` for each
3. Load `biomes/{id}/parallax.ron` for each (if exists)
4. When all loaded → build `BiomeRegistry` and `BiomeMap` resources
5. Transition to `LoadingAutotile`

**LoadingAutotile** (existing, unchanged)

**InGame**:
- Parallax spawned for player's **initial biome** (not global)
- Terrain gen uses `BiomeMap` + `BiomeRegistry`

### New Resources

```rust
#[derive(Resource)]
pub struct BiomeRegistry {
    pub biomes: HashMap<String, BiomeDef>,
}

#[derive(Resource)]
pub struct BiomeMap {
    pub regions: Vec<BiomeRegion>,
}

#[derive(Resource)]
pub struct PlanetType { /* from PlanetTypeAsset */ }
```

### Hot-Reload

- `*.biome.ron` changed → update `BiomeRegistry`, mark all chunks dirty
- `parallax.ron` changed → respawn parallax layers for current biome
- `*.planet.ron` changed → full reload (regenerate BiomeMap, all chunks dirty)

## 5. Changes to Existing Code

### Modified

| File | Change |
|------|--------|
| `src/world/terrain_gen.rs` | Full rework — biome-driven tile selection instead of hardcoded |
| `src/world/chunk.rs` | Pass BiomeMap/BiomeRegistry/PlanetType to terrain gen |
| `src/registry/mod.rs` | Add `LoadingBiomes` state, biome/planet loaders, hot-reload |
| `src/registry/assets.rs` | Add `PlanetTypeAsset`, `BiomeAsset` structs |
| `src/registry/world.rs` | Add `planet_type` field to `WorldConfig` |
| `src/parallax/mod.rs` | Biome-aware spawn, add transition systems |
| `src/parallax/spawn.rs` | Accept biome_id, `ParallaxLayer` gets biome_id field |
| `src/parallax/scroll.rs` | Handle two active layer sets during transition |
| `src/main.rs` | Register new state, systems |
| `assets/world/world.config.ron` | Add `planet_type: "garden"` |

### Removed

| Item | Reason |
|------|--------|
| `TerrainTiles` struct + logic | Replaced by BiomeRegistry tile lookup |
| `assets/world/parallax.ron` | Replaced by per-biome parallax |

### Unchanged

- `src/world/autotile.rs` — bitmask/variant logic untouched
- `src/world/atlas.rs` — atlas building untouched
- `src/world/mesh_builder.rs` — mesh building untouched
- `src/world/tile_renderer.rs` — material/shader untouched
- `assets/shaders/tile.wgsl` — shader untouched
- `src/player/` — all player systems untouched
- `src/camera/` — camera untouched
- `src/interaction/block_action.rs` — block interaction untouched

## 6. Initial Content (MVP)

### Surface Biomes

| Biome | Role | Surface | Subsurface | Fill | Cave Threshold |
|-------|------|---------|------------|------|---------------|
| meadow | primary | grass | dirt | stone | 0.3 |
| forest | secondary | grass | dirt | stone | 0.3 |
| rocky | secondary | stone | stone | stone | 0.3 |

### Layer Biomes

| Biome | Layer | Fill | Cave Threshold |
|-------|-------|------|---------------|
| underground_dirt | Underground | stone | 0.3 |
| underground_rock | Deep Underground | stone | 0.25 |
| core_magma | Core | stone | 0.15 |

### Placeholder Graphics

Background PNGs will be simple placeholders (solid gradients or simple silhouettes) to validate the system. Can be generated with a script.

## 7. Out of Scope (Future)

- Mini biomes (small sub-regions within major biomes)
- Weather per biome
- Music/ambient per biome
- Surface placeables (trees, plants, objects)
- Monster spawning per biome
- Status effects per biome
- Dungeons/structures
- Multiple planet types (only `garden` for now)
