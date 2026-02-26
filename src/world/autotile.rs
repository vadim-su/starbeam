use std::collections::HashMap;

use bevy::prelude::*;

use crate::registry::assets::{AutotileAsset, SpriteVariant};

/// Chunk dimensions in tiles.
pub const CHUNK_SIZE: u32 = 32;
/// Total tiles per chunk (CHUNK_SIZE * CHUNK_SIZE).
pub const CHUNK_TILE_COUNT: usize = (CHUNK_SIZE * CHUNK_SIZE) as usize;

// Neighbor bit layout for 8-bit bitmask (Blob47 scheme).
const BIT_N: u8 = 1;
const BIT_NE: u8 = 2;
const BIT_E: u8 = 4;
const BIT_SE: u8 = 8;
const BIT_S: u8 = 16;
const BIT_SW: u8 = 32;
const BIT_W: u8 = 64;
const BIT_NW: u8 = 128;

/// Runtime entry for one autotile type, built from an AutotileAsset.
/// Provides fast bitmask-to-variant lookup.
pub struct AutotileEntry {
    /// Position of this tile type's column in the combined atlas.
    pub column_index: u32,
    /// Length-256 lookup table indexed by bitmask value.
    /// Each entry holds the list of sprite variants for that bitmask.
    bitmask_map: Vec<Vec<SpriteVariant>>,
}

impl AutotileEntry {
    /// Build from a loaded asset and the assigned column index in the combined atlas.
    pub fn from_asset(asset: &AutotileAsset, column_index: u32) -> Self {
        let mut bitmask_map: Vec<Vec<SpriteVariant>> = (0..256).map(|_| Vec::new()).collect();
        for (&bitmask, mapping) in &asset.tiles {
            bitmask_map[bitmask as usize] = mapping.variants.clone();
        }
        Self {
            column_index,
            bitmask_map,
        }
    }

    /// Returns the variants for a given bitmask value.
    /// Falls back to bitmask 0 (isolated) if the requested bitmask has no entries.
    pub fn variants_for(&self, bitmask: u8) -> &[SpriteVariant] {
        let variants = &self.bitmask_map[bitmask as usize];
        if variants.is_empty() {
            &self.bitmask_map[0]
        } else {
            variants
        }
    }
}

/// Registry of all autotile entries, keyed by tile type name (e.g. "dirt", "stone").
#[derive(Resource, Default)]
pub struct AutotileRegistry {
    pub entries: HashMap<String, AutotileEntry>,
}

/// Compute the 8-bit bitmask for a tile at (x, y) based on its neighbors.
///
/// Corner bits (NE, SE, SW, NW) are only set when both adjacent cardinal
/// neighbors are also solid, matching the Blob47 autotile scheme.
pub fn compute_bitmask(mut is_solid_at: impl FnMut(i32, i32) -> bool, x: i32, y: i32) -> u8 {
    let n = is_solid_at(x, y + 1);
    let e = is_solid_at(x + 1, y);
    let s = is_solid_at(x, y - 1);
    let w = is_solid_at(x - 1, y);

    let mut mask = 0u8;
    if n {
        mask |= BIT_N;
    }
    if e {
        mask |= BIT_E;
    }
    if s {
        mask |= BIT_S;
    }
    if w {
        mask |= BIT_W;
    }

    // Corners only count when both adjacent cardinals are solid
    if n && e && is_solid_at(x + 1, y + 1) {
        mask |= BIT_NE;
    }
    if s && e && is_solid_at(x + 1, y - 1) {
        mask |= BIT_SE;
    }
    if s && w && is_solid_at(x - 1, y - 1) {
        mask |= BIT_SW;
    }
    if n && w && is_solid_at(x - 1, y + 1) {
        mask |= BIT_NW;
    }

    mask
}

/// Deterministic spatial hash for a tile position, returning a value in [0.0, 1.0].
/// Used for reproducible variant selection so the same tile always picks the same variant.
pub fn position_hash(x: i32, y: i32, seed: u32) -> f32 {
    // FNV-1a inspired hash for good distribution
    let mut h: u32 = 2166136261;
    h ^= x as u32;
    h = h.wrapping_mul(16777619);
    h ^= y as u32;
    h = h.wrapping_mul(16777619);
    h ^= seed;
    h = h.wrapping_mul(16777619);
    // Normalize to [0.0, 1.0]
    (h as f32) / (u32::MAX as f32)
}

