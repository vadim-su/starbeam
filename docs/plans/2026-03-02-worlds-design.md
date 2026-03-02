# Procedural Worlds Design

**Date:** 2026-03-02
**Status:** Approved
**Scope:** B — one procedural star system, spawn on garden planet

## Overview

Replace the current single hardcoded world (`assets/world/`) with a procedural universe system. Star systems contain stars and orbiting celestial bodies (planets, moons). All parameters are deterministically derived from hierarchical coordinates via hash-chain seed derivation. Asset files define templates with ranges; concrete values are computed per-body from seeds.

## 1. Hierarchical Addressing

Every celestial body has a unique address:

```rust
CelestialAddress {
    galaxy: IVec2,        // galaxy position in universe (2D)
    system: IVec2,        // system position in galaxy (2D)
    orbit: u32,           // orbital index (0 = closest to star)
    satellite: Option<u32>, // None = planet, Some(n) = nth moon
}
```

Example: `(0,0):(15,7):2:None` = galaxy 0,0 → system 15,7 → planet at orbit 2.

## 2. Seed Derivation (Hash-Chain)

One `universe_seed: u64` (from save file) determines the entire universe.

```
universe_seed
  → hash(universe_seed, galaxy.x, galaxy.y)           → galaxy_seed
    → hash(galaxy_seed, system.x, system.y)            → system_seed
      → hash(system_seed, "star")                      → star_seed
      → hash(system_seed, orbit_index)                 → planet_seed
        → hash(planet_seed, "terrain")                 → terrain_seed
        → hash(planet_seed, "daynight")                → daynight_seed
        → hash(planet_seed, "biomes")                  → biome_seed
        → hash(planet_seed, satellite_index)            → moon_seed
```

`hash` — deterministic hash function (e.g., FxHash or SipHash from tuple).

### What each level determines

| Level | Seed | Generates |
|---|---|---|
| Galaxy | `galaxy_seed` | System density, galaxy type (future) |
| System | `system_seed` | Number of orbits |
| Star | `star_seed` | Star type → luminosity, sun_colors, temperature zones |
| Planet | `planet_seed` | Planet type (from zone), concrete params from ranges |
| Moon | `moon_seed` | Same as planet but smaller, tied to parent |

## 3. Asset Structure

### `assets/worlds/` — templates and generation rules

Replaces `assets/world/`. Contains no concrete worlds.

```
assets/worlds/
  generation.ron
  star_types/
    yellow_dwarf/yellow_dwarf.star.ron
    red_giant/red_giant.star.ron
    blue_star/blue_star.star.ron
  planet_types/
    garden/garden.planet.ron
    desert/desert.planet.ron
    molten/molten.planet.ron
    frozen/frozen.planet.ron
    barren/barren.planet.ron
    toxic/toxic.planet.ron
```

### `assets/content/biomes/` — biome content

Moved from `assets/world/biomes/`. Biomes are content (sprites, parallax backgrounds), not generation config.

```
assets/content/biomes/
  meadow/
    meadow.biome.ron
    meadow.parallax.ron
    backgrounds/sky.png, near_hills.png, far_hills.png
  forest/...
  rocky/...
  underground_dirt/...
  underground_rock/...
  core_magma/...
```

### Deleted

- `assets/world/world.config.ron` — replaced by `generation.ron` + procedural generation
- `assets/world/day_night.config.ron` — params move into star_type + planet_type
- `assets/world/tiles.registry.ron` — separate task (per-tile `.tile.ron`)
- `assets/world/planet_types/` → `assets/worlds/planet_types/`
- `assets/world/biomes/` → `assets/content/biomes/`
- Entire `assets/world/` directory removed

## 4. RON File Formats

### `generation.ron` — global rules

```ron
(
    default_planet_size: (width: 2048, height: 1024),
    chunk_size: 32,
    tile_size: 8.0,
    chunk_load_radius: 3,
    orbit_temperature_falloff: 0.15,
)
```

### `*.star.ron` — star type template

```ron
(
    id: "yellow_dwarf",
    orbit_count: (3, 8),
    luminosity: (0.8, 1.2),
    sun_color: (1.0, 0.98, 0.90),
    zones: [
        (orbits: (0, 1), temperature: "hot",  types: ["molten", "barren"]),
        (orbits: (2, 4), temperature: "warm", types: ["garden", "desert", "toxic"]),
        (orbits: (5, 9), temperature: "cold", types: ["frozen", "barren"]),
    ],
)
```

### `*.planet.ron` — planet type template (Optional fields)

Fields set to `None` are computed procedurally (from orbit, star, seed).

```ron
(
    id: "garden",
    size: None,
    cycle_duration_range: None,
    day_ratio: Some((0.35, 0.50)),
    night_ratio: Some((0.30, 0.45)),
    dawn_ratio: None,
    sunset_ratio: None,
    sky_color_palette: Some([
        [(0.90, 0.50, 0.30, 1.0), (1.0, 0.60, 0.40, 1.0)],
        [(0.85, 0.90, 1.0, 1.0),  (1.0, 1.0, 1.0, 1.0)],
        [(0.85, 0.35, 0.25, 1.0), (0.95, 0.45, 0.35, 1.0)],
        [(0.05, 0.05, 0.15, 1.0), (0.12, 0.12, 0.22, 1.0)],
    ]),
    sun_intensity_modifier: None,
    primary_biome: "meadow",
    secondary_biomes: ["forest", "rocky"],
    layers: (
        surface:          (primary_biome: None,                    terrain_frequency: 0.02, terrain_amplitude: 40.0, depth_ratio: 0.30),
        underground:      (primary_biome: Some("underground_dirt"), terrain_frequency: 0.07, terrain_amplitude: 1.0,  depth_ratio: 0.25),
        deep_underground: (primary_biome: Some("underground_rock"), terrain_frequency: 0.05, terrain_amplitude: 1.0,  depth_ratio: 0.33),
        core:             (primary_biome: Some("core_magma"),       terrain_frequency: 0.04, terrain_amplitude: 1.0,  depth_ratio: 0.12),
    ),
    region_width_min: 300,
    region_width_max: 600,
    primary_region_ratio: 0.6,
    danger_multipliers: Some([0.5, 0.0, 0.5, 1.0]),
    temperature_modifiers: None,
)
```

