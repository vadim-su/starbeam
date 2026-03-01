use std::collections::HashMap;

use bevy::prelude::*;

use super::components::{BagTarget, Inventory};
use crate::item::EquipmentSlot;

/// Player equipment component.
#[derive(Component, Debug)]
pub struct Equipment {
    slots: HashMap<EquipmentSlot, Option<String>>,
}

impl Equipment {
    pub fn new() -> Self {
        let mut slots = HashMap::new();
        for slot in [
            EquipmentSlot::Head,
            EquipmentSlot::Chest,
            EquipmentSlot::Legs,
            EquipmentSlot::Back,
            EquipmentSlot::Accessory1,
            EquipmentSlot::Accessory2,
            EquipmentSlot::Accessory3,
            EquipmentSlot::Accessory4,
            EquipmentSlot::Weapon1,
            EquipmentSlot::Weapon2,
            EquipmentSlot::Pet,
            EquipmentSlot::CosmeticHead,
            EquipmentSlot::CosmeticChest,
            EquipmentSlot::CosmeticLegs,
            EquipmentSlot::CosmeticBack,
        ] {
            slots.insert(slot, None);
        }
        Self { slots }
    }

    pub fn get(&self, slot: EquipmentSlot) -> Option<&String> {
        self.slots.get(&slot).and_then(|s| s.as_ref())
    }

    /// Low-level equip (sets slot directly, no inventory interaction).
    pub fn equip(&mut self, slot: EquipmentSlot, item_id: String) {
        self.slots.insert(slot, Some(item_id));
    }

    /// Low-level unequip (clears slot, returns item_id).
    pub fn unequip(&mut self, slot: EquipmentSlot) -> Option<String> {
        self.slots.insert(slot, None).flatten()
    }

    /// Equip item from inventory: removes 1 from inventory, places in slot.
    /// If a different item is already equipped, it is returned to inventory.
    /// Returns false if the item is not in inventory or inventory is full for swap.
    pub fn equip_from_inventory(
        &mut self,
        slot: EquipmentSlot,
        item_id: &str,
        inventory: &mut Inventory,
    ) -> bool {
        if inventory.count_item(item_id) == 0 {
            return false;
        }

        // Already equipped — nothing to do
        if self.get(slot).is_some_and(|id| id == item_id) {
            return true;
        }

        // Return currently equipped item to inventory (if any)
        if let Some(old_id) = self.get(slot) {
            let old_id = old_id.clone();
            // Put old item back (non-stackable equipment → max_stack 1, main bag)
            let remaining = inventory.try_add_item(&old_id, 1, 1, BagTarget::Main);
            if remaining > 0 {
                return false; // Inventory full — can't swap
            }
        }

        inventory.remove_item(item_id, 1);
        self.equip(slot, item_id.to_string());
        true
    }

    /// Unequip item and return it to inventory.
    /// Returns false if nothing is equipped in that slot.
    pub fn unequip_to_inventory(&mut self, slot: EquipmentSlot, inventory: &mut Inventory) -> bool {
        let Some(item_id) = self.unequip(slot) else {
            return false;
        };

        // Equipment items are non-stackable (max_stack 1)
        let remaining = inventory.try_add_item(&item_id, 1, 1, BagTarget::Main);
        if remaining > 0 {
            // Inventory full — re-equip
            self.equip(slot, item_id);
            return false;
        }
        true
    }
}

impl Default for Equipment {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::EquipmentSlot;

    #[test]
    fn equipment_starts_empty() {
        let equip = Equipment::new();

        assert!(equip.get(EquipmentSlot::Head).is_none());
        assert!(equip.get(EquipmentSlot::Chest).is_none());
    }

    #[test]
    fn equipment_can_equip_item() {
        let mut equip = Equipment::new();

        equip.equip(EquipmentSlot::Head, "iron_helmet".into());

        assert_eq!(equip.get(EquipmentSlot::Head), Some(&"iron_helmet".into()));
    }

    #[test]
    fn equipment_unequip_returns_item() {
        let mut equip = Equipment::new();
        equip.equip(EquipmentSlot::Head, "iron_helmet".into());

        let item = equip.unequip(EquipmentSlot::Head);

        assert_eq!(item, Some("iron_helmet".into()));
        assert!(equip.get(EquipmentSlot::Head).is_none());
    }

    #[test]
    fn equip_from_inventory_removes_from_bag() {
        let mut equip = Equipment::new();
        let mut inv = Inventory::new();
        inv.try_add_item("iron_helmet", 1, 1, BagTarget::Main);

        assert!(equip.equip_from_inventory(EquipmentSlot::Head, "iron_helmet", &mut inv));
        assert_eq!(equip.get(EquipmentSlot::Head), Some(&"iron_helmet".into()));
        assert_eq!(inv.count_item("iron_helmet"), 0);
    }

    #[test]
    fn equip_from_inventory_fails_without_item() {
        let mut equip = Equipment::new();
        let mut inv = Inventory::new();

        assert!(!equip.equip_from_inventory(EquipmentSlot::Head, "iron_helmet", &mut inv));
        assert!(equip.get(EquipmentSlot::Head).is_none());
    }

    #[test]
    fn equip_from_inventory_swaps_old_item() {
        let mut equip = Equipment::new();
        let mut inv = Inventory::new();
        inv.try_add_item("gold_helmet", 1, 1, BagTarget::Main);

        // Equip iron first
        equip.equip(EquipmentSlot::Head, "iron_helmet".into());

        // Equip gold — iron should go back to inventory
        assert!(equip.equip_from_inventory(EquipmentSlot::Head, "gold_helmet", &mut inv));
        assert_eq!(equip.get(EquipmentSlot::Head), Some(&"gold_helmet".into()));
        assert_eq!(inv.count_item("iron_helmet"), 1);
    }

    #[test]
    fn unequip_to_inventory_returns_item() {
        let mut equip = Equipment::new();
        let mut inv = Inventory::new();
        equip.equip(EquipmentSlot::Head, "iron_helmet".into());

        assert!(equip.unequip_to_inventory(EquipmentSlot::Head, &mut inv));
        assert!(equip.get(EquipmentSlot::Head).is_none());
        assert_eq!(inv.count_item("iron_helmet"), 1);
    }

    #[test]
    fn unequip_to_inventory_fails_when_empty() {
        let mut equip = Equipment::new();
        let mut inv = Inventory::new();

        assert!(!equip.unequip_to_inventory(EquipmentSlot::Head, &mut inv));
    }
}
