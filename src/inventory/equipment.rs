use std::collections::HashMap;

use bevy::prelude::*;

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

    pub fn equip(&mut self, slot: EquipmentSlot, item_id: String) {
        self.slots.insert(slot, Some(item_id));
    }

    pub fn unequip(&mut self, slot: EquipmentSlot) -> Option<String> {
        self.slots.insert(slot, None).flatten()
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
}
