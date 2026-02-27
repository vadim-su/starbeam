# Biome System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Data-driven biome system with planet types, horizontal region distribution, 4 vertical layers, per-biome parallax with crossfade transitions.

**Architecture:** Planet type RON defines which biomes appear on a world. At world gen time, a BiomeMap of horizontal regions is built from seed. Terrain gen reads biome at each tile coordinate to select blocks. Parallax layers are per-biome with crossfade on biome change.

**Tech Stack:** Rust, Bevy 0.18, RON (serde), Perlin noise

**Design doc:** `docs/plans/2026-02-27-biome-system-design.md`

---

### Task 1: BiomeMap — Region Generation Algorithm

Pure data structures and algorithm, fully testable without Bevy.

**Files:**
- Create: `src/world/biome_map.rs`
- Modify: `src/world/mod.rs` (add `pub mod biome_map;`)

**Step 1: Write failing tests**

In `src/world/biome_map.rs`, write tests at bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_regions() {
        let map = BiomeMap::generate(
            "meadow",
            &["forest", "rocky"],
            42,
            2048,
            300,
            600,
            0.6,
        );
        assert!(!map.regions.is_empty());
    }

    #[test]
    fn regions_cover_entire_width() {
        let map = BiomeMap::generate("meadow", &["forest", "rocky"], 42, 2048, 300, 600, 0.6);
        let total: u32 = map.regions.iter().map(|r| r.width).sum();
        assert_eq!(total, 2048);
    }

    #[test]
    fn regions_start_x_is_contiguous() {
        let map = BiomeMap::generate("meadow", &["forest", "rocky"], 42, 2048, 300, 600, 0.6);
        let mut expected_start = 0u32;
        for region in &map.regions {
            assert_eq!(region.start_x, expected_start);
            expected_start += region.width;
        }
    }

    #[test]
    fn no_adjacent_same_biome() {
        let map = BiomeMap::generate("meadow", &["forest", "rocky"], 42, 2048, 300, 600, 0.6);
        for i in 1..map.regions.len() {
            assert_ne!(
                map.regions[i].biome_id, map.regions[i - 1].biome_id,
                "Adjacent regions {} and {} have same biome '{}'",
                i - 1, i, map.regions[i].biome_id
            );
        }
    }

    #[test]
    fn first_last_region_differ_for_wrap() {
        let map = BiomeMap::generate("meadow", &["forest", "rocky"], 42, 2048, 300, 600, 0.6);
        if map.regions.len() > 1 {
            assert_ne!(
                map.regions.first().unwrap().biome_id,
                map.regions.last().unwrap().biome_id,
                "First and last regions must differ for cylindrical wrap"
            );
        }
    }

    #[test]
    fn primary_biome_ratio_approximately_correct() {
        let map = BiomeMap::generate("meadow", &["forest", "rocky"], 42, 2048, 300, 600, 0.6);
        let primary_width: u32 = map.regions.iter()
            .filter(|r| r.biome_id == "meadow")
            .map(|r| r.width)
            .sum();
        let ratio = primary_width as f64 / 2048.0;
        // Allow ±20% tolerance due to discrete region widths
        assert!(ratio > 0.35 && ratio < 0.85, "Primary ratio was {ratio}");
    }

    #[test]
    fn biome_at_returns_correct_biome() {
        let map = BiomeMap::generate("meadow", &["forest", "rocky"], 42, 2048, 300, 600, 0.6);
        let first = &map.regions[0];
        assert_eq!(map.biome_at(first.start_x), first.biome_id);
        assert_eq!(map.biome_at(first.start_x + first.width - 1), first.biome_id);
        if map.regions.len() > 1 {
            let second = &map.regions[1];
            assert_eq!(map.biome_at(second.start_x), second.biome_id);
        }
    }

    #[test]
    fn biome_at_wraps_around() {
        let map = BiomeMap::generate("meadow", &["forest", "rocky"], 42, 2048, 300, 600, 0.6);
        // Past world width should wrap
        let at_0 = map.biome_at(0);
        let at_2048 = map.biome_at(2048);
        assert_eq!(at_0, at_2048);
    }

    #[test]
    fn deterministic_generation() {
        let map1 = BiomeMap::generate("meadow", &["forest", "rocky"], 42, 2048, 300, 600, 0.6);
        let map2 = BiomeMap::generate("meadow", &["forest", "rocky"], 42, 2048, 300, 600, 0.6);
        assert_eq!(map1.regions.len(), map2.regions.len());
        for (r1, r2) in map1.regions.iter().zip(map2.regions.iter()) {
            assert_eq!(r1.biome_id, r2.biome_id);
            assert_eq!(r1.start_x, r2.start_x);
            assert_eq!(r1.width, r2.width);
        }
    }

    #[test]
    fn different_seed_different_result() {
        let map1 = BiomeMap::generate("meadow", &["forest", "rocky"], 42, 2048, 300, 600, 0.6);
        let map2 = BiomeMap::generate("meadow", &["forest", "rocky"], 99, 2048, 300, 600, 0.6);
        // At least one region should differ (extremely unlikely to be identical)
        let same = map1.regions.iter().zip(map2.regions.iter())
            .all(|(r1, r2)| r1.biome_id == r2.biome_id && r1.width == r2.width);
        assert!(!same, "Different seeds should produce different maps");
    }

    #[test]
    fn region_index_at_returns_index() {
        let map = BiomeMap::generate("meadow", &["forest", "rocky"], 42, 2048, 300, 600, 0.6);
        assert_eq!(map.region_index_at(0), 0);
        let second_start = map.regions[1].start_x;
        assert_eq!(map.region_index_at(second_start), 1);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib world::biome_map -- --nocapture`
Expected: FAIL — module and types don't exist

**Step 3: Write implementation**

```rust
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone)]
pub struct BiomeRegion {
    pub biome_id: String,
    pub start_x: u32,
    pub width: u32,
}

