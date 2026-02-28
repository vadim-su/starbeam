//! Maps item IDs to their icon textures.

use std::collections::HashMap;

use bevy::prelude::*;

/// Registry mapping item IDs to icon image handles.
#[derive(Resource)]
pub struct ItemIconRegistry {
    icons: HashMap<String, Handle<Image>>,
}

impl ItemIconRegistry {
    pub fn new() -> Self {
        Self {
            icons: HashMap::new(),
        }
    }

    /// Register an icon for an item.
    pub fn register(&mut self, item_id: &str, handle: Handle<Image>) {
        self.icons.insert(item_id.to_string(), handle);
    }

    /// Get icon handle for an item.
    pub fn get(&self, item_id: &str) -> Option<&Handle<Image>> {
        self.icons.get(item_id)
    }
}

impl Default for ItemIconRegistry {
    fn default() -> Self {
        Self::new()
    }
}
