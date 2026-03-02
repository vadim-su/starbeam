# Procedural Worlds Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the hardcoded single world with a procedural star system, deriving all world parameters from hierarchical coordinates via hash-chain seed derivation.

**Architecture:** New `cosmos/` module handles addressing, seed derivation, and system generation. Star/planet type templates define ranges; concrete values resolved at generation time. Existing `world/` module consumes generated `ActiveWorld` + `DayNightConfig` resources unchanged.

**Tech Stack:** Rust, Bevy 0.18, RON assets, Perlin noise (existing), FxHash for seed derivation.

**Design doc:** `docs/plans/2026-03-02-worlds-design.md`

---

### Task 1: Create `cosmos/address.rs` — CelestialAddress + CelestialSeeds

**Files:**
- Create: `src/cosmos/mod.rs`
- Create: `src/cosmos/address.rs`
- Modify: `src/lib.rs` (add `pub mod cosmos;`)

**Step 1: Write the tests in `address.rs`**

```rust
// src/cosmos/address.rs

use bevy::math::IVec2;

/// Unique address for any celestial body in the universe.
#[derive(Clone, Hash, Eq, PartialEq, Debug)]
pub struct CelestialAddress {
    pub galaxy: IVec2,
    pub system: IVec2,
    pub orbit: u32,
    pub satellite: Option<u32>,
}

/// Deterministic seeds derived from a universe seed + address via hash-chain.
#[derive(Debug, Clone)]
pub struct CelestialSeeds {
    pub galaxy_seed: u64,
    pub system_seed: u64,
    pub star_seed: u64,
    pub body_seed: u64,
    pub terrain_seed: u64,
    pub daynight_seed: u64,
    pub biome_seed: u64,
}

/// Deterministic hash combiner — mixes a seed with extra u64 data.
fn hash_combine(seed: u64, value: u64) -> u64 {
    // SplitMix64 step on (seed ^ value)
    let mut z = seed.wrapping_add(value).wrapping_add(0x9e3779b97f4a7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

/// Hash combiner for string tags (e.g., "terrain", "daynight").
fn hash_tag(seed: u64, tag: &str) -> u64 {
    let tag_hash: u64 = tag.bytes().fold(0u64, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(b as u64)
    });
    hash_combine(seed, tag_hash)
}

/// Combine two i32 coordinates into a single u64 for hashing.
fn pack_coords(x: i32, y: i32) -> u64 {
    ((x as u64) << 32) | (y as u32 as u64)
}

impl CelestialSeeds {
    /// Derive all seeds from a universe seed and celestial address.
    pub fn derive(universe_seed: u64, address: &CelestialAddress) -> Self {
        let galaxy_seed = hash_combine(universe_seed, pack_coords(address.galaxy.x, address.galaxy.y));
        let system_seed = hash_combine(galaxy_seed, pack_coords(address.system.x, address.system.y));
        let star_seed = hash_tag(system_seed, "star");

        let body_seed = if let Some(moon_idx) = address.satellite {
            let planet_seed = hash_combine(system_seed, address.orbit as u64);
            hash_combine(planet_seed, moon_idx as u64)
        } else {
            hash_combine(system_seed, address.orbit as u64)
        };

        let terrain_seed = hash_tag(body_seed, "terrain");
        let daynight_seed = hash_tag(body_seed, "daynight");
        let biome_seed = hash_tag(body_seed, "biomes");

        Self {
            galaxy_seed,
            system_seed,
            star_seed,
            body_seed,
            terrain_seed,
            daynight_seed,
            biome_seed,
        }
    }

    /// Convenience: extract terrain seed as u32 for Perlin noise compatibility.
    pub fn terrain_seed_u32(&self) -> u32 {
        self.terrain_seed as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_address() -> CelestialAddress {
        CelestialAddress {
            galaxy: IVec2::ZERO,
            system: IVec2::ZERO,
            orbit: 2,
            satellite: None,
        }
    }

    #[test]
    fn derive_is_deterministic() {
        let a = CelestialSeeds::derive(42, &test_address());
        let b = CelestialSeeds::derive(42, &test_address());
        assert_eq!(a.galaxy_seed, b.galaxy_seed);
        assert_eq!(a.system_seed, b.system_seed);
        assert_eq!(a.star_seed, b.star_seed);
        assert_eq!(a.body_seed, b.body_seed);
        assert_eq!(a.terrain_seed, b.terrain_seed);
        assert_eq!(a.daynight_seed, b.daynight_seed);
        assert_eq!(a.biome_seed, b.biome_seed);
    }

    #[test]
    fn different_universe_seed_different_result() {
        let a = CelestialSeeds::derive(42, &test_address());
        let b = CelestialSeeds::derive(999, &test_address());
        assert_ne!(a.galaxy_seed, b.galaxy_seed);
    }

    #[test]
    fn different_orbit_different_body_seed() {
        let addr1 = test_address();
        let mut addr2 = test_address();
        addr2.orbit = 3;
        let a = CelestialSeeds::derive(42, &addr1);
        let b = CelestialSeeds::derive(42, &addr2);
        assert_eq!(a.star_seed, b.star_seed, "same system → same star");
        assert_ne!(a.body_seed, b.body_seed, "different orbit → different body");
    }

    #[test]
    fn moon_differs_from_planet() {
        let planet_addr = test_address();
        let moon_addr = CelestialAddress {
            satellite: Some(0),
            ..test_address()
        };
        let a = CelestialSeeds::derive(42, &planet_addr);
        let b = CelestialSeeds::derive(42, &moon_addr);
        assert_ne!(a.body_seed, b.body_seed);
    }

    #[test]
    fn different_galaxy_different_seeds() {
        let addr1 = test_address();
        let addr2 = CelestialAddress {
            galaxy: IVec2::new(1, 0),
            ..test_address()
        };
        let a = CelestialSeeds::derive(42, &addr1);
        let b = CelestialSeeds::derive(42, &addr2);
        assert_ne!(a.galaxy_seed, b.galaxy_seed);
        assert_ne!(a.system_seed, b.system_seed);
    }

    #[test]
    fn sub_seeds_differ_from_each_other() {
        let s = CelestialSeeds::derive(42, &test_address());
        assert_ne!(s.terrain_seed, s.daynight_seed);
        assert_ne!(s.terrain_seed, s.biome_seed);
        assert_ne!(s.daynight_seed, s.biome_seed);
    }

    #[test]
    fn terrain_seed_u32_truncates() {
        let s = CelestialSeeds::derive(42, &test_address());
        assert_eq!(s.terrain_seed_u32(), s.terrain_seed as u32);
    }
}
```

