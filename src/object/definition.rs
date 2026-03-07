use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::item::definition::{DropDef, ItemDef, ItemType, Rarity};

/// Compact object identifier. Index into ObjectRegistry.defs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ObjectId(pub u16);

impl ObjectId {
    pub const NONE: ObjectId = ObjectId(0);
}

#[derive(Debug, Clone, Deserialize)]
pub enum PlacementRule {
    Floor,
    Wall,
    Ceiling,
    FloorOrWall,
    Any,
}

#[derive(Debug, Clone, Deserialize)]
pub enum ObjectType {
    Decoration,
    Container { slots: u16 },
    LightSource,
    CraftingStation { station_id: String },
}

fn default_solid_mask() -> Vec<bool> {
    vec![true]
}

fn default_light_emission() -> [u8; 3] {
    [0, 0, 0]
}

/// Optional inline item definition. When present on an ObjectDef, an ItemDef
/// is auto-generated during registry loading — no separate `.item.ron` needed.
#[derive(Debug, Clone, Deserialize)]
pub struct AutoItemConfig {
    /// Override item ID. Defaults to object ID if not set.
    #[serde(default)]
    pub item_id: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_max_stack")]
    pub max_stack: u16,
    #[serde(default)]
    pub rarity: Rarity,
    #[serde(default)]
    pub item_type: ItemType,
    /// Explicit icon path (relative to object folder). If None, UI falls back
    /// to the object sprite (Starbound-style).
    #[serde(default)]
    pub icon: Option<String>,
}

fn default_max_stack() -> u16 {
    99
}

fn default_one() -> u32 {
    1
}

fn default_zero_f32() -> f32 {
    0.0
}

