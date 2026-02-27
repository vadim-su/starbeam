use std::collections::HashMap;

use bevy::prelude::*;

use crate::registry::tile::TileId;

/// Type-safe biome identifier backed by a `u16`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BiomeId(pub u16);

impl std::fmt::Display for BiomeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BiomeId({})", self.0)
    }
}

/// Runtime definition of a biome, built from BiomeAsset + TileRegistry lookups.
#[derive(Debug, Clone)]
pub struct BiomeDef {
    #[allow(dead_code)] // used for debug display and hot-reload identification
    pub id: String,
    pub surface_block: TileId,
    pub subsurface_block: TileId,
    pub subsurface_depth: i32,
    pub fill_block: TileId,
    pub cave_threshold: f64,
    #[allow(dead_code)]
    // stored for hot-reload; parallax loaded separately via BiomeParallaxConfigs
    pub parallax_path: Option<String>,
}

/// All loaded biome definitions keyed by BiomeId.
#[derive(Resource, Debug, Default)]
pub struct BiomeRegistry {
    biomes: HashMap<BiomeId, BiomeDef>,
    name_to_id: HashMap<String, BiomeId>,
    id_to_name: HashMap<BiomeId, String>,
    next_id: u16,
}

impl BiomeRegistry {
    /// Insert or update a biome definition. Returns the BiomeId (existing or newly allocated).
    pub fn insert(&mut self, name: &str, def: BiomeDef) -> BiomeId {
        if let Some(&id) = self.name_to_id.get(name) {
            self.biomes.insert(id, def);
            id
        } else {
            let id = BiomeId(self.next_id);
            self.next_id += 1;
            self.name_to_id.insert(name.to_string(), id);
            self.id_to_name.insert(id, name.to_string());
            self.biomes.insert(id, def);
            id
        }
    }

    pub fn get(&self, id: BiomeId) -> &BiomeDef {
        self.biomes
            .get(&id)
            .unwrap_or_else(|| panic!("Unknown biome: {id}"))
    }

    #[allow(dead_code)] // public API for future use; tested
    pub fn get_opt(&self, id: BiomeId) -> Option<&BiomeDef> {
        self.biomes.get(&id)
    }

    pub fn id_by_name(&self, name: &str) -> BiomeId {
        *self
            .name_to_id
            .get(name)
            .unwrap_or_else(|| panic!("Unknown biome name: {name}"))
    }

    pub fn name_of(&self, id: BiomeId) -> &str {
        self.id_to_name
            .get(&id)
            .map(|s| s.as_str())
            .unwrap_or_else(|| panic!("Unknown biome id: {id}"))
    }
}

/// Computed Y boundaries for each layer (tile coordinates, from bottom).
#[derive(Debug, Clone)]
pub struct LayerBoundaries {
    /// First tile above Core (Core occupies 0..core_top).
    pub core_top: i32,
    /// First tile above DeepUnderground.
    pub deep_underground_top: i32,
    /// First tile above Underground.
    pub underground_top: i32,
}

impl LayerBoundaries {
    /// Compute boundaries from layer depth ratios and world height.
    pub fn from_layers(layers: &LayerConfigs, world_height: i32) -> Self {
        let h = world_height as f64;
        let core_top = (layers.core.depth_ratio * h) as i32;
        let deep_underground_top = core_top + (layers.deep_underground.depth_ratio * h) as i32;
        let underground_top = deep_underground_top + (layers.underground.depth_ratio * h) as i32;
        Self {
            core_top,
            deep_underground_top,
            underground_top,
        }
    }
}

/// Runtime planet type data, built from PlanetTypeAsset.
#[derive(Resource, Debug, Clone)]
pub struct PlanetConfig {
    #[allow(dead_code)] // used for debug display and logging
    pub id: String,
    pub primary_biome: String,
    pub secondary_biomes: Vec<String>,
    pub layers: LayerConfigs,
    /// Computed Y boundaries for each layer.
    pub layer_boundaries: LayerBoundaries,
    pub region_width_min: u32,
    pub region_width_max: u32,
    pub primary_region_ratio: f64,
}

