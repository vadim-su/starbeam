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
        assert_eq!(
            WorldLayer::from_tile_y(130, 1024),
            WorldLayer::DeepUnderground
        );
        assert_eq!(WorldLayer::from_tile_y(380, 1024), WorldLayer::Underground);
        assert_eq!(WorldLayer::from_tile_y(720, 1024), WorldLayer::Surface);
        assert_eq!(WorldLayer::from_tile_y(1023, 1024), WorldLayer::Surface);
    }

    #[test]
    fn biome_registry_get() {
        let mut reg = BiomeRegistry::default();
        reg.biomes.insert(
            "meadow".into(),
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