/// Select a variant from a weighted list using a deterministic position hash.
/// Returns the `row` of the chosen variant in the atlas.
pub fn select_variant(variants: &[SpriteVariant], x: i32, y: i32, seed: u32) -> u32 {
    if variants.len() == 1 {
        return variants[0].row;
    }

    let total_weight: f32 = variants.iter().map(|v| v.weight).sum();
    let threshold = position_hash(x, y, seed) * total_weight;

    let mut cumulative = 0.0;
    for variant in variants {
        cumulative += variant.weight;
        if cumulative >= threshold {
            return variant.row;
        }
    }

    // Fallback to last variant (shouldn't happen with valid weights)
    variants.last().unwrap().row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmask_isolated() {
        let mask = compute_bitmask(|_, _| false, 0, 0);
        assert_eq!(mask, 0);
    }

    #[test]
    fn bitmask_surrounded() {
        let mask = compute_bitmask(|_, _| true, 0, 0);
        assert_eq!(mask, 255);
    }

    #[test]
    fn bitmask_north_only() {
        let mask = compute_bitmask(|x, y| x == 0 && y == 1, 0, 0);
        assert_eq!(mask, BIT_N);
        assert_eq!(mask, 1);
    }

    #[test]
    fn bitmask_cardinal_nsew() {
        // N + E + S + W = 1 + 4 + 16 + 64 = 85
        let mask = compute_bitmask(
            |x, y| {
                (x == 0 && y == 1)  // N
                || (x == 1 && y == 0)  // E
                || (x == 0 && y == -1) // S
                || (x == -1 && y == 0) // W
            },
            0,
            0,
        );
        assert_eq!(mask, 85);
    }

    #[test]
    fn bitmask_corner_ignored_without_cardinals() {
        // NE is solid but N and E are not — corner bit should NOT be set
        let mask = compute_bitmask(|x, y| x == 1 && y == 1, 0, 0);
        assert_eq!(mask, 0);
    }

    #[test]
    fn bitmask_corner_set_with_cardinals() {
        // N + E + NE all solid → N(1) + NE(2) + E(4) = 7
        let mask = compute_bitmask(
            |x, y| {
                (x == 0 && y == 1)  // N
                || (x == 1 && y == 0)  // E
                || (x == 1 && y == 1) // NE
            },
            0,
            0,
        );
        assert_eq!(mask, 7);
    }

    #[test]
    fn position_hash_deterministic() {
        let h1 = position_hash(10, 20, 42);
        let h2 = position_hash(10, 20, 42);
        assert_eq!(h1, h2);
    }

    #[test]
    fn position_hash_varies() {
        let h1 = position_hash(10, 20, 42);
        let h2 = position_hash(11, 20, 42);
        let h3 = position_hash(10, 21, 42);
        assert_ne!(h1, h2);
        assert_ne!(h1, h3);
    }

    #[test]
    fn position_hash_range() {
        // Test a spread of positions to verify range
        for x in -50..50 {
            for y in -50..50 {
                let h = position_hash(x, y, 42);
                assert!(h >= 0.0, "hash {h} below 0.0 at ({x}, {y})");
                assert!(h <= 1.0, "hash {h} above 1.0 at ({x}, {y})");
            }
        }
    }

    #[test]
    fn select_single_variant() {
        let variants = vec![SpriteVariant {
            row: 5,
            weight: 1.0,
            col: 0,
            index: 0,
        }];
        let row = select_variant(&variants, 10, 20, 42);
        assert_eq!(row, 5);
    }

    #[test]
    fn select_variant_deterministic() {
        let variants = vec![
            SpriteVariant {
                row: 0,
                weight: 1.0,
                col: 0,
                index: 0,
            },
            SpriteVariant {
                row: 1,
                weight: 1.0,
                col: 0,
                index: 0,
            },
            SpriteVariant {
                row: 2,
                weight: 1.0,
                col: 0,
                index: 0,
            },
        ];
        let r1 = select_variant(&variants, 10, 20, 42);
        let r2 = select_variant(&variants, 10, 20, 42);
        assert_eq!(r1, r2);
    }
}
