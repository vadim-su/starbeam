use bevy::prelude::*;
use serde::Deserialize;

use crate::item::DropDef;

/// Compact object identifier. Index into ObjectRegistry.defs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
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
}

fn default_solid_mask() -> Vec<bool> {
    vec![true]
}

fn default_light_emission() -> [u8; 3] {
    [0, 0, 0]
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
}

impl ObjectDef {
    /// Check if a specific local tile within this object is solid.
    /// `rel_x` and `rel_y` are relative to the anchor (bottom-left).
    pub fn is_tile_solid(&self, rel_x: u32, rel_y: u32) -> bool {
        let idx = (rel_y * self.size.0 + rel_x) as usize;
        self.solid_mask.get(idx).copied().unwrap_or(false)
    }

    /// Validate that solid_mask length matches size. Panics on mismatch.
    pub fn validate(&self) {
        let expected = (self.size.0 * self.size.1) as usize;
        assert_eq!(
            self.solid_mask.len(),
            expected,
            "ObjectDef '{}': solid_mask len {} != size {}x{} = {}",
            self.id,
            self.solid_mask.len(),
            self.size.0,
            self.size.1,
            expected
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_id_none_is_zero() {
        assert_eq!(ObjectId::NONE, ObjectId(0));
    }

    #[test]
    fn is_tile_solid_1x1() {
        let def = ObjectDef {
            id: "barrel".into(),
            display_name: "Barrel".into(),
            size: (1, 1),
            sprite: "objects/barrel.png".into(),
            solid_mask: vec![true],
            placement: PlacementRule::Floor,
            light_emission: [0, 0, 0],
            object_type: ObjectType::Decoration,
            drops: vec![],
            sprite_columns: 1,
            sprite_rows: 1,
            sprite_fps: 0.0,
            flicker_speed: 0.0,
            flicker_strength: 0.0,
            flicker_min: 1.0,
        };
        assert!(def.is_tile_solid(0, 0));
    }

    #[test]
    fn is_tile_solid_multi_tile() {
        // Table 3x2: legs solid, top passable
        let def = ObjectDef {
            id: "table".into(),
            display_name: "Table".into(),
            size: (3, 2),
            sprite: "objects/table.png".into(),
            solid_mask: vec![true, false, true, false, false, false],
            placement: PlacementRule::Floor,
            light_emission: [0, 0, 0],
            object_type: ObjectType::Decoration,
            drops: vec![],
            sprite_columns: 1,
            sprite_rows: 1,
            sprite_fps: 0.0,
            flicker_speed: 0.0,
            flicker_strength: 0.0,
            flicker_min: 1.0,
        };
        assert!(def.is_tile_solid(0, 0));
        assert!(!def.is_tile_solid(1, 0));
        assert!(def.is_tile_solid(2, 0));
        assert!(!def.is_tile_solid(0, 1));
        assert!(!def.is_tile_solid(1, 1));
        assert!(!def.is_tile_solid(2, 1));
    }

    #[test]
    fn is_tile_solid_out_of_bounds_returns_false() {
        let def = ObjectDef {
            id: "torch".into(),
            display_name: "Torch".into(),
            size: (1, 1),
            sprite: "objects/torch.png".into(),
            solid_mask: vec![false],
            placement: PlacementRule::Wall,
            light_emission: [240, 180, 80],
            object_type: ObjectType::LightSource,
            drops: vec![],
            sprite_columns: 1,
            sprite_rows: 1,
            sprite_fps: 0.0,
            flicker_speed: 0.0,
            flicker_strength: 0.0,
            flicker_min: 1.0,
        };
        assert!(!def.is_tile_solid(5, 5));
    }
}
