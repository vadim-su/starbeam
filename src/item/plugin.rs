use bevy::prelude::*;

use super::dropped_item::{dropped_item_physics_system, PickupConfig};
use super::registry::ItemRegistry;

pub struct ItemPlugin;

impl Plugin for ItemPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ItemRegistry::from_defs(vec![]))
            .insert_resource(PickupConfig::default())
            .add_systems(Update, dropped_item_physics_system);
    }
}