#[derive(Debug, Clone)]
pub struct LayerConfig {
    pub primary_biome: Option<String>,
    pub terrain_frequency: f64,
    pub terrain_amplitude: f64,
    /// Fraction of world height this layer occupies (0.0–1.0).
    pub depth_ratio: f64,
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
    /// Determine which vertical layer a tile_y belongs to, using data-driven boundaries.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_layer_boundaries() {
        use crate::test_helpers::fixtures;
        let pc = fixtures::test_planet_config();
        // With depth_ratios 0.12, 0.33, 0.25, 0.30 and height 1024:
        // core_top = (0.12 * 1024) = 122
        // deep_top = 122 + (0.33 * 1024) = 122 + 337 = 459
        // underground_top = 459 + (0.25 * 1024) = 459 + 256 = 715
        assert_eq!(WorldLayer::from_tile_y(0, &pc), WorldLayer::Core);
        assert_eq!(WorldLayer::from_tile_y(100, &pc), WorldLayer::Core);
        assert_eq!(
            WorldLayer::from_tile_y(130, &pc),
            WorldLayer::DeepUnderground
        );
        assert_eq!(WorldLayer::from_tile_y(460, &pc), WorldLayer::Underground);
        assert_eq!(WorldLayer::from_tile_y(720, &pc), WorldLayer::Surface);
        assert_eq!(WorldLayer::from_tile_y(1023, &pc), WorldLayer::Surface);
    }

    #[test]
    fn biome_registry_insert_and_get() {
        let mut reg = BiomeRegistry::default();
        let id = reg.insert(
            "meadow",
            BiomeDef {
                id: "meadow".into(),
                surface_block: TileId(1),
                subsurface_block: TileId(2),
                subsurface_depth: 4,
                fill_block: TileId(3),
                cave_threshold: 0.3,
                parallax_path: Some("biomes/meadow/parallax.ron".into()),
            },
        );
        let def = reg.get(id);
        assert_eq!(def.id, "meadow");
        assert_eq!(def.surface_block, TileId(1));
        assert_eq!(reg.id_by_name("meadow"), id);
        assert_eq!(reg.name_of(id), "meadow");
    }

    #[test]
    fn biome_registry_insert_updates_existing() {
        let mut reg = BiomeRegistry::default();
        let id1 = reg.insert(
            "meadow",
            BiomeDef {
                id: "meadow".into(),
                surface_block: TileId(1),
                subsurface_block: TileId(2),
                subsurface_depth: 4,
                fill_block: TileId(3),
                cave_threshold: 0.3,
                parallax_path: None,
            },
        );
        let id2 = reg.insert(
            "meadow",
            BiomeDef {
                id: "meadow".into(),
                surface_block: TileId(10),
                subsurface_block: TileId(2),
                subsurface_depth: 4,
                fill_block: TileId(3),
                cave_threshold: 0.3,
                parallax_path: None,
            },
        );
        assert_eq!(id1, id2, "re-insert must return same BiomeId");
        assert_eq!(reg.get(id1).surface_block, TileId(10));
    }

    #[test]
    fn biome_registry_get_opt_none() {
        let reg = BiomeRegistry::default();
        assert!(reg.get_opt(BiomeId(999)).is_none());
    }

    #[test]
    #[should_panic(expected = "Unknown biome: BiomeId(999)")]
    fn biome_registry_get_panics() {
        let reg = BiomeRegistry::default();
        reg.get(BiomeId(999));
    }

    #[test]
    #[should_panic(expected = "Unknown biome name: missing")]
    fn biome_registry_id_by_name_panics() {
        let reg = BiomeRegistry::default();
        reg.id_by_name("missing");
    }
}
