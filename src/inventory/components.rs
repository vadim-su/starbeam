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

    /// Try to add an item to the inventory.
    /// Returns the count that couldn't fit.
    pub fn try_add_item(&mut self, item_id: &str, count: u16, max_stack: u16) -> u16 {
        let mut remaining = count;

        // First, try to stack into existing slots
        for slot in self.main_bag.iter_mut() {
            if remaining == 0 {
                break;
            }

            if let Some(s) = slot {
                if s.item_id == item_id && s.count < max_stack {
                    let can_add = max_stack - s.count;
                    let to_add = remaining.min(can_add);
                    s.count += to_add;
                    remaining -= to_add;
                }
            }
        }

        // Then, try to create new slots
        if remaining > 0 {
            for slot in self.main_bag.iter_mut() {
                if remaining == 0 {
                    break;
                }

                if slot.is_none() {
                    let to_add = remaining.min(max_stack);
                    *slot = Some(InventorySlot {
                        item_id: item_id.to_string(),
                        count: to_add,
                    });
                    remaining -= to_add;
                }
            }
        }

        remaining
    }

    /// Count total items of a specific type.
    pub fn count_item(&self, item_id: &str) -> u16 {
        self.main_bag
            .iter()
            .chain(self.material_bag.iter())
            .filter_map(|s| s.as_ref())
            .filter(|s| s.item_id == item_id)
            .map(|s| s.count)
            .sum()
    }

    /// Remove items from inventory. Returns true if successful.
    pub fn remove_item(&mut self, item_id: &str, count: u16) -> bool {
        let total = self.count_item(item_id);
        if total < count {
            return false;
        }

        let mut remaining = count;

        for slot in self.main_bag.iter_mut().chain(self.material_bag.iter_mut()) {
            if remaining == 0 {
                break;
            }

            if let Some(s) = slot {
                if s.item_id == item_id {
                    let to_remove = remaining.min(s.count);
                    s.count -= to_remove;
                    remaining -= to_remove;

                    if s.count == 0 {
                        *slot = None;
                    }
                }
            }
        }

        true
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

    #[test]
    fn try_add_item_to_empty_slot() {
        let mut inv = Inventory::new();
        let remaining = inv.try_add_item("dirt", 10, 999);

        assert_eq!(remaining, 0);
        assert!(inv.main_bag[0].is_some());
        assert_eq!(inv.main_bag[0].as_ref().unwrap().count, 10);
    }

    #[test]
    fn try_add_item_stacks_into_existing() {
        let mut inv = Inventory::new();
        inv.main_bag[0] = Some(InventorySlot {
            item_id: "dirt".into(),
            count: 50,
        });

        let remaining = inv.try_add_item("dirt", 30, 999);

        assert_eq!(remaining, 0);
        assert_eq!(inv.main_bag[0].as_ref().unwrap().count, 80);
    }

    #[test]
    fn try_add_item_respects_max_stack() {
        let mut inv = Inventory::new();
        inv.main_bag[0] = Some(InventorySlot {
            item_id: "dirt".into(),
            count: 990,
        });

        let remaining = inv.try_add_item("dirt", 20, 999);

        // 990 + 9 = 999 (max_stack respected), remaining 11 goes to new slot
        assert_eq!(remaining, 0);
        assert_eq!(inv.main_bag[0].as_ref().unwrap().count, 999);
        assert_eq!(inv.main_bag[1].as_ref().unwrap().count, 11);
    }

    #[test]
    fn try_add_item_creates_new_slot_when_full() {
        let mut inv = Inventory::new();
        inv.main_bag[0] = Some(InventorySlot {
            item_id: "dirt".into(),
            count: 999,
        });

        let remaining = inv.try_add_item("dirt", 10, 999);

        assert_eq!(remaining, 0);
        assert_eq!(inv.main_bag[1].as_ref().unwrap().count, 10);
    }

    #[test]
    fn try_add_item_returns_remainder_when_inventory_full() {
        let mut inv = Inventory::new();
        // Fill all slots
        for slot in inv.main_bag.iter_mut() {
            *slot = Some(InventorySlot {
                item_id: "stone".into(),
                count: 999,
            });
        }

        let remaining = inv.try_add_item("dirt", 10, 999);

        assert_eq!(remaining, 10);
    }
}