**Step 2: Create `src/cosmos/mod.rs`**

```rust
pub mod address;
```

**Step 3: Add `pub mod cosmos;` to `src/lib.rs`**

**Step 4: Run tests**

Run: `cargo test cosmos::address`
Expected: All 7 tests PASS.

**Step 5: Commit**

```bash
git add src/cosmos/ src/lib.rs
git commit -m "feat(cosmos): add CelestialAddress and hash-chain seed derivation"
```

---

### Task 2: Create star type and generation config assets

**Files:**
- Create: `src/cosmos/assets.rs`
- Create: `assets/worlds/generation.ron`
- Create: `assets/worlds/star_types/yellow_dwarf/yellow_dwarf.star.ron`
- Modify: `src/cosmos/mod.rs` (add `pub mod assets;`)
- Modify: `src/registry/mod.rs` (register new asset types + loaders)

**Step 1: Create `src/cosmos/assets.rs`**

```rust
use bevy::prelude::*;
use bevy::reflect::TypePath;
use serde::Deserialize;

/// Global generation rules loaded from generation.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct GenerationConfigAsset {
    pub default_planet_size: PlanetSizeConfig,
    pub chunk_size: u32,
    pub tile_size: f32,
    pub chunk_load_radius: i32,
    #[serde(default = "default_orbit_temp_falloff")]
    pub orbit_temperature_falloff: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlanetSizeConfig {
    pub width: i32,
    pub height: i32,
}

fn default_orbit_temp_falloff() -> f32 {
    0.15
}

/// Temperature zone within a star type.
#[derive(Debug, Clone, Deserialize)]
pub struct TemperatureZone {
    pub orbits: (u32, u32),
    pub temperature: String,
    pub types: Vec<String>,
}

/// Star type template loaded from *.star.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct StarTypeAsset {
    pub id: String,
    pub orbit_count: (u32, u32),
    pub luminosity: (f32, f32),
    pub sun_color: [f32; 3],
    pub zones: Vec<TemperatureZone>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_generation_config() {
        let ron_str = include_str!("../../assets/worlds/generation.ron");
        let config: GenerationConfigAsset =
            ron::from_str(ron_str).expect("Failed to parse generation.ron");
        assert_eq!(config.chunk_size, 32);
        assert!(config.default_planet_size.width > 0);
    }

    #[test]
    fn parse_star_type() {
        let ron_str = include_str!("../../assets/worlds/star_types/yellow_dwarf/yellow_dwarf.star.ron");
        let star: StarTypeAsset =
            ron::from_str(ron_str).expect("Failed to parse yellow_dwarf.star.ron");
        assert_eq!(star.id, "yellow_dwarf");
        assert!(!star.zones.is_empty());
        assert!(star.orbit_count.0 <= star.orbit_count.1);
    }
}
```

**Step 2: Create `assets/worlds/generation.ron`**

```ron
(
    default_planet_size: (width: 2048, height: 1024),
    chunk_size: 32,
    tile_size: 8.0,
    chunk_load_radius: 3,
    orbit_temperature_falloff: 0.15,
)
```

**Step 3: Create `assets/worlds/star_types/yellow_dwarf/yellow_dwarf.star.ron`**

```ron
(
    id: "yellow_dwarf",
    orbit_count: (3, 8),
    luminosity: (0.8, 1.2),
    sun_color: (1.0, 0.98, 0.90),
    zones: [
        (orbits: (0, 1), temperature: "hot",  types: ["barren"]),
        (orbits: (2, 4), temperature: "warm", types: ["garden"]),
        (orbits: (5, 9), temperature: "cold", types: ["barren"]),
    ],
)
```

Note: Only existing planet types (`garden`, `barren`) referenced for now. `barren` will be created in Task 3.

**Step 4: Register new asset types in `src/registry/mod.rs`**

Add to imports and `RegistryPlugin::build()`:
- `init_asset::<StarTypeAsset>()`
- `init_asset::<GenerationConfigAsset>()`
- `register_asset_loader(RonLoader::<StarTypeAsset>::new(&["star.ron"]))`
- `register_asset_loader(RonLoader::<GenerationConfigAsset>::new(&["generation.ron"]))`

**Step 5: Run tests**

Run: `cargo test cosmos::assets`
Expected: 2 tests PASS.

**Step 6: Commit**

```bash
git add src/cosmos/ assets/worlds/ src/registry/mod.rs
git commit -m "feat(cosmos): add StarTypeAsset, GenerationConfigAsset, and RON files"
```

---

### Task 3: Update PlanetTypeAsset with Optional fields + create barren planet type

**Files:**
- Modify: `src/registry/assets.rs` (PlanetTypeAsset — add Optional fields for day/night)
- Modify: `assets/worlds/planet_types/garden/garden.planet.ron` (move from `assets/world/planet_types/`)
- Create: `assets/worlds/planet_types/barren/barren.planet.ron`
- Create: `assets/content/biomes/barren/barren.biome.ron` (minimal biome for barren worlds)

