//! Deterministic biome region generation for horizontal world layout.
//!
//! Distributes biomes as contiguous horizontal regions across the world width,
//! ensuring no two adjacent regions share the same biome (including cylindrical wrap).

use bevy::prelude::Resource;

use crate::registry::biome::{BiomeId, BiomeRegistry};

// ---------------------------------------------------------------------------
// Minimal splitmix64 RNG — no external crate needed
// ---------------------------------------------------------------------------

/// SplitMix64 RNG — deterministic, fast, non-cryptographic.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
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

    /// Returns a value in `[lo, hi]` (inclusive).
    fn range(&mut self, lo: u32, hi: u32) -> u32 {
        assert!(hi >= lo);
        let span = (hi - lo) as u64 + 1;
        (self.next() % span) as u32 + lo
    }
}

// ---------------------------------------------------------------------------
// Public data structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BiomeRegion {
    pub biome_id: BiomeId,
    pub start_x: u32,
    #[allow(dead_code)] // used in tests; implicitly encoded via start_x intervals at runtime
    pub width: u32,
}

#[derive(Debug, Clone, Resource)]
pub struct BiomeMap {
    pub regions: Vec<BiomeRegion>,
    pub world_width: u32,
}

impl BiomeMap {
    /// Generate a deterministic biome map from the given parameters.
    ///
    /// * `primary`         – biome that should dominate (~`primary_ratio` of slots)
    /// * `secondaries`     – other biomes to fill the remaining slots
    /// * `seed`            – deterministic RNG seed
    /// * `world_width`     – total world width in tiles
    /// * `region_min`      – minimum region width in tiles
    /// * `region_max`      – maximum region width in tiles
    /// * `primary_ratio`   – target fraction of regions assigned to the primary biome
    /// * `biome_registry`  – used to resolve biome names to BiomeId
    #[allow(clippy::too_many_arguments)]
    pub fn generate(
        primary: &str,
        secondaries: &[&str],
        seed: u64,
        world_width: u32,
        region_min: u32,
        region_max: u32,
        primary_ratio: f64,
        biome_registry: &BiomeRegistry,
    ) -> Self {
        assert!(region_min > 0, "region_min must be > 0");
        assert!(region_max >= region_min, "region_max must be >= region_min");
        assert!(!secondaries.is_empty(), "need at least one secondary biome");

        let mut rng = SplitMix64::new(seed);

        // Full palette of available biomes (for fallback replacements)
        let mut all_biomes: Vec<String> = Vec::with_capacity(1 + secondaries.len());
        all_biomes.push(primary.to_string());
        for s in secondaries {
            all_biomes.push(s.to_string());
        }

        // --- Compute region count ---
        let avg_width = (region_min + region_max) / 2;
        let region_count = (world_width / avg_width).max(2) as usize;

        // --- Allocate biome ids to slots ---
        let primary_slots = ((region_count as f64 * primary_ratio).round() as usize).max(1);
        let secondary_slots = region_count - primary_slots;

        let mut biome_names: Vec<String> = Vec::with_capacity(region_count);
        for _ in 0..primary_slots {
            biome_names.push(primary.to_string());
        }
        for i in 0..secondary_slots {
            let idx = i % secondaries.len();
            biome_names.push(secondaries[idx].to_string());
        }

        // --- Fisher-Yates shuffle ---
        for i in (1..biome_names.len()).rev() {
            let j = rng.range(0, i as u32) as usize;
            biome_names.swap(i, j);
        }

        // --- Fix adjacent duplicates ---
        fix_adjacent_duplicates(&mut biome_names, &all_biomes, &mut rng);

        // --- Fix wrap-around (first != last for cylindrical world) ---
        fix_wrap(&mut biome_names, &all_biomes, &mut rng);

        // --- Assign widths ---
        let mut widths: Vec<u32> = (0..region_count)
            .map(|_| rng.range(region_min, region_max))
            .collect();

        // Adjust last region so total == world_width
        let current_sum: u32 = widths.iter().sum();
        if current_sum <= world_width {
            *widths.last_mut().unwrap() += world_width - current_sum;
        } else {
            // Shrink from the end until we fit
            let mut excess = current_sum - world_width;
            for w in widths.iter_mut().rev() {
                if excess == 0 {
                    break;
                }
                let can_shrink = w.saturating_sub(1); // keep at least 1
                let shrink = can_shrink.min(excess);
                *w -= shrink;
                excess -= shrink;
            }
            debug_assert!(excess == 0, "could not shrink regions to fit world_width");
        }

        // --- Build regions with contiguous start_x, resolving names to BiomeId ---
        let mut regions = Vec::with_capacity(region_count);
        let mut start_x = 0u32;
        for (biome_name, width) in biome_names.into_iter().zip(widths) {
            regions.push(BiomeRegion {
                biome_id: biome_registry.id_by_name(&biome_name),
                start_x,
                width,
            });
            start_x += width;
        }

        Self {
            regions,
            world_width,
        }
    }

