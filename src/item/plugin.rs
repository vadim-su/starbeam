use bevy::prelude::*;

use super::definition::{ItemDef, ItemType, Rarity};
use super::dropped_item::{dropped_item_physics_system, PickupConfig};
use super::registry::ItemRegistry;

pub struct ItemPlugin;

impl Plugin for ItemPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ItemRegistry::from_defs(vec![
            ItemDef {
                id: "dirt".into(),
                display_name: "Dirt Block".into(),
                description: "A block of common dirt.".into(),
                max_stack: 999,
                rarity: Rarity::Common,
                item_type: ItemType::Block,
                icon: "items/dirt.png".into(),
                placeable: Some("dirt".into()),
                equipment_slot: None,
                stats: None,
            },
            ItemDef {
                id: "stone".into(),
                display_name: "Stone Block".into(),
                description: "A solid block of stone.".into(),
                max_stack: 999,
                rarity: Rarity::Common,
                item_type: ItemType::Block,
                icon: "items/stone.png".into(),
                placeable: Some("stone".into()),
                equipment_slot: None,
                stats: None,
            },
            ItemDef {
                id: "grass".into(),
                display_name: "Grass Block".into(),
                description: "A block of grass-covered dirt.".into(),
                max_stack: 999,
                rarity: Rarity::Common,
                item_type: ItemType::Block,
                icon: "items/grass.png".into(),
                placeable: Some("grass".into()),
                equipment_slot: None,
                stats: None,
            },
            ItemDef {
                id: "torch".into(),
                display_name: "Torch".into(),
                description: "A simple torch that emits warm light.".into(),
                max_stack: 999,
                rarity: Rarity::Common,
                item_type: ItemType::Block,
                icon: "items/torch.png".into(),
                placeable: Some("torch".into()),
                equipment_slot: None,
                stats: None,
            },
        ]))
        .insert_resource(PickupConfig::default())
        .add_systems(Update, dropped_item_physics_system);
    }
}