**Step 1: Add day/night range fields to `PlanetTypeAsset`**

In `src/registry/assets.rs`, add to `PlanetTypeAsset`:

```rust
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct PlanetTypeAsset {
    pub id: String,
    pub primary_biome: String,
    pub secondary_biomes: Vec<String>,
    pub layers: LayersAsset,
    pub region_width_min: u32,
    pub region_width_max: u32,
    pub primary_region_ratio: f64,
    // Day/night ranges — None = derive procedurally from orbit/star
    #[serde(default)]
    pub size: Option<(i32, i32)>,
    #[serde(default)]
    pub cycle_duration_range: Option<(f32, f32)>,
    #[serde(default)]
    pub day_ratio: Option<(f32, f32)>,
    #[serde(default)]
    pub night_ratio: Option<(f32, f32)>,
    #[serde(default)]
    pub dawn_ratio: Option<(f32, f32)>,
    #[serde(default)]
    pub sunset_ratio: Option<(f32, f32)>,
    #[serde(default)]
    pub sky_color_palette: Option<[[[f32; 4]; 2]; 4]>,
    #[serde(default)]
    pub sun_intensity_modifier: Option<(f32, f32)>,
    #[serde(default)]
    pub danger_multipliers: Option<[f32; 4]>,
    #[serde(default)]
    pub temperature_modifiers: Option<[f32; 4]>,
}
```

All new fields are `#[serde(default)]` → existing RON files continue to parse.

**Step 2: Move `garden.planet.ron` to new location**

```bash
git mv assets/world/planet_types/garden.planet.ron assets/worlds/planet_types/garden/garden.planet.ron
```

Add Optional fields to `garden.planet.ron`:

```ron
(
    id: "garden",
    primary_biome: "meadow",
    secondary_biomes: ["forest", "rocky"],
    layers: (
        surface: (primary_biome: None, terrain_frequency: 0.02, terrain_amplitude: 40.0, depth_ratio: 0.30),
        underground: (primary_biome: Some("underground_dirt"), terrain_frequency: 0.07, terrain_amplitude: 1.0, depth_ratio: 0.25),
        deep_underground: (primary_biome: Some("underground_rock"), terrain_frequency: 0.05, terrain_amplitude: 1.0, depth_ratio: 0.33),
        core: (primary_biome: Some("core_magma"), terrain_frequency: 0.04, terrain_amplitude: 1.0, depth_ratio: 0.12),
    ),
    region_width_min: 300,
    region_width_max: 600,
    primary_region_ratio: 0.6,
    day_ratio: Some((0.35, 0.50)),
    night_ratio: Some((0.30, 0.45)),
    sky_color_palette: Some([
        [(0.90, 0.50, 0.30, 1.0), (1.0, 0.60, 0.40, 1.0)],
        [(0.85, 0.90, 1.0, 1.0),  (1.0, 1.0, 1.0, 1.0)],
        [(0.85, 0.35, 0.25, 1.0), (0.95, 0.45, 0.35, 1.0)],
        [(0.05, 0.05, 0.15, 1.0), (0.12, 0.12, 0.22, 1.0)],
    ]),
    danger_multipliers: Some([0.5, 0.0, 0.5, 1.0]),
)
```

**Step 3: Create `barren.planet.ron`**

```ron
(
    id: "barren",
    primary_biome: "barren",
    secondary_biomes: ["barren"],
    layers: (
        surface: (primary_biome: None, terrain_frequency: 0.03, terrain_amplitude: 20.0, depth_ratio: 0.35),
        underground: (primary_biome: Some("underground_rock"), terrain_frequency: 0.06, terrain_amplitude: 1.0, depth_ratio: 0.30),
        deep_underground: (primary_biome: Some("underground_rock"), terrain_frequency: 0.05, terrain_amplitude: 1.0, depth_ratio: 0.25),
        core: (primary_biome: Some("core_magma"), terrain_frequency: 0.04, terrain_amplitude: 1.0, depth_ratio: 0.10),
    ),
    region_width_min: 400,
    region_width_max: 800,
    primary_region_ratio: 1.0,
    sky_color_palette: Some([
        [(0.20, 0.15, 0.10, 1.0), (0.30, 0.20, 0.15, 1.0)],
        [(0.35, 0.30, 0.25, 1.0), (0.45, 0.40, 0.35, 1.0)],
        [(0.25, 0.15, 0.10, 1.0), (0.30, 0.20, 0.15, 1.0)],
        [(0.03, 0.03, 0.05, 1.0), (0.08, 0.08, 0.12, 1.0)],
    ]),
)
```

**Step 4: Create minimal `barren.biome.ron`**

```ron
(
    id: "barren",
    surface_block: "stone",
    subsurface_block: "stone",
    subsurface_depth: 2,
    fill_block: "stone",
    cave_threshold: 0.25,
)
```

**Step 5: Run tests**

Run: `cargo test`
Expected: All existing 213 tests PASS (new Optional fields are `serde(default)` → backward compatible).

**Step 6: Commit**

```bash
git add -A
git commit -m "feat(cosmos): add Optional day/night fields to PlanetTypeAsset, create barren planet type"
```

---

### Task 4: Create `cosmos/generation.rs` — system generation logic

**Files:**
- Create: `src/cosmos/generation.rs`
- Modify: `src/cosmos/mod.rs` (add `pub mod generation;`)

This is the core generation module. It uses star/planet templates + seeds to produce concrete values.

**Step 1: Write `generation.rs` with tests**