fn default_one_f32() -> f32 {
    1.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct ObjectDef {
    pub id: String,
    pub display_name: String,
    pub size: (u32, u32), // (width, height) in tiles
    pub sprite: String,
    #[serde(default = "default_solid_mask")]
    pub solid_mask: Vec<bool>, // len = size.0 * size.1, row-major bottom-up
    pub placement: PlacementRule,
    #[serde(default = "default_light_emission")]
    pub light_emission: [u8; 3],
    pub object_type: ObjectType,
    #[serde(default)]
    pub drops: Vec<DropDef>,
    // Animation
    #[serde(default = "default_one")]
    pub sprite_columns: u32,
    #[serde(default = "default_one")]
    pub sprite_rows: u32,
    #[serde(default = "default_zero_f32")]
    pub sprite_fps: f32,
    // Flicker (for light sources)
    #[serde(default = "default_zero_f32")]
    pub flicker_speed: f32,
    #[serde(default = "default_zero_f32")]
    pub flicker_strength: f32,
    #[serde(default = "default_one_f32")]
    pub flicker_min: f32,
    /// If set, an ItemDef is auto-generated from this config during loading.
    /// The generated item will have `placeable_object` pointing to this object,
    /// and the object's `drops` will auto-include this item (if drops is empty).
    #[serde(default)]
    pub auto_item: Option<AutoItemConfig>,
}

impl ObjectDef {
    /// Generate an `ItemDef` from the `auto_item` config.
    /// `base_path` is the asset directory (e.g. "content/objects/torch/")
    /// used to resolve relative icon paths.
    pub fn generate_item_def(&self, base_path: &str) -> Option<ItemDef> {
        let config = self.auto_item.as_ref()?;
        let item_id = config.item_id.clone().unwrap_or_else(|| self.id.clone());
        Some(ItemDef {
            id: item_id,
            display_name: self.display_name.clone(),
            description: config.description.clone(),
            max_stack: config.max_stack,
            rarity: config.rarity,
            item_type: config.item_type,
            icon: config.icon.as_ref().map(|i| format!("{}{}", base_path, i)),
            placeable: None,
            placeable_object: Some(self.id.clone()),
            equipment_slot: None,
            stats: None,
        })
    }

    /// Check if a specific local tile within this object is solid.
    /// `rel_x` and `rel_y` are relative to the anchor (bottom-left).
    pub fn is_tile_solid(&self, rel_x: u32, rel_y: u32) -> bool {
        let idx = (rel_y * self.size.0 + rel_x) as usize;
        self.solid_mask.get(idx).copied().unwrap_or(false)
    }

    /// Validate that solid_mask length matches size.
    /// Logs an error and pads/truncates the mask instead of panicking.
    /// Also auto-fills `drops` from `auto_item` when drops is empty.
    pub fn validate(&mut self) {
        let expected = (self.size.0 * self.size.1) as usize;
        if self.solid_mask.len() != expected {
            error!(
                "ObjectDef '{}': solid_mask len {} != size {}x{} = {}, auto-correcting",
                self.id,
                self.solid_mask.len(),
                self.size.0,
                self.size.1,
                expected
            );
            self.solid_mask.resize(expected, false);
        }
        // Auto-fill drops from auto_item when drops is empty
        if self.drops.is_empty() {
            if let Some(ref config) = self.auto_item {
                let item_id = config.item_id.clone().unwrap_or_else(|| self.id.clone());
                self.drops = vec![DropDef {
                    item_id,
                    min: 1,
                    max: 1,
                    chance: 1.0,
                }];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_id_none_is_zero() {
        assert_eq!(ObjectId::NONE, ObjectId(0));
    }

    fn test_object(
        id: &str,
        size: (u32, u32),
        solid_mask: Vec<bool>,
        placement: PlacementRule,
        object_type: ObjectType,
    ) -> ObjectDef {
        ObjectDef {
            id: id.into(),
            display_name: id.into(),
            size,
            sprite: format!("objects/{id}.png"),
            solid_mask,
            placement,
            light_emission: [0, 0, 0],
            object_type,
            drops: vec![],
            sprite_columns: 1,
            sprite_rows: 1,
            sprite_fps: 0.0,
            flicker_speed: 0.0,
            flicker_strength: 0.0,
            flicker_min: 1.0,
            auto_item: None,
        }
    }

    #[test]
    fn is_tile_solid_1x1() {
        let def = test_object(
            "barrel",
            (1, 1),
            vec![true],
            PlacementRule::Floor,
            ObjectType::Decoration,
        );
        assert!(def.is_tile_solid(0, 0));
    }

    #[test]
    fn is_tile_solid_multi_tile() {
        let def = test_object(
            "table",
            (3, 2),
            vec![true, false, true, false, false, false],
            PlacementRule::Floor,
            ObjectType::Decoration,
        );
        assert!(def.is_tile_solid(0, 0));
        assert!(!def.is_tile_solid(1, 0));
        assert!(def.is_tile_solid(2, 0));
        assert!(!def.is_tile_solid(0, 1));
        assert!(!def.is_tile_solid(1, 1));
        assert!(!def.is_tile_solid(2, 1));
    }

    #[test]
    fn is_tile_solid_out_of_bounds_returns_false() {
        let mut def = test_object(
            "torch",
            (1, 1),
            vec![false],
            PlacementRule::Wall,
            ObjectType::LightSource,
        );
        def.light_emission = [240, 180, 80];
        assert!(!def.is_tile_solid(5, 5));
    }

    #[test]
    fn generate_item_def_from_auto_item() {
        let mut def = test_object(
            "wooden_table",
            (3, 2),
            vec![true; 6],
            PlacementRule::Floor,
            ObjectType::Decoration,
        );
        def.auto_item = Some(AutoItemConfig {
            item_id: None,
            description: "A sturdy table".into(),
            max_stack: 10,
            rarity: Rarity::Common,
            item_type: ItemType::Block,
            icon: None,
        });
        let item = def
            .generate_item_def("content/objects/wooden_table/")
            .unwrap();
        assert_eq!(item.id, "wooden_table");
        assert_eq!(item.display_name, "wooden_table"); // uses object display_name
        assert_eq!(item.placeable_object, Some("wooden_table".into()));
        assert_eq!(item.max_stack, 10);
        assert!(item.icon.is_none());
    }

    #[test]
    fn generate_item_def_returns_none_without_auto_item() {
        let def = test_object(
            "barrel",
            (1, 1),
            vec![true],
            PlacementRule::Floor,
            ObjectType::Decoration,
        );
        assert!(def.generate_item_def("content/objects/barrel/").is_none());
    }

    #[test]
    fn validate_auto_fills_drops() {
        let mut def = test_object(
            "chest",
            (2, 1),
            vec![true, true],
            PlacementRule::Floor,
            ObjectType::Decoration,
        );
        def.auto_item = Some(AutoItemConfig {
            item_id: None,
            description: "A chest".into(),
            max_stack: 10,
            rarity: Rarity::Common,
            item_type: ItemType::Block,
            icon: None,
        });
        assert!(def.drops.is_empty());
        def.validate();
        assert_eq!(def.drops.len(), 1);
        assert_eq!(def.drops[0].item_id, "chest");
    }
}
