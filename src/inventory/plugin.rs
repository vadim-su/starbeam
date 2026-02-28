use bevy::prelude::*;

use super::systems::item_magnetism_system;
use crate::item::PickupConfig;

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PickupConfig::default())
            .add_systems(Update, item_magnetism_system);
    }
}
