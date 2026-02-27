use bevy::prelude::*;

/// A single slot in the inventory.
#[derive(Clone, Debug, PartialEq)]
pub struct InventorySlot {
    pub item_id: String,
    pub count: u16,
}

/// Player inventory component.
#[derive(Component, Debug)]
pub struct Inventory {
    pub main_bag: Vec<Option<InventorySlot>>,
    pub material_bag: Vec<Option<InventorySlot>>,
    pub max_slots_base: usize,
    pub max_slots_bonus: usize,
}

impl Inventory {
    pub fn new() -> Self {
        Self {
            main_bag: vec![None; 40],
            material_bag: vec![None; 40],
            max_slots_base: 40,
            max_slots_bonus: 0,
        }
    }

    pub fn total_slots(&self) -> usize {
        self.max_slots_base + self.max_slots_bonus
    }
}

impl Default for Inventory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_slot_tracks_item_and_count() {
        let slot = InventorySlot {
            item_id: "dirt".into(),
            count: 50,
        };

        assert_eq!(slot.item_id, "dirt");
        assert_eq!(slot.count, 50);
    }

    #[test]
    fn inventory_starts_empty() {
        let inv = Inventory::new();

        assert_eq!(inv.main_bag.len(), 40);
        assert_eq!(inv.material_bag.len(), 40);
        assert!(inv.main_bag.iter().all(|s| s.is_none()));
    }

    #[test]
    fn inventory_total_slots_includes_bonus() {
        let mut inv = Inventory::new();
        assert_eq!(inv.total_slots(), 40);

        inv.max_slots_bonus = 10;
        assert_eq!(inv.total_slots(), 50);
    }
}