    /// Return the BiomeId at the given tile x-coordinate. O(log n) via binary search.
    pub fn biome_at(&self, tile_x: u32) -> BiomeId {
        let idx = self.region_index_at(tile_x);
        self.regions[idx].biome_id
    }

    /// Return the region index at the given tile x-coordinate. O(log n) via binary search.
    pub fn region_index_at(&self, tile_x: u32) -> usize {
        let wrapped = tile_x % self.world_width;
        // Binary search: find the last region whose start_x <= wrapped
        self.regions
            .partition_point(|r| r.start_x <= wrapped)
            .saturating_sub(1)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Ensure no two adjacent slots share the same biome.
///
/// First attempts swaps; if no valid swap exists, directly replaces `ids[i]`
/// with a biome from `all_biomes` that differs from both neighbors.
fn fix_adjacent_duplicates(ids: &mut [String], all_biomes: &[String], rng: &mut SplitMix64) {
    let len = ids.len();
    if len < 2 {
        return;
    }

    for i in 1..len {
        if ids[i] == ids[i - 1] {
            // Try to find a candidate to swap with
            let mut swapped = false;
            for offset in 1..len {
                let j = (i + offset) % len;
                if j == 0 {
                    continue; // skip index 0 to avoid breaking earlier fixes
                }
                let next_of_j = (j + 1) % len;
                let prev_of_j = if j > 0 { j - 1 } else { len - 1 };

                // Only swap if it won't create a new adjacent duplicate
                if ids[j] != ids[i - 1]
                    && (i + 1 >= len || ids[j] != ids[i + 1])
                    && ids[i] != ids[prev_of_j]
                    && (next_of_j == i || ids[i] != ids[next_of_j])
                {
                    ids.swap(i, j);
                    swapped = true;
                    break;
                }
            }

            // Fallback: replace ids[i] with a biome that differs from neighbors
            if !swapped {
                let prev = &ids[i - 1];
                let next = if i + 1 < len { &ids[i + 1] } else { prev };
                let candidates: Vec<&String> = all_biomes
                    .iter()
                    .filter(|b| *b != prev && *b != next)
                    .collect();
                if let Some(&replacement) = candidates.get(rng.next() as usize % candidates.len()) {
                    ids[i] = replacement.clone();
                }
            }
        }
    }
}

/// Ensure first and last regions differ (cylindrical wrap constraint).
///
/// First attempts swaps; if no valid swap exists, directly replaces the last
/// region's biome with one that differs from both the first and second-to-last.
fn fix_wrap(ids: &mut [String], all_biomes: &[String], rng: &mut SplitMix64) {
    let len = ids.len();
    if len < 3 {
        return;
    }

    if ids[0] == ids[len - 1] {
        // Try to find a region in the middle to swap with
        for j in 1..len - 1 {
            // After swap: ids[len-1] becomes ids[j], ids[j] becomes old ids[len-1]
            if ids[j] != ids[len - 2]
                && ids[j] != ids[0]
                && ids[len - 1] != ids[j.saturating_sub(1)]
                && (j + 1 >= len - 1 || ids[len - 1] != ids[j + 1])
            {
                ids.swap(j, len - 1);
                return;
            }
        }

        // Fallback: replace last region with a biome differing from first and second-to-last
        let first = &ids[0];
        let second_to_last = &ids[len - 2];
        let candidates: Vec<&String> = all_biomes
            .iter()
            .filter(|b| *b != first && *b != second_to_last)
            .collect();
        if let Some(&replacement) = candidates.get(rng.next() as usize % candidates.len()) {
            ids[len - 1] = replacement.clone();
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::biome::BiomeDef;
    use crate::registry::tile::TileId;

    const TEST_SEED: u64 = 42;
    const WORLD_WIDTH: u32 = 2048;
    const REGION_MIN: u32 = 300;
    const REGION_MAX: u32 = 600;
    const PRIMARY_RATIO: f64 = 0.6;

    fn test_registry() -> BiomeRegistry {
        let mut reg = BiomeRegistry::default();
        for name in ["meadow", "forest", "rocky"] {
            reg.insert(
                name,
                BiomeDef {
                    id: name.into(),
                    surface_block: TileId(1),
                    subsurface_block: TileId(2),
                    subsurface_depth: 4,
                    fill_block: TileId(3),
                    cave_threshold: 0.3,
                    parallax_path: None,
                },
            );
        }
        reg
    }

    fn test_map() -> (BiomeMap, BiomeRegistry) {
        let reg = test_registry();
        let map = BiomeMap::generate(
            "meadow",
            &["forest", "rocky"],
            TEST_SEED,
            WORLD_WIDTH,
            REGION_MIN,
            REGION_MAX,
            PRIMARY_RATIO,
            &reg,
        );
        (map, reg)
    }

    #[test]
    fn generate_produces_regions() {
        let (map, _) = test_map();
        assert!(!map.regions.is_empty(), "regions must not be empty");
    }

    #[test]
    fn regions_cover_entire_width() {
        let (map, _) = test_map();
        let total: u32 = map.regions.iter().map(|r| r.width).sum();
        assert_eq!(total, WORLD_WIDTH, "region widths must sum to world_width");
    }

    #[test]
    fn regions_start_x_is_contiguous() {
        let (map, _) = test_map();
        let mut expected_start = 0u32;
        for r in &map.regions {
            assert_eq!(
                r.start_x, expected_start,
                "region start_x must be contiguous"
            );
            expected_start += r.width;
        }
    }

    #[test]
    fn no_adjacent_same_biome() {
        let (map, _) = test_map();
        for pair in map.regions.windows(2) {
            assert_ne!(
                pair[0].biome_id, pair[1].biome_id,
                "adjacent regions must differ: {} at x={} vs {} at x={}",
                pair[0].biome_id, pair[0].start_x, pair[1].biome_id, pair[1].start_x,
            );
        }
    }

    #[test]
    fn first_last_region_differ_for_wrap() {
        let (map, _) = test_map();
        let first = map.regions.first().unwrap().biome_id;
        let last = map.regions.last().unwrap().biome_id;
        assert_ne!(
            first, last,
            "first and last regions must differ for cylindrical wrap"
        );
    }

    #[test]
    fn primary_biome_ratio_approximately_correct() {
        let (map, reg) = test_map();
        let meadow_id = reg.id_by_name("meadow");
        let primary_count = map
            .regions
            .iter()
            .filter(|r| r.biome_id == meadow_id)
            .count();
        let ratio = primary_count as f64 / map.regions.len() as f64;
        assert!(
            (ratio - PRIMARY_RATIO).abs() < 0.20,
            "primary ratio {ratio:.2} should be ~{PRIMARY_RATIO} ±0.20"
        );
    }

    #[test]
    fn biome_at_returns_correct_biome() {
        let (map, _) = test_map();
        // First region: biome_at(0) should match regions[0]
        assert_eq!(
            map.biome_at(0),
            map.regions[0].biome_id,
            "biome_at(0) must match first region"
        );
        // Second region: biome_at(start_x of region 1) should match regions[1]
        let second_start = map.regions[1].start_x;
        assert_eq!(
            map.biome_at(second_start),
            map.regions[1].biome_id,
            "biome_at at second region start must match"
        );
    }

    #[test]
    fn biome_at_wraps_around() {
        let (map, _) = test_map();
        assert_eq!(
            map.biome_at(0),
            map.biome_at(WORLD_WIDTH),
            "biome_at(0) must equal biome_at(world_width) due to wrap"
        );
    }

    #[test]
    fn deterministic_generation() {
        let (map1, _) = test_map();
        let (map2, _) = test_map();
        assert_eq!(map1.regions.len(), map2.regions.len());
        for (a, b) in map1.regions.iter().zip(map2.regions.iter()) {
            assert_eq!(a.biome_id, b.biome_id);
            assert_eq!(a.start_x, b.start_x);
            assert_eq!(a.width, b.width);
        }
    }

    #[test]
    fn different_seed_different_result() {
        let (map1, _) = test_map();
        let reg = test_registry();
        let map2 = BiomeMap::generate(
            "meadow",
            &["forest", "rocky"],
            999,
            WORLD_WIDTH,
            REGION_MIN,
            REGION_MAX,
            PRIMARY_RATIO,
            &reg,
        );
        // At least one region should differ in biome_id or start_x
        let differs = map1
            .regions
            .iter()
            .zip(map2.regions.iter())
            .any(|(a, b)| a.biome_id != b.biome_id || a.start_x != b.start_x);
        assert!(differs, "different seeds must produce different maps");
    }

    #[test]
    fn region_index_at_returns_index() {
        let (map, _) = test_map();
        // Index 0 at tile 0
        assert_eq!(map.region_index_at(0), 0);
        // Index 1 at start of second region
        assert_eq!(map.region_index_at(map.regions[1].start_x), 1);
        // Last region
        let last_idx = map.regions.len() - 1;
        let last_start = map.regions[last_idx].start_x;
        assert_eq!(map.region_index_at(last_start), last_idx);
    }
}