#[derive(Debug, Clone)]
pub struct BiomeMap {
    pub regions: Vec<BiomeRegion>,
    world_width: u32,
}

impl BiomeMap {
    /// Generate biome regions from planet type parameters and world seed.
    ///
    /// - `primary`: the dominant biome ID
    /// - `secondaries`: secondary biome IDs
    /// - `seed`: world seed for deterministic RNG
    /// - `world_width`: total world width in tiles
    /// - `region_min`/`region_max`: min/max region width in tiles
    /// - `primary_ratio`: fraction of regions allocated to primary biome (0.0..1.0)
    pub fn generate(
        primary: &str,
        secondaries: &[&str],
        seed: u64,
        world_width: u32,
        region_min: u32,
        region_max: u32,
        primary_ratio: f64,
    ) -> Self {
        let mut rng = SimpleRng::new(seed);
        let avg_width = (region_min + region_max) / 2;
        let region_count = (world_width / avg_width).max(2) as usize;

        // Allocate biome IDs to slots
        let primary_count = ((region_count as f64 * primary_ratio).round() as usize).max(1);
        let secondary_count = region_count - primary_count;

        let mut biome_ids: Vec<String> = Vec::with_capacity(region_count);
        for _ in 0..primary_count {
            biome_ids.push(primary.to_string());
        }
        for i in 0..secondary_count {
            if secondaries.is_empty() {
                biome_ids.push(primary.to_string());
            } else {
                let idx = i % secondaries.len();
                biome_ids.push(secondaries[idx].to_string());
            }
        }

        // Shuffle to distribute (Fisher-Yates)
        for i in (1..biome_ids.len()).rev() {
            let j = (rng.next() % (i as u64 + 1)) as usize;
            biome_ids.swap(i, j);
        }

        // Fix adjacent duplicates
        for pass in 0..biome_ids.len() * 2 {
            let mut fixed = true;
            for i in 1..biome_ids.len() {
                if biome_ids[i] == biome_ids[i - 1] {
                    // Find a non-adjacent swap target
                    let mut swapped = false;
                    for j in (i + 1)..biome_ids.len() {
                        if biome_ids[j] != biome_ids[i]
                            && (j + 1 >= biome_ids.len() || biome_ids[j] != biome_ids[j + 1])
                            && (i == 0 || biome_ids[j] != biome_ids[i - 1])
                        {
                            biome_ids.swap(i, j);
                            swapped = true;
                            break;
                        }
                    }
                    if !swapped {
                        fixed = false;
                    }
                }
            }
            // Fix wrap-around (first == last)
            if biome_ids.len() > 1 && biome_ids.first() == biome_ids.last() {
                for j in 1..biome_ids.len() - 1 {
                    let last = biome_ids.len() - 1;
                    if biome_ids[j] != biome_ids[last]
                        && biome_ids[j] != biome_ids[0]
                        && (j == 0 || biome_ids[last] != biome_ids[j - 1])
                        && (j + 1 >= biome_ids.len() || biome_ids[last] != biome_ids[j + 1])
                    {
                        biome_ids.swap(last, j);
                        break;
                    }
                }
            }
            if fixed && (biome_ids.len() <= 1 || biome_ids.first() != biome_ids.last()) {
                break;
            }
            if pass > biome_ids.len() * 2 - 2 {
                break; // give up after many passes — rare edge case with few biome types
            }
        }

        // Assign widths
        let mut widths: Vec<u32> = (0..biome_ids.len())
            .map(|_| {
                if region_max > region_min {
                    region_min + (rng.next() % (region_max - region_min) as u64) as u32
                } else {
                    region_min
                }
            })
            .collect();

        // Adjust to fill world exactly
        let total: u32 = widths.iter().sum();
        if total != world_width {
            let last = widths.len() - 1;
            if total < world_width {
                widths[last] += world_width - total;
            } else {
                let excess = total - world_width;
                if widths[last] > excess + region_min {
                    widths[last] -= excess;
                } else {
                    // Distribute reduction across regions
                    let mut remaining = excess;
                    for w in widths.iter_mut().rev() {
                        let can_reduce = w.saturating_sub(region_min);
                        let reduce = can_reduce.min(remaining);
                        *w -= reduce;
                        remaining -= reduce;
                        if remaining == 0 { break; }
                    }
                }
            }
        }

        // Build regions
        let mut regions = Vec::with_capacity(biome_ids.len());
        let mut start_x = 0u32;
        for (id, width) in biome_ids.into_iter().zip(widths.into_iter()) {
            regions.push(BiomeRegion {
                biome_id: id,
                start_x,
                width,
            });
            start_x += width;
        }

        Self { regions, world_width }
    }