```rust
// src/cosmos/generation.rs

use crate::cosmos::address::{CelestialAddress, CelestialSeeds};
use crate::cosmos::assets::{GenerationConfigAsset, StarTypeAsset};
use crate::registry::assets::PlanetTypeAsset;
use crate::world::day_night::DayNightConfig;

/// A procedurally generated star (concrete values, not ranges).
#[derive(Debug, Clone)]
pub struct GeneratedStar {
    pub type_id: String,
    pub luminosity: f32,
    pub sun_color: [f32; 3],
    pub orbit_count: u32,
}

/// A procedurally generated celestial body (planet or moon).
#[derive(Debug, Clone)]
pub struct GeneratedBody {
    pub address: CelestialAddress,
    pub planet_type_id: String,
    pub width_tiles: i32,
    pub height_tiles: i32,
    pub day_night: DayNightConfig,
}

/// A complete generated star system.
#[derive(Debug, Clone)]
pub struct GeneratedSystem {
    pub star: GeneratedStar,
    pub bodies: Vec<GeneratedBody>,
}

/// Deterministic float in [0, 1) from a seed.
fn seed_to_f32(seed: u64) -> f32 {
    (seed >> 33) as f32 / (1u64 << 31) as f32
}

/// Pick a value from a (min, max) range using a seed.
fn lerp_range(seed: u64, min: f32, max: f32) -> f32 {
    min + seed_to_f32(seed) * (max - min)
}

/// Pick an integer from a (min, max) inclusive range using a seed.
fn range_u32(seed: u64, min: u32, max: u32) -> u32 {
    min + (seed % (max - min + 1) as u64) as u32
}

/// Pick an element from a slice using a seed.
fn pick<'a, T>(seed: u64, items: &'a [T]) -> &'a T {
    &items[(seed % items.len() as u64) as usize]
}

/// Sub-seed helper: derives a new seed for a specific index from a parent.
fn sub_seed(parent: u64, index: u64) -> u64 {
    // Re-use the hash_combine logic
    let mut z = parent.wrapping_add(index).wrapping_add(0x9e3779b97f4a7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

/// Generate a complete star system from templates and a system address.
pub fn generate_system(
    universe_seed: u64,
    galaxy: bevy::math::IVec2,
    system: bevy::math::IVec2,
    star_templates: &[&StarTypeAsset],
    planet_templates: &std::collections::HashMap<String, &PlanetTypeAsset>,
    gen_config: &GenerationConfigAsset,
) -> GeneratedSystem {
    let base_addr = CelestialAddress {
        galaxy,
        system,
        orbit: 0,
        satellite: None,
    };
    let seeds = CelestialSeeds::derive(universe_seed, &base_addr);

    // Pick star type
    let star_template = pick(seeds.star_seed, star_templates);
    let luminosity = lerp_range(sub_seed(seeds.star_seed, 1), star_template.luminosity.0, star_template.luminosity.1);
    let orbit_count = range_u32(sub_seed(seeds.star_seed, 2), star_template.orbit_count.0, star_template.orbit_count.1);

    let star = GeneratedStar {
        type_id: star_template.id.clone(),
        luminosity,
        sun_color: star_template.sun_color,
        orbit_count,
    };

    // Generate bodies for each orbit
    let mut bodies = Vec::with_capacity(orbit_count as usize);
    for orbit in 0..orbit_count {
        let addr = CelestialAddress {
            galaxy,
            system,
            orbit,
            satellite: None,
        };
        let body_seeds = CelestialSeeds::derive(universe_seed, &addr);

        // Determine planet type from star's temperature zones
        let planet_type_id = determine_planet_type(orbit, star_template, body_seeds.body_seed);

        // Look up template (fallback to first available if type not found)
        let planet_template = planet_templates.get(&planet_type_id)
            .or_else(|| planet_templates.values().next())
            .expect("at least one planet template must exist");

        // Generate size
        let (width, height) = planet_template.size.unwrap_or(
            (gen_config.default_planet_size.width, gen_config.default_planet_size.height)
        );

        // Generate day/night
        let day_night = generate_day_night(
            &star,
            planet_template,
            orbit,
            orbit_count,
            body_seeds.daynight_seed,
        );

        bodies.push(GeneratedBody {
            address: addr,
            planet_type_id,
            width_tiles: width,
            height_tiles: height,
            day_night,
        });
    }

    GeneratedSystem { star, bodies }
}

/// Determine planet type from orbit index and star's temperature zones.
fn determine_planet_type(orbit: u32, star: &StarTypeAsset, seed: u64) -> String {
    for zone in &star.zones {
        if orbit >= zone.orbits.0 && orbit <= zone.orbits.1 && !zone.types.is_empty() {
            return pick(seed, &zone.types).clone();
        }
    }
    // Fallback: first type of last zone
    star.zones.last()
        .and_then(|z| z.types.first())
        .cloned()
        .unwrap_or_else(|| "barren".to_string())
}

/// Generate concrete DayNightConfig from star + planet template + orbit.
pub fn generate_day_night(
    star: &GeneratedStar,
    planet: &PlanetTypeAsset,
    orbit: u32,
    total_orbits: u32,
    seed: u64,
) -> DayNightConfig {
    // Orbit factor: 0.0 = closest, 1.0 = farthest
    let orbit_factor = if total_orbits > 1 {
        orbit as f32 / (total_orbits - 1) as f32
    } else {
        0.5
    };

    // Cycle duration: farther = longer days (600..1800 secs range)
    let cycle_duration = planet.cycle_duration_range
        .map(|(min, max)| lerp_range(sub_seed(seed, 10), min, max))
        .unwrap_or(600.0 + orbit_factor * 1200.0);

    // Day/night ratios
    let day_ratio = planet.day_ratio
        .map(|(min, max)| lerp_range(sub_seed(seed, 20), min, max))
        .unwrap_or(0.35 + orbit_factor * 0.1);
    let night_ratio = planet.night_ratio
        .map(|(min, max)| lerp_range(sub_seed(seed, 21), min, max))
        .unwrap_or(0.35 + orbit_factor * 0.1);
    // Dawn/sunset fill the remainder
    let transition_total = (1.0 - day_ratio - night_ratio).max(0.04);
    let dawn_ratio = planet.dawn_ratio
        .map(|(min, max)| lerp_range(sub_seed(seed, 22), min, max))
        .unwrap_or(transition_total * 0.5);
    let sunset_ratio = planet.sunset_ratio
        .map(|(min, max)| lerp_range(sub_seed(seed, 23), min, max))
        .unwrap_or(transition_total - dawn_ratio);

    // Normalize ratios to sum to 1.0
    let sum = day_ratio + night_ratio + dawn_ratio + sunset_ratio;
    let day_ratio = day_ratio / sum;
    let night_ratio = night_ratio / sum;
    let dawn_ratio = dawn_ratio / sum;
    let sunset_ratio = sunset_ratio / sum;

    // Sun intensity: luminosity diminished by distance
    let base_intensity = star.luminosity * (1.0 - orbit_factor * 0.6);
    let intensity_mod = planet.sun_intensity_modifier
        .map(|(min, max)| lerp_range(sub_seed(seed, 30), min, max))
        .unwrap_or(1.0);
    let peak_intensity = base_intensity * intensity_mod;

    let sun_intensities = [
        peak_intensity * 0.6,
        peak_intensity,
        peak_intensity * 0.5,
        0.0,
    ];

    // Sun colors from star
    let sun_colors = [
        [star.sun_color[0], star.sun_color[1] * 0.66, star.sun_color[2] * 0.39],
        star.sun_color,
        [star.sun_color[0], star.sun_color[1] * 0.51, star.sun_color[2] * 0.28],
        [star.sun_color[0] * 0.15, star.sun_color[1] * 0.15, star.sun_color[2] * 0.39],
    ];

    // Sky colors from planet palette or defaults
    let sky_colors = if let Some(palette) = &planet.sky_color_palette {
        let mut colors = [[0.0f32; 4]; 4];
        for (i, pair) in palette.iter().enumerate() {
            let s = sub_seed(seed, 40 + i as u64);
            let t = seed_to_f32(s);
            for c in 0..4 {
                colors[i][c] = pair[0][c] + t * (pair[1][c] - pair[0][c]);
            }
        }
        colors
    } else {
        // Default dim atmosphere
        [
            [0.5, 0.3, 0.2, 1.0],
            [0.6, 0.6, 0.7, 1.0],
            [0.5, 0.25, 0.15, 1.0],
            [0.05, 0.05, 0.1, 1.0],
        ]
    };

    let danger_multipliers = planet.danger_multipliers.unwrap_or([0.5, 0.0, 0.5, 1.0]);
    let temperature_modifiers = planet.temperature_modifiers.unwrap_or_else(|| {
        let base = -0.1 * (1.0 - orbit_factor);
        [base, 0.0, base * 0.5, base * 2.0]
    });

    // Ambient mins
    let ambient_mins = [
        0.08 * (1.0 - orbit_factor * 0.5),
        0.0,
        0.06 * (1.0 - orbit_factor * 0.5),
        0.04,
    ];

    DayNightConfig {
        cycle_duration_secs: cycle_duration,
        dawn_ratio,
        day_ratio,
        sunset_ratio,
        night_ratio,
        sun_colors,
        sun_intensities,
        ambient_mins,
        sky_colors,
        danger_multipliers,
        temperature_modifiers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cosmos::assets::{GenerationConfigAsset, PlanetSizeConfig, StarTypeAsset, TemperatureZone};
    use bevy::math::IVec2;
    use std::collections::HashMap;

    fn test_star() -> StarTypeAsset {
        StarTypeAsset {
            id: "yellow_dwarf".into(),
            orbit_count: (3, 6),
            luminosity: (0.9, 1.1),
            sun_color: [1.0, 0.98, 0.90],
            zones: vec![
                TemperatureZone { orbits: (0, 1), temperature: "hot".into(), types: vec!["barren".into()] },
                TemperatureZone { orbits: (2, 4), temperature: "warm".into(), types: vec!["garden".into()] },
                TemperatureZone { orbits: (5, 9), temperature: "cold".into(), types: vec!["barren".into()] },
            ],
        }
    }

    fn test_planet_template() -> PlanetTypeAsset {
        PlanetTypeAsset {
            id: "garden".into(),
            primary_biome: "meadow".into(),
            secondary_biomes: vec!["forest".into()],
            layers: crate::registry::assets::LayersAsset {
                surface: crate::registry::assets::LayerConfigAsset { primary_biome: None, terrain_frequency: 0.02, terrain_amplitude: 40.0, depth_ratio: 0.30 },
                underground: crate::registry::assets::LayerConfigAsset { primary_biome: Some("underground_dirt".into()), terrain_frequency: 0.07, terrain_amplitude: 1.0, depth_ratio: 0.25 },
                deep_underground: crate::registry::assets::LayerConfigAsset { primary_biome: Some("underground_rock".into()), terrain_frequency: 0.05, terrain_amplitude: 1.0, depth_ratio: 0.33 },
                core: crate::registry::assets::LayerConfigAsset { primary_biome: Some("core_magma".into()), terrain_frequency: 0.04, terrain_amplitude: 1.0, depth_ratio: 0.12 },
            },
            region_width_min: 300,
            region_width_max: 600,
            primary_region_ratio: 0.6,
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
            danger_multipliers: Some([0.5, 0.0, 0.5, 1.0]),
            temperature_modifiers: None,
        }
    }

    fn test_gen_config() -> GenerationConfigAsset {
        GenerationConfigAsset {
            default_planet_size: PlanetSizeConfig { width: 2048, height: 1024 },
            chunk_size: 32,
            tile_size: 8.0,
            chunk_load_radius: 3,
            orbit_temperature_falloff: 0.15,
        }
    }

    #[test]
    fn generate_system_deterministic() {
        let star = test_star();
        let planet = test_planet_template();
        let gen = test_gen_config();
        let mut templates = HashMap::new();
        templates.insert("garden".to_string(), &planet);
        templates.insert("barren".to_string(), &planet); // reuse for test

        let sys1 = generate_system(42, IVec2::ZERO, IVec2::ZERO, &[&star], &templates, &gen);
        let sys2 = generate_system(42, IVec2::ZERO, IVec2::ZERO, &[&star], &templates, &gen);

        assert_eq!(sys1.star.type_id, sys2.star.type_id);
        assert_eq!(sys1.star.orbit_count, sys2.star.orbit_count);
        assert_eq!(sys1.bodies.len(), sys2.bodies.len());
        for (a, b) in sys1.bodies.iter().zip(sys2.bodies.iter()) {
            assert_eq!(a.planet_type_id, b.planet_type_id);
            assert_eq!(a.day_night.cycle_duration_secs, b.day_night.cycle_duration_secs);
        }
    }

    #[test]
    fn different_seed_different_system() {
        let star = test_star();
        let planet = test_planet_template();
        let gen = test_gen_config();
        let mut templates = HashMap::new();
        templates.insert("garden".to_string(), &planet);
        templates.insert("barren".to_string(), &planet);

        let sys1 = generate_system(42, IVec2::ZERO, IVec2::ZERO, &[&star], &templates, &gen);
        let sys2 = generate_system(999, IVec2::ZERO, IVec2::ZERO, &[&star], &templates, &gen);

        // At minimum, orbit count or luminosity should differ
        let differs = sys1.star.luminosity != sys2.star.luminosity
            || sys1.star.orbit_count != sys2.star.orbit_count;
        assert!(differs, "different universe seeds should produce different systems");
    }

    #[test]
    fn orbit_determines_planet_type() {
        let star = test_star();
        // Orbit 0 should be "hot" zone → barren
        let t = determine_planet_type(0, &star, 42);
        assert_eq!(t, "barren");
        // Orbit 3 should be "warm" zone → garden
        let t = determine_planet_type(3, &star, 42);
        assert_eq!(t, "garden");
    }

    #[test]
    fn day_night_ratios_sum_to_one() {
        let star = GeneratedStar {
            type_id: "yellow_dwarf".into(),
            luminosity: 1.0,
            sun_color: [1.0, 0.98, 0.90],
            orbit_count: 5,
        };
        let planet = test_planet_template();
        let dn = generate_day_night(&star, &planet, 2, 5, 12345);
        let sum = dn.dawn_ratio + dn.day_ratio + dn.sunset_ratio + dn.night_ratio;
        assert!((sum - 1.0).abs() < 0.001, "ratios must sum to 1.0, got {sum}");
    }

    #[test]
    fn day_night_sun_color_from_star() {
        let star = GeneratedStar {
            type_id: "test".into(),
            luminosity: 1.0,
            sun_color: [0.8, 0.3, 0.1],
            orbit_count: 3,
        };
        let planet = test_planet_template();
        let dn = generate_day_night(&star, &planet, 1, 3, 42);
        // Day sun_color should be the star's color
        assert_eq!(dn.sun_colors[1], [0.8, 0.3, 0.1]);
    }

    #[test]
    fn farther_orbit_longer_cycle() {
        let star = GeneratedStar {
            type_id: "test".into(),
            luminosity: 1.0,
            sun_color: [1.0, 1.0, 1.0],
            orbit_count: 6,
        };
        // Use planet without explicit cycle_duration_range → derive from orbit
        let mut planet = test_planet_template();
        planet.cycle_duration_range = None;
        let dn_close = generate_day_night(&star, &planet, 0, 6, 42);
        let dn_far = generate_day_night(&star, &planet, 5, 6, 42);
        assert!(dn_far.cycle_duration_secs > dn_close.cycle_duration_secs,
            "farther orbit should have longer cycle: {} vs {}", dn_far.cycle_duration_secs, dn_close.cycle_duration_secs);
    }
}
```

