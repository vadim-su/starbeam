use bevy::prelude::*;

use super::dropped_item::despawn_expired_drops;

pub struct ItemPlugin;

impl Plugin for ItemPlugin {
    fn build(&self, app: &mut App) {
        // ItemRegistry is now built from item.ron files during the registry
        // loading pipeline (see registry/loading.rs check_loading).
        app.add_systems(Update, despawn_expired_drops);
    }
}
