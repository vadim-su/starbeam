use std::collections::HashMap;

use bevy::prelude::*;
use bevy::reflect::TypePath;
use serde::Deserialize;

use super::tile::TileDef;
use crate::crafting::Recipe;
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
    #[serde(default = "default_swim_impulse")]
    pub swim_impulse: f32,
    #[serde(default = "default_swim_gravity_factor")]
    pub swim_gravity_factor: f32,
    #[serde(default = "default_swim_drag")]
    pub swim_drag: f32,
    pub sprite_size: (u32, u32),
    #[serde(default = "default_render_scale")]
    pub render_scale: f32,
    pub animations: HashMap<String, AnimationDef>,
    #[serde(default)]
    pub parts: Option<CharacterPartsDef>,
}

/// A single animation within a CharacterDefAsset.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct AnimationDef {
    #[serde(default)]
    pub frames: Vec<String>,
    pub fps: f32,
}

/// Per-part sprite configuration within a character.
#[derive(Debug, Clone, Deserialize)]
pub struct PartDef {
    /// Relative path to the sprite directory (within the character folder).
    /// Contains animation subdirectories matching the character's animation names.
    pub sprite_dir: String,
    /// Pixel size of each frame for this part.
    pub frame_size: (u32, u32),
    /// Pixel offset from the parent entity's origin.
    #[serde(default)]
    pub offset: (f32, f32),
    /// Pivot point for rotation (shoulder attachment), in pixels relative to sprite center.
    /// Used by arms for aiming rotation.
    #[serde(default)]
    pub pivot: Option<(f32, f32)>,
    /// Default rotation angle in degrees when not aiming. Applied to arms at rest.
    #[serde(default)]
    pub default_angle: Option<f32>,
}

/// All body parts for a modular character.
#[derive(Debug, Clone, Deserialize)]
pub struct CharacterPartsDef {
    pub body: PartDef,
    #[serde(default)]
    pub head: Option<PartDef>,
    #[serde(default)]
    pub legs: Option<PartDef>,
    #[serde(default, alias = "front_arm")]
    pub hand_right: Option<PartDef>,
    #[serde(default, alias = "back_arm")]
    pub hand_left: Option<PartDef>,
}

impl CharacterPartsDef {
    /// Look up the config for a given part type.
    /// Body always returns `Some`; others may be absent.
    pub fn config_for(&self, part: crate::player::parts::PartType) -> Option<&PartDef> {
        use crate::player::parts::PartType;
        match part {
            PartType::Body => Some(&self.body),
            PartType::Head => self.head.as_ref(),
            PartType::Legs => self.legs.as_ref(),
            PartType::FrontArm => self.hand_right.as_ref(),
            PartType::BackArm => self.hand_left.as_ref(),
        }
    }
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
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub placeable: Option<String>,
    #[serde(default)]
    pub placeable_object: Option<String>,
    #[serde(default)]
    pub equipment_slot: Option<crate::item::definition::EquipmentSlot>,
    #[serde(default)]
    pub stats: Option<crate::item::definition::ItemStats>,
    #[serde(default)]
    pub blueprint_item: Option<String>,
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
            icon: self.icon.as_ref().map(|i| format!("{}{}", base_path, i)),
            placeable: self.placeable.clone(),
            placeable_object: self.placeable_object.clone(),
            equipment_slot: self.equipment_slot,
            stats: self.stats.clone(),
            blueprint_item: self.blueprint_item.clone(),
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
fn default_swim_impulse() -> f32 {
    180.0
}
fn default_swim_gravity_factor() -> f32 {
    0.3
}
fn default_swim_drag() -> f32 {
    0.15
}
fn default_render_scale() -> f32 {
    1.0
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

/// A single weather type entry with optional temperature constraints.
#[derive(Debug, Clone, Deserialize)]
pub struct WeatherTypeEntry {
    pub kind: String,
    #[serde(default = "default_neg_inf")]
    pub temp_min: f32,
    #[serde(default = "default_pos_inf")]
    pub temp_max: f32,
}

fn default_neg_inf() -> f32 {
    f32::NEG_INFINITY
}
fn default_pos_inf() -> f32 {
    f32::INFINITY
}

/// Weather configuration for a planet type.
#[derive(Debug, Clone, Deserialize)]
pub struct WeatherConfig {
    pub precipitation_chance: f32,
    pub precipitation_duration: (f32, f32),
    pub cooldown: (f32, f32),
    pub types: Vec<WeatherTypeEntry>,
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
    #[serde(default)]
    pub temperature_celsius_offsets: Option<[f32; 4]>,
    #[serde(default)]
    pub wrap_x: Option<bool>,
    #[serde(default)]
    pub base_temperature: Option<f32>,
    #[serde(default)]
    pub weather: Option<WeatherConfig>,
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
    #[serde(default)]
    pub snow_base_chance: f32,
    #[serde(default)]
    pub snow_permanent: bool,
    #[serde(default)]
    pub temperature_offset: f32,
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

/// Asset loaded from *.recipes.ron — a list of crafting recipes.
#[derive(Asset, TypePath, Debug, Deserialize)]
#[serde(transparent)]
pub struct RecipeListAsset(pub Vec<Recipe>);

/// Asset loaded from *.liquid.ron — liquid definitions.
#[derive(Asset, TypePath, Debug, Deserialize)]
#[serde(transparent)]
pub struct LiquidRegistryAsset(pub Vec<crate::liquid::registry::LiquidDef>);

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
        // Verify auto_item is parsed
        let auto = def.auto_item.as_ref().expect("torch should have auto_item");
        assert_eq!(auto.max_stack, 999);
        assert_eq!(auto.description, "A simple torch that emits warm light.");
        // Verify auto-generated item uses item_id override
        let item = def.generate_item_def("content/objects/torch/").unwrap();
        assert_eq!(item.id, "torch");
        assert_eq!(item.placeable_object, Some("torch_object".into()));
        assert_eq!(item.icon, Some("content/objects/torch/item.png".into()));
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
        assert_eq!(asset.sprite_size, (48, 48));
        assert!(asset.parts.is_some());
        let parts = asset.parts.as_ref().unwrap();
        assert_eq!(parts.body.frame_size, (48, 48));
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