**Step 2: Add `pub mod generation;` to `src/cosmos/mod.rs`**

**Step 3: Run tests**

Run: `cargo test cosmos::generation`
Expected: All 5 tests PASS.

**Step 4: Commit**

```bash
git add src/cosmos/
git commit -m "feat(cosmos): add system generation with day/night derivation from star + orbit"
```

---

### Task 5: Create `ActiveWorld` resource (rename WorldConfig)

**Files:**
- Modify: `src/registry/world.rs` (rename `WorldConfig` → `ActiveWorld`, add fields)
- Modify: all files that reference `WorldConfig` (mechanical rename)

**Step 1: Rename `WorldConfig` to `ActiveWorld` in `src/registry/world.rs`**

Add `address: CelestialAddress` and `seeds: CelestialSeeds` fields. Keep all existing fields and methods.

```rust
use bevy::prelude::*;
use serde::Deserialize;

use crate::cosmos::address::{CelestialAddress, CelestialSeeds};

/// Active world parameters — the currently loaded celestial body.
/// Previously called WorldConfig.
#[derive(Resource, Debug, Clone)]
pub struct ActiveWorld {
    pub address: CelestialAddress,
    pub seeds: CelestialSeeds,
    pub width_tiles: i32,
    pub height_tiles: i32,
    pub chunk_size: u32,
    pub tile_size: f32,
    pub chunk_load_radius: i32,
    pub planet_type: String,
}

impl ActiveWorld {
    pub fn width_chunks(&self) -> i32 { self.width_tiles / self.chunk_size as i32 }
    pub fn height_chunks(&self) -> i32 { self.height_tiles / self.chunk_size as i32 }
    pub fn wrap_tile_x(&self, tile_x: i32) -> i32 { tile_x.rem_euclid(self.width_tiles) }
    pub fn wrap_chunk_x(&self, chunk_x: i32) -> i32 { chunk_x.rem_euclid(self.width_chunks()) }
    pub fn world_pixel_width(&self) -> f32 { self.width_tiles as f32 * self.tile_size }
    pub fn world_pixel_height(&self) -> f32 { self.height_tiles as f32 * self.tile_size }
}
```

