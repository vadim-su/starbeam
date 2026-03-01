use bevy::prelude::*;

use super::definition::{ObjectDef, ObjectType, PlacementRule};
use super::registry::ObjectRegistry;

pub struct ObjectPlugin;

impl Plugin for ObjectPlugin {
    fn build(&self, app: &mut App) {
        // Hardcoded registry for now (will move to RON loading later)
        app.insert_resource(ObjectRegistry::from_defs(vec![
            // Index 0: NONE placeholder (ObjectId::NONE)
            ObjectDef {
                id: "none".into(),
                display_name: "None".into(),
                size: (1, 1),
                sprite: "".into(),
                solid_mask: vec![false],
                placement: PlacementRule::Any,
                light_emission: [0, 0, 0],
                object_type: ObjectType::Decoration,
                drops: vec![],
            },
            ObjectDef {
                id: "torch_object".into(),
                display_name: "Torch".into(),
                size: (1, 1),
                sprite: "objects/torch.png".into(),
                solid_mask: vec![false],
                placement: PlacementRule::Wall,
                light_emission: [240, 180, 80],
                object_type: ObjectType::LightSource,
                drops: vec![],
            },
            ObjectDef {
                id: "wooden_chest".into(),
                display_name: "Wooden Chest".into(),
                size: (2, 1),
                sprite: "objects/wooden_chest.png".into(),
                solid_mask: vec![true, true],
                placement: PlacementRule::Floor,
                light_emission: [0, 0, 0],
                object_type: ObjectType::Container { slots: 16 },
                drops: vec![],
            },
            ObjectDef {
                id: "wooden_table".into(),
                display_name: "Wooden Table".into(),
                size: (3, 2),
                sprite: "objects/wooden_table.png".into(),
                solid_mask: vec![true, false, true, false, false, false],
                placement: PlacementRule::Floor,
                light_emission: [0, 0, 0],
                object_type: ObjectType::Decoration,
                drops: vec![],
            },
        ]));
    }
}
