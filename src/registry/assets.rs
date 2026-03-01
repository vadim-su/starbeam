use std::collections::HashMap;

use bevy::prelude::*;
use bevy::reflect::TypePath;
use serde::Deserialize;

use super::tile::TileDef;
use crate::object::definition::ObjectDef;
use crate::parallax::config::ParallaxLayerDef;

/// Asset loaded from tiles.registry.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct TileRegistryAsset {
    pub tiles: Vec<TileDef>,
}

/// Asset loaded from objects.registry.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct ObjectRegistryAsset {
    pub objects: Vec<ObjectDef>,
}

/// Asset loaded from player.def.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct PlayerDefAsset {
    pub speed: f32,
    pub jump_velocity: f32,
    pub gravity: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default = "default_magnet_radius")]
    pub magnet_radius: f32,
    #[serde(default = "default_magnet_strength")]
    pub magnet_strength: f32,
    #[serde(default = "default_pickup_radius")]
    pub pickup_radius: f32,
}

fn default_magnet_radius() -> f32 {
    96.0
}
fn default_magnet_strength() -> f32 {
    400.0
}
fn default_pickup_radius() -> f32 {
    20.0
}

/// Asset loaded from world.config.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct WorldConfigAsset {
    pub width_tiles: i32,
    pub height_tiles: i32,
    pub chunk_size: u32,
    pub tile_size: f32,
    pub chunk_load_radius: i32,
    pub seed: u32,
    #[serde(default)]
    pub planet_type: String,
}

/// Asset loaded from bg.parallax.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct ParallaxConfigAsset {
    pub layers: Vec<ParallaxLayerDef>,
}

/// A single sprite variant within a bitmask mapping.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // col, index: present for RON compatibility with autotile47.py output
pub struct SpriteVariant {
    pub row: u32,
    pub weight: f32,
    #[serde(default)]
    pub col: u32,
    #[serde(default)]
    pub index: u32,
}

/// Mapping for a single bitmask value: description + weighted variants.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // description: present for RON readability, not used at runtime
pub struct BitmaskMapping {
    #[serde(default)]
    pub description: String,
    pub variants: Vec<SpriteVariant>,
}

/// Asset loaded from *.autotile.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
#[allow(dead_code)] // atlas_columns: reserved, not yet used at runtime
pub struct AutotileAsset {
    pub tile_size: u32,
    pub atlas_columns: u32,
    pub atlas_rows: u32,
    pub tiles: HashMap<u8, BitmaskMapping>,
}

/// Layer configuration within a planet type.
#[derive(Debug, Clone, Deserialize)]
pub struct LayerConfigAsset {
    pub primary_biome: Option<String>,
    pub terrain_frequency: f64,
    pub terrain_amplitude: f64,
    /// Fraction of world height this layer occupies (0.0–1.0).
    #[serde(default)]
    pub depth_ratio: f64,
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
    // Future fields — not implemented in MVP, kept for RON schema forward-compatibility
    #[allow(dead_code)]
    #[serde(default)]
    pub weather: Option<Vec<String>>,
    #[allow(dead_code)]
    #[serde(default)]
    pub music: Option<Vec<String>>,
    #[allow(dead_code)]
    #[serde(default)]
    pub ambient: Option<Vec<String>>,
    #[allow(dead_code)]
    #[serde(default)]
    pub placeables: Option<Vec<String>>,
    #[allow(dead_code)]
    #[serde(default)]
    pub monsters: Option<Vec<String>>,
    #[allow(dead_code)]
    #[serde(default)]
    pub status_effects: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ron_roundtrip_objects() {
        let ron_str = std::fs::read_to_string("assets/world/objects.objects.ron")
            .expect("objects.objects.ron should exist");
        let asset: ObjectRegistryAsset =
            ron::from_str(&ron_str).expect("objects.objects.ron should parse");
        assert!(
            asset.objects.len() >= 2,
            "should have at least none + torch"
        );
        assert_eq!(asset.objects[0].id, "none");
        assert_eq!(asset.objects[1].id, "torch_object");
        assert_eq!(asset.objects[1].light_emission, [255, 170, 40]);
        assert_eq!(asset.objects[1].sprite_columns, 4);
        assert_eq!(asset.objects[1].sprite_rows, 4);
    }
}
