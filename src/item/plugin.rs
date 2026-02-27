use bevy::prelude::*;

use super::registry::ItemRegistry;

pub struct ItemPlugin;

impl Plugin for ItemPlugin {
    fn build(&self, app: &mut App) {
        // For now, insert empty registry (will be loaded from assets later)
        app.insert_resource(ItemRegistry::from_defs(vec![]));
    }
}