**Step 2: Mechanical rename across codebase**

Every file that references `WorldConfig` or `super::world::WorldConfig` or `crate::registry::world::WorldConfig` must be updated to `ActiveWorld`. Key files:

- `src/registry/loading.rs` — `use super::world::WorldConfig` → `ActiveWorld`, resource insertion
- `src/registry/hot_reload.rs` — `WorldConfig` → `ActiveWorld` in `hot_reload_world` and `hot_reload_planet_type`
- `src/registry/mod.rs` — `WorldConfigAsset` stays (it's the asset type); but references to `WorldConfig` as resource change
- `src/world/ctx.rs` — `WorldCtx.config: Res<WorldConfig>` → `Res<ActiveWorld>`
- `src/world/terrain_gen.rs` — `wc: &WorldConfig` → `wc: &ActiveWorld`
- `src/world/chunk.rs` — `wc: Res<WorldConfig>` → `Res<ActiveWorld>`
- `src/world/rc_lighting.rs` — `world_config: &WorldConfig` → `&ActiveWorld`
- `src/world/autotile.rs` — doc comment reference
- `src/player/mod.rs` — `world_config: Res<WorldConfig>` → `Res<ActiveWorld>`
- `src/player/wrap.rs` — same
- `src/parallax/transition.rs` — same
- `src/camera/follow.rs` — same
- `src/ui/debug_panel.rs` — same
- `src/test_helpers.rs` — `test_world_config()` → `test_active_world()`

**Step 3: Update tests**

Update `test_world_config()` in `test_helpers.rs` to return `ActiveWorld` with default address/seeds:

```rust
pub fn test_active_world() -> ActiveWorld {
    let address = CelestialAddress {
        galaxy: IVec2::ZERO,
        system: IVec2::ZERO,
        orbit: 2,
        satellite: None,
    };
    let seeds = CelestialSeeds::derive(42, &address);
    ActiveWorld {
        address,
        seeds,
        width_tiles: 2048,
        height_tiles: 1024,
        chunk_size: 32,
        tile_size: 32.0,
        chunk_load_radius: 3,
        planet_type: "garden".into(),
    }
}
```

**Step 4: Run tests**

Run: `cargo test`
Expected: All 213+ tests PASS.

**Step 5: Commit**

```bash
git add -A
git commit -m "refactor: rename WorldConfig → ActiveWorld, add CelestialAddress + seeds"
```

---

### Task 6: Move biomes to `content/biomes/`, update loading paths

**Files:**
- Move: `assets/world/biomes/*` → `assets/content/biomes/`
- Modify: `src/registry/loading.rs` (biome loading paths)
- Modify: biome RON files (parallax paths)

**Step 1: Move biome directories**

```bash
git mv assets/world/biomes/meadow    assets/content/biomes/meadow
git mv assets/world/biomes/forest    assets/content/biomes/forest
git mv assets/world/biomes/rocky     assets/content/biomes/rocky
git mv assets/world/biomes/underground_dirt  assets/content/biomes/underground_dirt
git mv assets/world/biomes/underground_rock  assets/content/biomes/underground_rock
git mv assets/world/biomes/core_magma        assets/content/biomes/core_magma
```

**Step 2: Update biome loading path in `loading.rs`**

Change line `format!("world/biomes/{id}/{id}.biome.ron")` → `format!("content/biomes/{id}/{id}.biome.ron")`

**Step 3: Update parallax paths in biome RON files**

Each `*.biome.ron` that has a `parallax` field points to the parallax RON. Update paths from `world/biomes/...` to `content/biomes/...`.

Example in `meadow.biome.ron`:
```
parallax: Some("content/biomes/meadow/meadow.parallax.ron"),
```

**Step 4: Run game briefly or run tests**

Run: `cargo test`
Expected: All tests PASS.

**Step 5: Commit**

```bash
git add -A
git commit -m "refactor: move biomes to content/biomes/, update loading paths"
```

---

### Task 7: Wire cosmos generation into loading pipeline

**Files:**
- Modify: `src/registry/loading.rs` (replace `WorldConfigAsset` loading with cosmos generation)
- Modify: `src/registry/mod.rs` (register new asset types, update `RegistryHandles`)
- Modify: `src/registry/hot_reload.rs` (update `hot_reload_world` for `ActiveWorld`)
- Modify: `src/world/day_night.rs` (remove `include_str!()`, receive generated config)

**Step 1: Update `LoadingAssets` and `start_loading`**

Replace `world_config: Handle<WorldConfigAsset>` with:
```rust
generation_config: Handle<GenerationConfigAsset>,
star_types: Vec<(String, Handle<StarTypeAsset>)>,
```

In `start_loading`:
```rust
let generation_config = asset_server.load::<GenerationConfigAsset>("worlds/generation.ron");
let star_types = vec![
    ("yellow_dwarf".to_string(),
     asset_server.load::<StarTypeAsset>("worlds/star_types/yellow_dwarf/yellow_dwarf.star.ron")),
];
```

**Step 2: Update `check_loading` — generate system instead of reading world.config.ron**

After all base assets loaded:
1. Build star/planet template collections from loaded assets
2. Call `generate_system(universe_seed=42, galaxy=(0,0), system=(0,0), ...)`
3. Find first garden planet in generated system
4. Insert `ActiveWorld` from generated body
5. Insert `DayNightConfig` from generated body's `day_night`
6. Insert `TerrainNoiseCache::new(body_seeds.terrain_seed_u32())`

**Step 3: Update planet type loading path**

Change `format!("world/planet_types/{}.planet.ron", ...)` → `format!("worlds/planet_types/{}/{}.planet.ron", ...)`

**Step 4: Remove `load_day_night_config` system**

In `src/world/day_night.rs`, remove the `load_day_night_config` function (it uses `include_str!()`). The `DayNightConfig` is now inserted by `check_loading` from the generated system.

Remove its registration from wherever it's called (likely `WorldPlugin` in `src/world/mod.rs`).

**Step 5: Update `RegistryHandles`**

Remove `world_config: Handle<WorldConfigAsset>` field, add handles for generation config and star types.

**Step 6: Update hot-reload**

`hot_reload_world` system — remove or simplify. The `ActiveWorld` is no longer loaded from a file, it's generated. For now, hot-reload of world config is not needed (it's procedural). Keep hot-reload for planet types, star types if desired.