    /// O(log n) lookup of biome at tile X coordinate.
    pub fn biome_at(&self, tile_x: u32) -> &str {
        let wrapped = tile_x % self.world_width;
        let idx = self.region_index_at(wrapped);
        &self.regions[idx].biome_id
    }

    /// O(log n) lookup of region index at tile X coordinate.
    pub fn region_index_at(&self, tile_x: u32) -> usize {
        let wrapped = tile_x % self.world_width;
        match self.regions.binary_search_by(|r| r.start_x.cmp(&wrapped)) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }
}

/// Minimal deterministic RNG (splitmix64) — no external crate needed.
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib world::biome_map -- --nocapture`
Expected: all 10 tests PASS

**Step 5: Add module to world/mod.rs**

Add `pub mod biome_map;` to `src/world/mod.rs`.

**Step 6: Commit**

```bash
git add src/world/biome_map.rs src/world/mod.rs
git commit -m "feat(biome): add BiomeMap region generation algorithm with tests"
```

---

### Task 2: Asset Types — PlanetTypeAsset & BiomeAsset

RON-deserializable asset structs and loader registration.

**Files:**
- Modify: `src/registry/assets.rs` (add new asset structs)
- Modify: `src/registry/mod.rs` (register loaders)

**Step 1: Add PlanetTypeAsset and BiomeAsset to `src/registry/assets.rs`**

Append after `AutotileAsset`:

```rust
/// Layer configuration within a planet type.
#[derive(Debug, Clone, Deserialize)]
pub struct LayerConfigAsset {
    pub primary_biome: Option<String>,
    pub terrain_frequency: f64,
    pub terrain_amplitude: f64,
}

/// All 4 vertical layers.
#[derive(Debug, Clone, Deserialize)]
pub struct LayersAsset {
    pub surface: LayerConfigAsset,
    pub underground: LayerConfigAsset,
    pub deep_underground: LayerConfigAsset,
    pub core: LayerConfigAsset,
}

/// Asset loaded from *.planet.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct PlanetTypeAsset {
    pub id: String,
    pub primary_biome: String,
    pub secondary_biomes: Vec<String>,
    pub layers: LayersAsset,
    pub region_width_min: u32,
    pub region_width_max: u32,
    pub primary_region_ratio: f64,
}

/// Asset loaded from *.biome.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct BiomeAsset {
    pub id: String,
    pub surface_block: String,
    pub subsurface_block: String,
    pub subsurface_depth: i32,
    pub fill_block: String,
    pub cave_threshold: f64,
    pub parallax: Option<String>,
    // Future fields — not implemented in MVP
    #[serde(default)]
    pub weather: Option<Vec<String>>,
    #[serde(default)]
    pub music: Option<Vec<String>>,
    #[serde(default)]
    pub ambient: Option<Vec<String>>,
    #[serde(default)]
    pub placeables: Option<Vec<String>>,
    #[serde(default)]
    pub monsters: Option<Vec<String>>,
    #[serde(default)]
    pub status_effects: Option<Vec<String>>,
}
```

**Step 2: Register loaders in `src/registry/mod.rs`**

In `RegistryPlugin::build`, add after existing `.init_asset` / `.register_asset_loader` lines:

```rust
.init_asset::<PlanetTypeAsset>()
.init_asset::<BiomeAsset>()
.register_asset_loader(RonLoader::<PlanetTypeAsset>::new(&["planet.ron"]))
.register_asset_loader(RonLoader::<BiomeAsset>::new(&["biome.ron"]))
```

And update the imports at the top to include `PlanetTypeAsset, BiomeAsset`.

**Step 3: Verify compilation**

Run: `cargo build 2>&1 | head -20`
Expected: compiles with no errors

**Step 4: Commit**

```bash
git add src/registry/assets.rs src/registry/mod.rs
git commit -m "feat(biome): add PlanetTypeAsset and BiomeAsset RON asset types"
```

---

### Task 3: BiomeRegistry & Runtime Resources

Runtime resources built from loaded assets.

**Files:**
- Create: `src/registry/biome.rs`
- Modify: `src/registry/mod.rs` (add `pub mod biome;`)

**Step 1: Write BiomeRegistry with tests**

```rust
use std::collections::HashMap;

use bevy::prelude::*;

use crate::registry::tile::TileId;

/// Runtime definition of a biome, built from BiomeAsset + TileRegistry lookups.
#[derive(Debug, Clone)]
pub struct BiomeDef {
    pub id: String,
    pub surface_block: TileId,
    pub subsurface_block: TileId,
    pub subsurface_depth: i32,
    pub fill_block: TileId,
    pub cave_threshold: f64,
    pub parallax_path: Option<String>,
}

/// All loaded biome definitions keyed by biome ID.
#[derive(Resource, Debug, Default)]
pub struct BiomeRegistry {
    pub biomes: HashMap<String, BiomeDef>,
}

impl BiomeRegistry {
    pub fn get(&self, id: &str) -> &BiomeDef {
        self.biomes
            .get(id)
            .unwrap_or_else(|| panic!("Unknown biome: {id}"))
    }

    pub fn get_opt(&self, id: &str) -> Option<&BiomeDef> {
        self.biomes.get(id)
    }
}

