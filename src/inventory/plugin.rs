use bevy::prelude::*;

use super::systems::{
    hotbar_input_system, item_magnetism_system, item_pickup_system, ItemPickupEvent,
};
use crate::item::PickupConfig;
use crate::sets::GameSet;

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PickupConfig::default())
            .add_message::<ItemPickupEvent>()
            .add_systems(Update, hotbar_input_system.in_set(GameSet::Input))
            .add_systems(Update, (item_magnetism_system, item_pickup_system).chain());
    }
}
