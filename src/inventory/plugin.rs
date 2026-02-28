use bevy::prelude::*;

use super::systems::{item_magnetism_system, item_pickup_system, ItemPickupEvent};
use crate::item::PickupConfig;

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PickupConfig::default())
            .add_message::<ItemPickupEvent>()
            .add_systems(Update, (item_magnetism_system, item_pickup_system).chain());
    }
}