/// Runtime planet type data, built from PlanetTypeAsset.
#[derive(Resource, Debug, Clone)]
pub struct PlanetConfig {
    pub id: String,
    pub primary_biome: String,
    pub secondary_biomes: Vec<String>,
    pub layers: LayerConfigs,
    pub region_width_min: u32,
    pub region_width_max: u32,
    pub primary_region_ratio: f64,
}

#[derive(Debug, Clone)]
pub struct LayerConfig {
    pub primary_biome: Option<String>,
    pub terrain_frequency: f64,
    pub terrain_amplitude: f64,
}

#[derive(Debug, Clone)]
pub struct LayerConfigs {
    pub surface: LayerConfig,
    pub underground: LayerConfig,
    pub deep_underground: LayerConfig,
    pub core: LayerConfig,
}

/// Determines which vertical layer a tile_y coordinate belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldLayer {
    Core,
    DeepUnderground,
    Underground,
    Surface,
}

impl WorldLayer {
    /// Layer boundaries as fractions of world height (from bottom):
    /// Core: 0-12%, Deep: 12-37%, Underground: 37-70%, Surface: 70-100%
    pub fn from_tile_y(tile_y: i32, world_height: i32) -> Self {
        let ratio = tile_y as f64 / world_height as f64;
        if ratio < 0.12 {
            WorldLayer::Core
        } else if ratio < 0.37 {
            WorldLayer::DeepUnderground
        } else if ratio < 0.70 {
            WorldLayer::Underground
        } else {
            WorldLayer::Surface
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_layer_boundaries() {
        assert_eq!(WorldLayer::from_tile_y(0, 1024), WorldLayer::Core);
        assert_eq!(WorldLayer::from_tile_y(100, 1024), WorldLayer::Core);
        assert_eq!(WorldLayer::from_tile_y(130, 1024), WorldLayer::DeepUnderground);
        assert_eq!(WorldLayer::from_tile_y(380, 1024), WorldLayer::Underground);
        assert_eq!(WorldLayer::from_tile_y(720, 1024), WorldLayer::Surface);
        assert_eq!(WorldLayer::from_tile_y(1023, 1024), WorldLayer::Surface);
    }

    #[test]
    fn biome_registry_get() {
        let mut reg = BiomeRegistry::default();
        reg.biomes.insert("meadow".into(), BiomeDef {
            id: "meadow".into(),
            surface_block: TileId(1),
            subsurface_block: TileId(2),
            subsurface_depth: 4,
            fill_block: TileId(3),
            cave_threshold: 0.3,
            parallax_path: Some("biomes/meadow/parallax.ron".into()),
        });
        let def = reg.get("meadow");
        assert_eq!(def.id, "meadow");
        assert_eq!(def.surface_block, TileId(1));
    }

    #[test]
    fn biome_registry_get_opt_none() {
        let reg = BiomeRegistry::default();
        assert!(reg.get_opt("missing").is_none());
    }

    #[test]
    #[should_panic(expected = "Unknown biome: missing")]
    fn biome_registry_get_panics() {
        let reg = BiomeRegistry::default();
        reg.get("missing");
    }
}
```

**Step 2: Run tests**

Run: `cargo test --lib registry::biome -- --nocapture`
Expected: all 4 tests PASS

**Step 3: Add module to registry/mod.rs**

Add `pub mod biome;` to `src/registry/mod.rs`.

**Step 4: Commit**

```bash
git add src/registry/biome.rs src/registry/mod.rs
git commit -m "feat(biome): add BiomeRegistry, PlanetConfig, and WorldLayer runtime types"
```

---

### Task 4: RON Content Files

Create all RON configs and placeholder background PNGs.

**Files:**
- Create: `assets/world/planet_types/garden.planet.ron`
- Create: `assets/world/biomes/meadow/meadow.biome.ron`
- Create: `assets/world/biomes/meadow/parallax.ron`
- Create: `assets/world/biomes/forest/forest.biome.ron`
- Create: `assets/world/biomes/forest/parallax.ron`
- Create: `assets/world/biomes/rocky/rocky.biome.ron`
- Create: `assets/world/biomes/rocky/parallax.ron`
- Create: `assets/world/biomes/underground_dirt/underground_dirt.biome.ron`
- Create: `assets/world/biomes/underground_rock/underground_rock.biome.ron`
- Create: `assets/world/biomes/core_magma/core_magma.biome.ron`
- Modify: `assets/world/world.config.ron` (add `planet_type` field)

**Step 1: Create garden.planet.ron**

```ron
(
    id: "garden",
    primary_biome: "meadow",
    secondary_biomes: ["forest", "rocky"],
    layers: (
        surface: (
            primary_biome: None,
            terrain_frequency: 0.02,
            terrain_amplitude: 40.0,
        ),
        underground: (
            primary_biome: Some("underground_dirt"),
            terrain_frequency: 0.07,
            terrain_amplitude: 1.0,
        ),
        deep_underground: (
            primary_biome: Some("underground_rock"),
            terrain_frequency: 0.05,
            terrain_amplitude: 1.0,
        ),
        core: (
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

**Step 2: Create surface biome RONs**

`assets/world/biomes/meadow/meadow.biome.ron`:
```ron
(
    id: "meadow",
    surface_block: "grass",
    subsurface_block: "dirt",
    subsurface_depth: 4,
    fill_block: "stone",
    cave_threshold: 0.3,
    parallax: Some("world/biomes/meadow/parallax.ron"),
)
```

`assets/world/biomes/meadow/parallax.ron`:
```ron
(
    layers: [
        ( name: "sky", image: "world/biomes/meadow/backgrounds/sky.png", speed_x: 0.0, speed_y: 0.0, repeat_x: false, repeat_y: false, z_order: -100.0 ),
        ( name: "far_hills", image: "world/biomes/meadow/backgrounds/far_hills.png", speed_x: 0.1, speed_y: 0.05, repeat_x: true, repeat_y: false, z_order: -90.0 ),
        ( name: "near_hills", image: "world/biomes/meadow/backgrounds/near_hills.png", speed_x: 0.3, speed_y: 0.15, repeat_x: true, repeat_y: false, z_order: -80.0 ),
    ],
)
```

`assets/world/biomes/forest/forest.biome.ron`:
```ron
(
    id: "forest",
    surface_block: "grass",
    subsurface_block: "dirt",
    subsurface_depth: 4,
    fill_block: "stone",
    cave_threshold: 0.3,
    parallax: Some("world/biomes/forest/parallax.ron"),
)
```

`assets/world/biomes/forest/parallax.ron`:
```ron
(
    layers: [
        ( name: "sky", image: "world/biomes/forest/backgrounds/sky.png", speed_x: 0.0, speed_y: 0.0, repeat_x: false, repeat_y: false, z_order: -100.0 ),
        ( name: "far_trees", image: "world/biomes/forest/backgrounds/far_trees.png", speed_x: 0.1, speed_y: 0.05, repeat_x: true, repeat_y: false, z_order: -90.0 ),
        ( name: "near_trees", image: "world/biomes/forest/backgrounds/near_trees.png", speed_x: 0.3, speed_y: 0.15, repeat_x: true, repeat_y: false, z_order: -80.0 ),
    ],
)
```

`assets/world/biomes/rocky/rocky.biome.ron`:
```ron
(
    id: "rocky",
    surface_block: "stone",
    subsurface_block: "stone",
    subsurface_depth: 2,
    fill_block: "stone",
    cave_threshold: 0.3,
    parallax: Some("world/biomes/rocky/parallax.ron"),
)
```

`assets/world/biomes/rocky/parallax.ron`:
```ron
(
    layers: [
        ( name: "sky", image: "world/biomes/rocky/backgrounds/sky.png", speed_x: 0.0, speed_y: 0.0, repeat_x: false, repeat_y: false, z_order: -100.0 ),
        ( name: "far_rocks", image: "world/biomes/rocky/backgrounds/far_rocks.png", speed_x: 0.1, speed_y: 0.05, repeat_x: true, repeat_y: false, z_order: -90.0 ),
        ( name: "near_rocks", image: "world/biomes/rocky/backgrounds/near_rocks.png", speed_x: 0.3, speed_y: 0.15, repeat_x: true, repeat_y: false, z_order: -80.0 ),
    ],
)
```

**Step 3: Create underground biome RONs (no parallax)**

`assets/world/biomes/underground_dirt/underground_dirt.biome.ron`:
```ron
(
    id: "underground_dirt",
    surface_block: "dirt",
    subsurface_block: "dirt",
    subsurface_depth: 0,
    fill_block: "stone",
    cave_threshold: 0.3,
    parallax: None,
)
```

`assets/world/biomes/underground_rock/underground_rock.biome.ron`:
```ron
(
    id: "underground_rock",
    surface_block: "stone",
    subsurface_block: "stone",
    subsurface_depth: 0,
    fill_block: "stone",
    cave_threshold: 0.25,
    parallax: None,
)
```

`assets/world/biomes/core_magma/core_magma.biome.ron`:
```ron
(
    id: "core_magma",
    surface_block: "stone",
    subsurface_block: "stone",
    subsurface_depth: 0,
    fill_block: "stone",
    cave_threshold: 0.15,
    parallax: None,
)
```

**Step 4: Update world.config.ron**

Add `planet_type` field:
```ron
(
  width_tiles: 2048,
  height_tiles: 1024,
  chunk_size: 32,
  tile_size: 8.0,
  chunk_load_radius: 3,
  seed: 42,
  planet_type: "garden",
)
```

And update `WorldConfigAsset` in `src/registry/assets.rs` to add:
```rust
pub planet_type: String,
```

And `WorldConfig` in `src/registry/world.rs`:
```rust
pub planet_type: String,
```

And `check_loading` in `src/registry/mod.rs` to pass it:
```rust
planet_type: world_cfg.planet_type.clone(),
```

And `hot_reload_world` to include it.

**Step 5: Generate placeholder background PNGs**

Create a small Python script or use ImageMagick to generate simple colored gradient PNGs for each biome. Each biome needs 3 PNGs (sky, far, near).

Meadow: blue sky, green far hills, bright green near hills
Forest: darker blue sky, dark green far trees, dark green near trees
Rocky: gray sky, gray far rocks, brown near rocks

Use `convert` (ImageMagick) one-liners, e.g.:
```bash
convert -size 1280x720 gradient:'#87CEEB-#4A90D9' sky.png
convert -size 1280x360 gradient:'#228B22-#006400' far_hills.png
```

Or a Python script using Pillow.

**Step 6: Verify all RON files parse**

Run: `cargo build`
Expected: compiles (RON files are validated at runtime, not compile time, but asset types must compile)

**Step 7: Commit**

```bash
git add assets/world/ src/registry/assets.rs src/registry/world.rs src/registry/mod.rs
git commit -m "feat(biome): add RON content files for garden planet type with 6 biomes"
```

---

### Task 5: Loading Pipeline — LoadingBiomes State

New state between Loading and LoadingAutotile that loads planet type + all biomes.

**Files:**
- Modify: `src/registry/mod.rs` (add LoadingBiomes state, loading systems)

**Step 1: Add LoadingBiomes to AppState**

```rust
pub enum AppState {
    #[default]
    Loading,
    LoadingBiomes,     // NEW
    LoadingAutotile,
    InGame,
}
```

**Step 2: Update `check_loading` to transition to LoadingBiomes**

Change: `next_state.set(AppState::LoadingAutotile)` → `next_state.set(AppState::LoadingBiomes)`

Also: load the planet type RON in `check_loading`:
- After reading `world_cfg.planet_type`, load `planet_types/{planet_type}.planet.ron`
- Store the handle in a new `LoadingBiomeAssets` resource

**Step 3: Add `start_loading_biomes` system (OnEnter LoadingBiomes)**

This system:
1. Reads the PlanetTypeAsset (wait for it to load)
2. Collects all biome IDs (primary + secondary + all layer biomes)
3. Loads each `biomes/{id}/{id}.biome.ron`
4. Loads each biome's parallax RON (if specified)
5. Stores handles in `LoadingBiomeAssets`

**Step 4: Add `check_biomes_loaded` system (Update, run_if LoadingBiomes)**

When all biome + parallax assets are loaded:
1. Build `BiomeRegistry` from BiomeAssets + TileRegistry
2. Build `PlanetConfig` from PlanetTypeAsset
3. Build `BiomeMap` from PlanetConfig + WorldConfig seed
4. Store per-biome `ParallaxConfig`s in a new `BiomeParallaxConfigs` resource
5. Insert all as resources
6. Transition to `LoadingAutotile`

**Step 5: Add resource types**

```rust
#[derive(Resource)]
struct LoadingBiomeAssets {
    planet_type: Handle<PlanetTypeAsset>,
    biomes: Vec<(String, Handle<BiomeAsset>)>,
    parallax_configs: Vec<(String, Handle<ParallaxConfigAsset>)>,
}

/// Per-biome parallax configs, keyed by biome ID.
#[derive(Resource, Debug, Default)]
pub struct BiomeParallaxConfigs {
    pub configs: HashMap<String, ParallaxConfig>,
}
```

**Step 6: Register in plugin**

```rust
.add_systems(OnEnter(AppState::LoadingBiomes), start_loading_biomes)
.add_systems(Update, check_biomes_loaded.run_if(in_state(AppState::LoadingBiomes)))
```

**Step 7: Verify compilation and log output**

Run: `cargo build`
Expected: compiles

Run: `cargo run` (briefly, Ctrl-C after InGame)
Expected: log shows "Loading biomes...", "BiomeMap generated with N regions", "Entering LoadingAutotile..."

**Step 8: Commit**

```bash
git add src/registry/
git commit -m "feat(biome): add LoadingBiomes state with planet type and biome asset loading"
```

---

### Task 6: Terrain Gen Rework — Biome-Driven Tile Selection

Replace hardcoded tile logic with biome lookups.

**Files:**
- Modify: `src/world/terrain_gen.rs` (complete rework)

**Step 1: Update function signatures**

`generate_tile` new signature:
```rust
pub fn generate_tile(
    seed: u32,
    tile_x: i32,
    tile_y: i32,
    wc: &WorldConfig,
    biome_map: &BiomeMap,
    biome_registry: &BiomeRegistry,
    tile_registry: &TileRegistry,
    planet_config: &PlanetConfig,
) -> TileId
```

`generate_chunk_tiles` same change.

`surface_height` — takes `terrain_frequency` and `terrain_amplitude` from layer config instead of constants. The surface layer config's frequency/amplitude used for surface height.

**Step 2: New generate_tile logic**

```rust
pub fn generate_tile(
    seed: u32,
    tile_x: i32,
    tile_y: i32,
    wc: &WorldConfig,
    biome_map: &BiomeMap,
    biome_registry: &BiomeRegistry,
    tile_registry: &TileRegistry,
    planet_config: &PlanetConfig,
) -> TileId {
    if tile_y < 0 || tile_y >= wc.height_tiles {
        return TileId::AIR;
    }

    let tile_x = wc.wrap_tile_x(tile_x);

    // Determine vertical layer
    let layer = WorldLayer::from_tile_y(tile_y, wc.height_tiles);

    // Get biome for this position
    let biome_id = match layer {
        WorldLayer::Surface => biome_map.biome_at(tile_x as u32),
        WorldLayer::Underground => planet_config.layers.underground.primary_biome
            .as_deref().unwrap_or("underground_dirt"),
        WorldLayer::DeepUnderground => planet_config.layers.deep_underground.primary_biome
            .as_deref().unwrap_or("underground_rock"),
        WorldLayer::Core => planet_config.layers.core.primary_biome
            .as_deref().unwrap_or("core_magma"),
    };

    let biome = biome_registry.get(biome_id);

    // Surface height (using surface layer params)
    let surface_y = surface_height(
        seed, tile_x, wc,
        planet_config.layers.surface.terrain_frequency,
        planet_config.layers.surface.terrain_amplitude,
    );

    if tile_y > surface_y {
        return TileId::AIR;
    }

    // Surface layer tile assignment
    if layer == WorldLayer::Surface {
        if tile_y == surface_y {
            return biome.surface_block;
        }
        if tile_y > surface_y - biome.subsurface_depth {
            return biome.subsurface_block;
        }
    }

    // Cave check
    let cave_perlin = Perlin::new(seed.wrapping_add(1));
    let layer_freq = match layer {
        WorldLayer::Surface => planet_config.layers.surface.terrain_frequency,
        WorldLayer::Underground => planet_config.layers.underground.terrain_frequency,
        WorldLayer::DeepUnderground => planet_config.layers.deep_underground.terrain_frequency,
        WorldLayer::Core => planet_config.layers.core.terrain_frequency,
    };
    let angle = tile_x as f64 / wc.width_tiles as f64 * 2.0 * std::f64::consts::PI;
    let radius = wc.width_tiles as f64 * layer_freq / (2.0 * std::f64::consts::PI);
    let cave_val = cave_perlin.get([
        radius * angle.cos(),
        radius * angle.sin(),
        tile_y as f64 * layer_freq,
    ]);
    if cave_val.abs() < biome.cave_threshold {
        TileId::AIR
    } else {
        biome.fill_block
    }
}
```

**Step 3: Update surface_height**

```rust
pub fn surface_height(
    seed: u32,
    tile_x: i32,
    wc: &WorldConfig,
    frequency: f64,
    amplitude: f64,
) -> i32 {
    let perlin = Perlin::new(seed);
    let base = SURFACE_BASE * wc.height_tiles as f64;
    let angle = tile_x as f64 / wc.width_tiles as f64 * 2.0 * std::f64::consts::PI;
    let radius = wc.width_tiles as f64 * frequency / (2.0 * std::f64::consts::PI);
    let nx = radius * angle.cos();
    let ny = radius * angle.sin();
    let noise_val = perlin.get([nx, ny]);
    (base + noise_val * amplitude) as i32
}
```

**Step 4: Update tests**

All terrain_gen tests need to be updated to use BiomeMap/BiomeRegistry/PlanetConfig instead of TerrainTiles. Create helper functions that build test BiomeMap, BiomeRegistry, and PlanetConfig from the existing 4-tile pattern.

**Step 5: Run tests**

Run: `cargo test --lib world::terrain_gen -- --nocapture`
Expected: all tests PASS

**Step 6: Commit**

```bash
git add src/world/terrain_gen.rs
git commit -m "feat(biome): rework terrain gen to use biome-driven tile selection"
```

---

### Task 7: Chunk System Updates

Update WorldMap and chunk systems to use new terrain gen signature. Remove TerrainTiles dependency.

**Files:**
- Modify: `src/world/chunk.rs` (update all signatures)
- Modify: `src/world/mod.rs` (if needed)
- Modify: `src/interaction/block_action.rs` (remove TerrainTiles usage)

**Step 1: Update WorldMap methods**

All methods that take `&TerrainTiles` now take `&BiomeMap`, `&BiomeRegistry`, `&PlanetConfig` instead.

`get_or_generate_chunk`:
```rust
pub fn get_or_generate_chunk(
    &mut self,
    chunk_x: i32,
    chunk_y: i32,
    wc: &WorldConfig,
    biome_map: &BiomeMap,
    biome_registry: &BiomeRegistry,
    tile_registry: &TileRegistry,
    planet_config: &PlanetConfig,
) -> &ChunkData
```

`get_tile`, `set_tile`, `is_solid`, `get_tile_if_loaded` — update similarly.

Note: `get_tile_if_loaded` and `get_tile` for out-of-bounds Y return `TileId::AIR` (above) or `TileId(stone_id)` (below). Since stone TileId depends on registry, we need to pass `&TileRegistry` and use `tile_registry.by_name("stone")` or just return a fixed "bedrock" tile. Simplest: for `tile_y < 0` return the fill_block of the core biome, for `tile_y >= height` return AIR.

**Step 2: Update `chunk_loading_system` and `spawn_chunk`**

Add `biome_map: Res<BiomeMap>`, `biome_registry: Res<BiomeRegistry>`, `planet_config: Res<PlanetConfig>` as system params.

Remove `tt: Res<TerrainTiles>`.

**Step 3: Update `update_bitmasks_around` and `init_chunk_bitmasks`**

Same signature changes — remove `&TerrainTiles`, add biome resources.

**Step 4: Update `block_action.rs`**

Remove `terrain_tiles: Res<TerrainTiles>`. Add biome resources. All calls to `world_map.get_tile`, `world_map.set_tile`, `update_bitmasks_around` updated.

**Step 5: Remove TerrainTiles**

Delete `TerrainTiles` struct from `src/registry/tile.rs`.
Remove construction in `check_loading` and `hot_reload_tiles` in `src/registry/mod.rs`.

**Step 6: Run full test suite**

Run: `cargo test`
Expected: all tests PASS

**Step 7: Commit**

```bash
git add src/world/ src/interaction/ src/registry/
git commit -m "feat(biome): update chunk system and interactions to use BiomeMap/BiomeRegistry"
```

---

### Task 8: Parallax Rework — Per-Biome Backgrounds with Crossfade

Replace global parallax with per-biome parallax and crossfade transitions.

**Files:**
- Modify: `src/parallax/mod.rs` (add transition systems)
- Modify: `src/parallax/spawn.rs` (biome-aware spawning)
- Modify: `src/parallax/config.rs` (add biome_id to ParallaxLayer)
- Create: `src/parallax/transition.rs` (transition logic)
- Modify: `src/registry/mod.rs` (remove global parallax loading)

**Step 1: Add CurrentBiome and ParallaxTransition resources**

In `src/parallax/transition.rs`:

```rust
use bevy::prelude::*;

/// Tracks which biome the player is currently in.
#[derive(Resource, Debug, Default)]
pub struct CurrentBiome {
    pub biome_id: String,
    pub region_index: usize,
}

/// Active parallax crossfade transition.
#[derive(Resource, Debug)]
pub struct ParallaxTransition {
    pub from_biome: String,
    pub to_biome: String,
    pub progress: f32,
    pub duration: f32,
}

const TRANSITION_DURATION: f32 = 1.5;
```

**Step 2: Add biome_id to ParallaxLayer component**

```rust
pub struct ParallaxLayer {
    pub biome_id: String,    // NEW — which biome this layer belongs to
    pub speed_x: f32,
    pub speed_y: f32,
    pub repeat_x: bool,
    pub repeat_y: bool,
    pub texture_size: Vec2,
    pub initialized: bool,
}
```

**Step 3: Write `track_player_biome` system**

Reads player position → looks up biome → if changed, starts transition by:
1. Spawning new parallax layers for `to_biome` (alpha = 0)
2. Inserting `ParallaxTransition` resource
3. Updating `CurrentBiome`

**Step 4: Write `parallax_transition_system`**

Each frame:
1. Advance `progress` by `dt / duration`
2. Set alpha on `from` layers = `1.0 - progress`
3. Set alpha on `to` layers = `progress`
4. When `progress >= 1.0`: despawn `from` layers, remove `ParallaxTransition`

Alpha is set via `Sprite.color.set_alpha()`.

**Step 5: Update `spawn_parallax_layers` to accept biome_id**

```rust
pub fn spawn_biome_parallax(
    commands: &mut Commands,
    asset_server: &AssetServer,
    config: &ParallaxConfig,
    biome_id: &str,
    initial_alpha: f32,
)
```

**Step 6: Update `parallax_scroll` to handle two sets**

No change needed — it already iterates all `ParallaxLayer` entities. Both `from` and `to` layers will be scrolled correctly.

**Step 7: Update ParallaxPlugin system ordering**

```rust
(track_player_biome, parallax_transition_system, spawn_parallax_layers, parallax_scroll)
    .chain()
    .after(camera_follow_player)
    .run_if(in_state(AppState::InGame))
```

**Step 8: Update OnEnter(InGame) to spawn initial biome parallax**

Instead of spawning from global config, spawn from player's starting biome's parallax config.

**Step 9: Remove global parallax loading**

- Remove `parallax: Handle<ParallaxConfigAsset>` from `LoadingAssets` and `RegistryHandles`
- Remove `hot_reload_parallax` (replaced by biome-aware hot-reload)
- Delete `assets/world/parallax.ron`
- Remove global `ParallaxConfig` resource insertion

**Step 10: Run game, verify parallax transitions**

Run: `cargo run`
Expected: game starts with meadow parallax, walking to different biome triggers crossfade

**Step 11: Commit**

```bash
git add src/parallax/ src/registry/ assets/
git commit -m "feat(biome): per-biome parallax with crossfade transitions"
```

---

### Task 9: Cleanup, Hot-Reload & Final Verification

**Files:**
- Modify: `src/registry/mod.rs` (biome hot-reload)
- Modify: various (cleanup dead code)

**Step 1: Add biome hot-reload systems**

```rust
fn hot_reload_biomes(/* ... */) {
    // On BiomeAsset modified: update BiomeRegistry entry, mark all chunks dirty
}

fn hot_reload_planet_type(/* ... */) {
    // On PlanetTypeAsset modified: rebuild PlanetConfig, BiomeMap, mark all chunks dirty
}
```

**Step 2: Remove dead code**

- Remove unused `#[allow(dead_code)]` if TerrainTiles is fully gone
- Remove `assets/world/parallax.ron` if not already done
- Check for any remaining references to `TerrainTiles`

**Step 3: Run full test suite**

Run: `cargo test`
Expected: all tests PASS

**Step 4: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: no warnings

**Step 5: Run the game**

Run: `cargo run`
Expected:
- Game loads with biome log messages
- Different terrain visible when walking far enough
- Parallax crossfade works on biome boundaries
- Block break/place still works

**Step 6: Commit**

```bash
git add -A
git commit -m "feat(biome): hot-reload, cleanup, and final verification"
```
