use bevy::prelude::*;

use super::definition::{ItemDef, ItemType, Rarity};
use super::dropped_item::despawn_expired_drops;
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
                placeable_object: None,
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
                placeable_object: None,
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
                placeable_object: None,
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
                placeable: None,
                placeable_object: Some("torch_object".into()),
                equipment_slot: None,
                stats: None,
            },
        ]))
        .add_systems(Update, despawn_expired_drops);
    }
}
