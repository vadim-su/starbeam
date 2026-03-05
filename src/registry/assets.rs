use std::collections::HashMap;

use bevy::prelude::*;
use bevy::reflect::TypePath;
use serde::Deserialize;

use super::tile::TileDef;
use crate::item::definition::ItemDef;
use crate::object::definition::ObjectDef;
use crate::parallax::config::ParallaxLayerDef;

/// Asset loaded from *.registry.ron (monolithic tile list)
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct TileRegistryAsset {
    pub tiles: Vec<TileDef>,
}

/// Asset loaded from a single *.object.ron file (per-entity).
/// Thin wrapper around `ObjectDef` that resolves the sprite path
/// relative to the entity folder.
#[derive(Asset, TypePath, Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct ObjectDefAsset(pub ObjectDef);

impl ObjectDefAsset {
    /// Convert to an `ObjectDef`, resolving the sprite path relative to the
    /// object.ron file's directory.
    pub fn to_object_def(&self, base_path: &str) -> ObjectDef {
        let mut def = self.0.clone();
        if !def.sprite.is_empty() {
            def.sprite = format!("{}{}", base_path, def.sprite);
        }
        def
    }
}

/// Asset loaded from *.character.ron — replaces PlayerDefAsset.
/// Contains physics parameters, sprite info, and animation definitions.
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct CharacterDefAsset {
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
    pub sprite_size: (u32, u32),
    pub animations: HashMap<String, AnimationDef>,
}

/// A single animation within a CharacterDefAsset.
#[derive(Debug, Clone, Deserialize)]
pub struct AnimationDef {
    pub frames: Vec<String>,
    pub fps: f32,
}

/// Asset loaded from item.ron — a single item definition.
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct ItemDefAsset {
    pub id: String,
    pub display_name: String,
    pub description: String,
    #[serde(default = "default_max_stack")]
    pub max_stack: u16,
    #[serde(default)]
    pub rarity: crate::item::definition::Rarity,
    #[serde(default)]
    pub item_type: crate::item::definition::ItemType,
    pub icon: String,
    #[serde(default)]
    pub placeable: Option<String>,
    #[serde(default)]
    pub placeable_object: Option<String>,
    #[serde(default)]
    pub equipment_slot: Option<crate::item::definition::EquipmentSlot>,
    #[serde(default)]
    pub stats: Option<crate::item::definition::ItemStats>,
}

impl ItemDefAsset {
    /// Convert to an `ItemDef`, resolving the icon path relative to the
    /// item.ron file's directory.
    pub fn to_item_def(&self, base_path: &str) -> ItemDef {
        ItemDef {
            id: self.id.clone(),
            display_name: self.display_name.clone(),
            description: self.description.clone(),
            max_stack: self.max_stack,
            rarity: self.rarity,
            item_type: self.item_type,
            icon: format!("{}{}", base_path, self.icon),
            placeable: self.placeable.clone(),
            placeable_object: self.placeable_object.clone(),
            equipment_slot: self.equipment_slot,
            stats: self.stats.clone(),
        }
    }
}

fn default_max_stack() -> u16 {
    99
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

/// Asset loaded from *.parallax.ron
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

    // --- Day/night range fields (Optional — None = derive procedurally) ---
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
    fn ron_roundtrip_object_def() {
        let ron_str = std::fs::read_to_string("assets/content/objects/torch/torch.object.ron")
            .expect("torch.object.ron should exist");
        let asset: ObjectDefAsset = ron::from_str(&ron_str).expect("torch.object.ron should parse");
        assert_eq!(asset.0.id, "torch_object");
        assert_eq!(asset.0.light_emission, [255, 170, 40]);
        assert_eq!(asset.0.sprite_columns, 4);
        assert_eq!(asset.0.sprite_rows, 4);
        // Verify sprite path resolution
        let def = asset.to_object_def("content/objects/torch/");
        assert_eq!(def.sprite, "content/objects/torch/torch.png");
    }

    #[test]
    fn ron_roundtrip_character() {
        let ron_str = std::fs::read_to_string(
            "assets/content/characters/adventurer/adventurer.character.ron",
        )
        .expect("adventurer.character.ron should exist");
        let asset: CharacterDefAsset =
            ron::from_str(&ron_str).expect("adventurer.character.ron should parse");
        assert!(asset.speed > 0.0);
        assert!(asset.animations.contains_key("staying"));
        assert!(asset.animations.contains_key("running"));
        assert!(asset.animations.contains_key("jumping"));
        assert_eq!(asset.sprite_size, (44, 44));
    }

    #[test]
    fn ron_roundtrip_item() {
        let ron_str = std::fs::read_to_string("assets/content/tiles/dirt/dirt.item.ron")
            .expect("dirt.item.ron should exist");
        let asset: ItemDefAsset = ron::from_str(&ron_str).expect("dirt.item.ron should parse");
        assert_eq!(asset.id, "dirt");
        assert_eq!(asset.max_stack, 999);
        assert!(asset.placeable.is_some());
    }

}
