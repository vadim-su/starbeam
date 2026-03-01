//! Maps item IDs to their icon textures.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::item::ItemId;

/// Registry mapping item IDs to icon image handles.
#[derive(Resource)]
pub struct ItemIconRegistry {
    icons: HashMap<ItemId, Handle<Image>>,
}

impl ItemIconRegistry {
    pub fn new() -> Self {
        Self {
            icons: HashMap::new(),
        }
    }

    /// Register an icon for an item.
    pub fn register(&mut self, id: ItemId, handle: Handle<Image>) {
        self.icons.insert(id, handle);
    }

    /// Get icon handle for an item by ItemId.
    pub fn get(&self, id: ItemId) -> Option<&Handle<Image>> {
        self.icons.get(&id)
    }
}

impl Default for ItemIconRegistry {
    fn default() -> Self {
        Self::new()
    }
}