**Step 7: Run tests**

Run: `cargo test`
Expected: All tests PASS. The `ron_parse_config` test in `day_night.rs` that uses `include_str!()` on the old file needs to be updated or removed since `day_night.config.ron` is being deleted.

**Step 8: Commit**

```bash
git add -A
git commit -m "feat(cosmos): wire procedural generation into loading pipeline, remove include_str day/night"
```

---

### Task 8: Delete old `assets/world/` directory and clean up

**Files:**
- Delete: `assets/world/world.config.ron`
- Delete: `assets/world/day_night.config.ron`
- Delete: `assets/world/biomes/` (already moved in Task 6)
- Delete: `assets/world/planet_types/` (already moved in Task 3)
- Delete: `assets/world/` directory
- Modify: `src/registry/assets.rs` (remove `WorldConfigAsset` if no longer used)
- Modify: `src/registry/mod.rs` (remove `WorldConfigAsset` registration if unused)

Note: `tiles.registry.ron` elimination is a separate task — if it's still in `assets/world/`, move it to a temp location or keep `assets/world/` alive with just that file until the tiles task is done. Decision depends on whether tiles elimination happens before or after this plan.

**Step 1: Delete old files**

```bash
git rm assets/world/world.config.ron
git rm assets/world/day_night.config.ron
# If biomes/planet_types already moved, rmdir:
rm -rf assets/world/biomes assets/world/planet_types
```

