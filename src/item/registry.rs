use std::collections::HashMap;

use bevy::prelude::*;

use super::definition::ItemDef;

/// Compact item identifier. Index into ItemRegistry.defs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ItemId(pub u16);

/// Registry of all item definitions. Inserted as a Resource after asset loading.
#[derive(Resource, Debug)]
pub struct ItemRegistry {
    defs: Vec<ItemDef>,
    name_to_id: HashMap<String, ItemId>,
}

impl ItemRegistry {
    /// Build registry from a list of ItemDefs. Order = ItemId index.
    pub fn from_defs(defs: Vec<ItemDef>) -> Self {
        let name_to_id = defs
            .iter()
            .enumerate()
            .map(|(i, d)| (d.id.clone(), ItemId(i as u16)))
            .collect();
        Self { defs, name_to_id }
    }

    pub fn get(&self, id: ItemId) -> &ItemDef {
        &self.defs[id.0 as usize]
    }

    /// Try to get an ItemDef, returning None for out-of-bounds IDs.
    pub fn try_get(&self, id: ItemId) -> Option<&ItemDef> {
        self.defs.get(id.0 as usize)
    }

    pub fn max_stack(&self, id: ItemId) -> u16 {
        self.defs[id.0 as usize].max_stack
    }

    /// Look up item by name. Returns None for unknown items.
    pub fn by_name(&self, name: &str) -> Option<ItemId> {
        self.name_to_id.get(name).copied()
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{ItemType, Rarity};

    fn test_registry() -> ItemRegistry {
        ItemRegistry::from_defs(vec![
            ItemDef {
                id: "dirt".into(),
                display_name: "Dirt Block".into(),
                description: "A block of dirt".into(),
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
                display_name: "Stone".into(),
                description: "A block of stone".into(),
                max_stack: 999,
                rarity: Rarity::Common,
                item_type: ItemType::Block,
                icon: "items/stone.png".into(),
                placeable: Some("stone".into()),
                placeable_object: None,
                equipment_slot: None,
                stats: None,
            },
        ])
    }

    #[test]
    fn registry_lookup_by_name() {
        let reg = test_registry();
        assert_eq!(reg.by_name("dirt"), Some(ItemId(0)));
        assert_eq!(reg.by_name("stone"), Some(ItemId(1)));
    }

    #[test]
    fn by_name_returns_none_on_unknown() {
        let reg = test_registry();
        assert_eq!(reg.by_name("nonexistent_item"), None);
    }

    #[test]
    fn registry_get_returns_def() {
        let reg = test_registry();
        let dirt = reg.get(ItemId(0));
        assert_eq!(dirt.id, "dirt");
        assert_eq!(dirt.max_stack, 999);
    }

    #[test]
    fn try_get_returns_none_for_invalid_id() {
        let reg = test_registry();
        assert!(reg.try_get(ItemId(999)).is_none());
    }

    #[test]
    fn registry_max_stack() {
        let reg = test_registry();
        assert_eq!(reg.max_stack(ItemId(0)), 999);
        assert_eq!(reg.max_stack(ItemId(1)), 999);
    }
}
