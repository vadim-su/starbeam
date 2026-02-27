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

/// Runtime planet type data, built from PlanetTypeAsset.
#[derive(Resource, Debug, Clone)]
pub struct PlanetConfig {
    #[allow(dead_code)] // used for debug display and logging
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
        assert_eq!(
            WorldLayer::from_tile_y(130, 1024),
            WorldLayer::DeepUnderground
        );
        assert_eq!(WorldLayer::from_tile_y(380, 1024), WorldLayer::Underground);
        assert_eq!(WorldLayer::from_tile_y(720, 1024), WorldLayer::Surface);
        assert_eq!(WorldLayer::from_tile_y(1023, 1024), WorldLayer::Surface);
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