If `tiles.registry.ron` still exists, move it:
```bash
git mv assets/world/tiles.registry.ron assets/worlds/tiles.registry.ron
```
Then delete `assets/world/`.

**Step 2: Clean up unused types**

If `WorldConfigAsset` is no longer loaded anywhere, remove it from `assets.rs` and `mod.rs`. If the `config.ron` extension is no longer used by any other asset, remove its loader registration.

**Step 3: Run tests**

Run: `cargo test`
Expected: All tests PASS.

**Step 4: Commit**

```bash
git add -A
git commit -m "chore: delete old assets/world/ directory, clean up unused WorldConfigAsset"
```

---

### Task 9: Final integration test — verify procedural world loads correctly

**Files:**
- Modify: `src/cosmos/generation.rs` (add integration-style test)

**Step 1: Add end-to-end test**

```rust
#[test]
fn full_pipeline_garden_planet_found() {
    let star = test_star();
    let garden = test_planet_template();
    let barren = PlanetTypeAsset {
        id: "barren".into(),
        primary_biome: "barren".into(),
        secondary_biomes: vec!["barren".into()],
        ..test_planet_template()
    };
    let gen = test_gen_config();
    let mut templates = HashMap::new();
    templates.insert("garden".to_string(), &garden);
    templates.insert("barren".to_string(), &barren);

    // Generate system and verify at least one garden planet exists
    let sys = generate_system(42, IVec2::ZERO, IVec2::ZERO, &[&star], &templates, &gen);
    let has_garden = sys.bodies.iter().any(|b| b.planet_type_id == "garden");
    assert!(has_garden, "system should have at least one garden planet (orbits 2-4 are warm zone)");

    // Verify the garden planet has valid day/night config
    let garden_body = sys.bodies.iter().find(|b| b.planet_type_id == "garden").unwrap();
    let dn = &garden_body.day_night;
    let sum = dn.dawn_ratio + dn.day_ratio + dn.sunset_ratio + dn.night_ratio;
    assert!((sum - 1.0).abs() < 0.001);
    assert!(dn.cycle_duration_secs > 0.0);
    assert!(dn.sun_intensities[1] > 0.0, "day intensity should be positive");
}
```

**Step 2: Run full test suite**

Run: `cargo test`
Expected: All tests PASS (213+ original + ~15 new cosmos tests).

**Step 3: Run the game**

Run: `cargo run`
Expected: Game launches, world generates procedurally, day/night cycle works with generated parameters. Visual result should be similar to before (garden planet with similar but not identical parameters).

**Step 4: Commit**

```bash
git add -A
git commit -m "test(cosmos): add full pipeline integration test"
```

---

## Summary

| Task | Description | Files | Tests |
|------|-------------|-------|-------|
| 1 | CelestialAddress + CelestialSeeds | 3 new | 7 |
| 2 | StarTypeAsset + GenerationConfigAsset + RON files | 4 new, 1 mod | 2 |
| 3 | PlanetTypeAsset Optional fields + barren type | 2 new, 2 mod | existing pass |
| 4 | System generation logic | 1 new | 5 |
| 5 | WorldConfig → ActiveWorld rename | ~15 mod | all pass |
| 6 | Move biomes to content/biomes/ | moves + 1 mod | all pass |
| 7 | Wire cosmos into loading pipeline | 4 mod | all pass |
| 8 | Delete old assets/world/ | deletes + cleanup | all pass |
| 9 | Integration test | 1 mod | 1 new |