Resolution logic: `planet_type.field.unwrap_or_else(|| derive_from_orbit_and_star(...))`

## 5. Rust Architecture

### New module: `src/cosmos/`

```
src/cosmos/
    mod.rs          — CosmosPlugin
    address.rs      — CelestialAddress, CelestialSeeds
    generation.rs   — generate_system(), generate_day_night(), determine_star_type(), determine_planet_type()
    assets.rs       — StarTypeAsset, GenerationConfigAsset
```

### Key types

```rust
/// Deterministic seeds derived from address
pub struct CelestialSeeds {
    pub galaxy_seed: u64,
    pub system_seed: u64,
    pub star_seed: u64,
    pub body_seed: u64,
    pub terrain_seed: u64,
    pub daynight_seed: u64,
    pub biome_seed: u64,
}

/// Result of procedural system generation
pub struct GeneratedSystem {
    pub star: GeneratedStar,
    pub bodies: Vec<GeneratedBody>,
}

pub struct GeneratedStar {
    pub type_id: String,
    pub luminosity: f32,
    pub sun_color: [f32; 3],
}

pub struct GeneratedBody {
    pub address: CelestialAddress,
    pub planet_type_id: String,
    pub size: (i32, i32),
    pub day_night: DayNightConfig,
    pub moons: Vec<GeneratedBody>,
}
```

### Changes to existing code

| Current | New | Change |
|---|---|---|
| `WorldConfig` (Resource) | `ActiveWorld` (Resource) | Rename + add `address`, `seeds` fields |
| `WorldConfigAsset` | `GenerationConfigAsset` | New asset for `generation.ron` |
| `DayNightConfig` loaded via `include_str!()` | `DayNightConfig` generated by `cosmos::generation` | Same struct, different source |
| `PlanetConfig` | `PlanetConfig` (unchanged) | Built from `PlanetTypeAsset` as before |
| `WorldCtx.config: Res<WorldConfig>` | `WorldCtx.config: Res<ActiveWorld>` | Type rename |

### Day/night parameter resolution

| Parameter | Source |
|---|---|
| `cycle_duration_secs` | Orbit distance (farther from star = longer day) |
| `day_ratio / night_ratio` | Planet axis tilt (procedural from seed) or planet_type range |
| `sun_colors` | Star type (red_giant = reddish, yellow_dwarf = yellow) |
| `sun_intensities` | Star luminosity × orbit distance |
| `sky_colors` | Planet atmosphere (planet_type palette range) |
| `danger_multipliers` | Planet_type range or derived from temperature zone |
| `temperature_modifiers` | Temperature zone + orbit distance |

Two-level resolution:
1. **Star type** provides: base sun_color, luminosity
2. **Planet type** provides: ranges for cycle_duration, day/night ratios, sky_colors palette
3. **Concrete planet** (seed from coordinates): picks exact values from ranges + modifies sun_intensity by orbit distance

### World loading flow (new)

```
1. Load assets: generation.ron, star_types/*.star.ron, planet_types/*.planet.ron, biomes/*.biome.ron
2. Player creates/loads universe → universe_seed
3. Generate system:
   CelestialSeeds::derive(universe_seed, address)
   → determine_star_type(star_seed, star_templates)
   → for each orbit: determine_planet_type(planet_seed, star.zones)
   → generate_day_night(star, planet_template, orbit, daynight_seed)
4. Player selects planet → insert ActiveWorld resource
5. Existing pipeline: load biomes → build BiomeMap → generate terrain
   (works as before, seed from ActiveWorld.seeds.terrain_seed)
```

## 6. Scope B Boundaries

### In scope (this phase)

- New `cosmos/` module: address, seeds, generation
- Assets `assets/worlds/`: `generation.ron`, `yellow_dwarf.star.ron`, `garden.planet.ron` (Optional fields) + at least one contrasting type (e.g., `barren.planet.ron`)
- Biomes → `assets/content/biomes/` (move from `assets/world/biomes/`)
- `WorldConfig` → `ActiveWorld` rename + new fields
- `DayNightConfig` generated procedurally, remove `include_str!()`
- On game start: generate one system from hardcoded `universe_seed` and address `(0,0):(0,0)`, spawn on first garden planet
- Delete `assets/world/`

### Out of scope (future)

- UI for system/planet selection
- Travel between planets
- Multiple galaxies
- Moons (structures in place, not generated)
- Save/load with deltas
- `tiles.registry.ron` elimination (separate task)
- Full asset autodiscovery (`load_folder`)

### Backward compatibility

- All 213 tests must continue passing
- `terrain_gen.rs` — minimal changes (reads from `ActiveWorld` instead of `WorldConfig`)
- `WorldCtx` / `WorldCtxRef` — `config: &WorldConfig` → `config: &ActiveWorld`
- `BiomeMap::generate()` — unchanged, receives seed from `ActiveWorld`
- `PlanetConfig` — built same way, planet type comes from `cosmos::generation`

## 7. Save Data (Future Reference)

Delta-only approach for modified worlds:
- Save file stores `universe_seed` + list of visited `CelestialAddress`es
- Per visited world: only player modifications (placed/removed blocks) as deltas
- On load: regenerate world from coordinates → apply deltas
- Not implemented in scope B, but architecture supports it
