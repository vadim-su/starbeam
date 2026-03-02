use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// A stack of items with ID and count.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stack {
    pub item_id: String,
    pub count: u16,
}

/// A single slot in the inventory — type alias for Stack.
pub type InventorySlot = Stack;

/// Which bag an item should be routed to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BagTarget {
    /// Blocks and materials go to the material bag.
    Material,
    /// Everything else goes to the main bag.
    Main,
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

    /// Try to add an item to the specified bag.
    /// Returns the count that couldn't fit.
    pub fn try_add_item(
        &mut self,
        item_id: &str,
        count: u16,
        max_stack: u16,
        target: BagTarget,
    ) -> u16 {
        let bag = match target {
            BagTarget::Material => &mut self.material_bag,
            BagTarget::Main => &mut self.main_bag,
        };
        let mut remaining = Self::try_stack_into(bag, item_id, count, max_stack);

        // Overflow into the other bag if primary is full
        if remaining > 0 {
            let overflow_bag = match target {
                BagTarget::Material => &mut self.main_bag,
                BagTarget::Main => &mut self.material_bag,
            };
            remaining = Self::try_stack_into(overflow_bag, item_id, remaining, max_stack);
        }

        remaining
    }

    /// Stack items into a specific bag. Returns remainder.
    fn try_stack_into(
        bag: &mut [Option<InventorySlot>],
        item_id: &str,
        count: u16,
        max_stack: u16,
    ) -> u16 {
        let mut remaining = count;

        // First, stack into existing slots
        for slot in bag.iter_mut() {
            if remaining == 0 {
                break;
            }
            if let Some(s) = slot
                && s.item_id == item_id
                && s.count < max_stack
            {
                let can_add = max_stack - s.count;
                let to_add = remaining.min(can_add);
                s.count += to_add;
                remaining -= to_add;
            }
        }

        // Then, create new slots
        if remaining > 0 {
            for slot in bag.iter_mut() {
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

    /// Count total items of a specific type across all bags. Returns u32 to avoid overflow.
    pub fn count_item(&self, item_id: &str) -> u32 {
        self.main_bag
            .iter()
            .chain(self.material_bag.iter())
            .filter_map(|s| s.as_ref())
            .filter(|s| s.item_id == item_id)
            .map(|s| s.count as u32)
            .sum()
    }

    /// Remove items from inventory (both bags). Returns true if successful.
    pub fn remove_item(&mut self, item_id: &str, count: u16) -> bool {
        let total = self.count_item(item_id);
        if total < count as u32 {
            return false;
        }

        let mut remaining = count;

        for slot in self.main_bag.iter_mut().chain(self.material_bag.iter_mut()) {
            if remaining == 0 {
                break;
            }

            if let Some(s) = slot
                && s.item_id == item_id
            {
                let to_remove = remaining.min(s.count);
                s.count -= to_remove;
                remaining -= to_remove;

                if s.count == 0 {
                    *slot = None;
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
    fn try_add_item_to_empty_main_bag() {
        let mut inv = Inventory::new();
        let remaining = inv.try_add_item("dirt", 10, 999, BagTarget::Main);

        assert_eq!(remaining, 0);
        assert!(inv.main_bag[0].is_some());
        assert_eq!(inv.main_bag[0].as_ref().unwrap().count, 10);
    }

    #[test]
    fn try_add_item_to_material_bag() {
        let mut inv = Inventory::new();
        let remaining = inv.try_add_item("dirt", 10, 999, BagTarget::Material);

        assert_eq!(remaining, 0);
        assert!(inv.material_bag[0].is_some());
        assert_eq!(inv.material_bag[0].as_ref().unwrap().count, 10);
        // Main bag should be empty
        assert!(inv.main_bag.iter().all(|s| s.is_none()));
    }

    #[test]
    fn try_add_item_overflows_to_other_bag() {
        let mut inv = Inventory::new();
        // Fill all material bag slots
        for slot in inv.material_bag.iter_mut() {
            *slot = Some(InventorySlot {
                item_id: "stone".into(),
                count: 999,
            });
        }
        // Adding to material bag should overflow into main bag
        let remaining = inv.try_add_item("dirt", 10, 999, BagTarget::Material);
        assert_eq!(remaining, 0);
        assert_eq!(inv.main_bag[0].as_ref().unwrap().item_id, "dirt");
    }

    #[test]
    fn try_add_item_stacks_into_existing() {
        let mut inv = Inventory::new();
        inv.main_bag[0] = Some(InventorySlot {
            item_id: "dirt".into(),
            count: 50,
        });

        let remaining = inv.try_add_item("dirt", 30, 999, BagTarget::Main);

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

        let remaining = inv.try_add_item("dirt", 20, 999, BagTarget::Main);

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

        let remaining = inv.try_add_item("dirt", 10, 999, BagTarget::Main);

        assert_eq!(remaining, 0);
        assert_eq!(inv.main_bag[1].as_ref().unwrap().count, 10);
    }

    #[test]
    fn stack_default_is_empty() {
        let stack = Stack::default();
        assert_eq!(stack.item_id, "");
        assert_eq!(stack.count, 0);
    }

    #[test]
    fn stack_equality() {
        let a = Stack {
            item_id: "dirt".into(),
            count: 10,
        };
        let b = Stack {
            item_id: "dirt".into(),
            count: 10,
        };
        let c = Stack {
            item_id: "stone".into(),
            count: 10,
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn stack_clone() {
        let original = Stack {
            item_id: "dirt".into(),
            count: 50,
        };
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn stack_as_inventory_slot() {
        // InventorySlot is a type alias for Stack — verify interchangeability
        let slot: InventorySlot = Stack {
            item_id: "dirt".into(),
            count: 10,
        };
        assert_eq!(slot.item_id, "dirt");
        assert_eq!(slot.count, 10);
    }

    #[test]
    fn try_add_item_returns_remainder_when_both_bags_full() {
        let mut inv = Inventory::new();
        // Fill all slots in both bags
        for slot in inv.main_bag.iter_mut().chain(inv.material_bag.iter_mut()) {
            *slot = Some(InventorySlot {
                item_id: "stone".into(),
                count: 999,
            });
        }

        let remaining = inv.try_add_item("dirt", 10, 999, BagTarget::Main);
        assert_eq!(remaining, 10);
    }

    #[test]
    fn count_item_sums_across_both_bags() {
        let mut inv = Inventory::new();
        inv.main_bag[0] = Some(InventorySlot {
            item_id: "dirt".into(),
            count: 999,
        });
        inv.material_bag[0] = Some(InventorySlot {
            item_id: "dirt".into(),
            count: 999,
        });
        // u32 result can hold totals > u16::MAX
        assert_eq!(inv.count_item("dirt"), 1998);
    }

    #[test]
    fn remove_item_spans_both_bags() {
        let mut inv = Inventory::new();
        inv.main_bag[0] = Some(InventorySlot {
            item_id: "dirt".into(),
            count: 5,
        });
        inv.material_bag[0] = Some(InventorySlot {
            item_id: "dirt".into(),
            count: 5,
        });
        assert!(inv.remove_item("dirt", 8));
        assert_eq!(inv.count_item("dirt"), 2);
    }

    #[test]
    fn remove_item_fails_when_not_enough() {
        let mut inv = Inventory::new();
        inv.main_bag[0] = Some(InventorySlot {
            item_id: "dirt".into(),
            count: 3,
        });
        assert!(!inv.remove_item("dirt", 5));
        // Inventory unchanged on failure
        assert_eq!(inv.count_item("dirt"), 3);
    }

    #[test]
    fn remove_item_clears_empty_slots() {
        let mut inv = Inventory::new();
        inv.main_bag[0] = Some(InventorySlot {
            item_id: "dirt".into(),
            count: 5,
        });
        assert!(inv.remove_item("dirt", 5));
        assert!(inv.main_bag[0].is_none());
    }

    #[test]
    fn count_item_returns_zero_for_missing() {
        let inv = Inventory::new();
        assert_eq!(inv.count_item("nonexistent"), 0);
    }

    #[test]
    fn bag_target_routing_preserves_separation() {
        let mut inv = Inventory::new();
        inv.try_add_item("stone", 50, 999, BagTarget::Material);
        inv.try_add_item("sword", 1, 1, BagTarget::Main);

        // Stone in material bag, sword in main bag
        assert!(inv.material_bag[0].is_some());
        assert_eq!(inv.material_bag[0].as_ref().unwrap().item_id, "stone");
        assert!(inv.main_bag[0].is_some());
        assert_eq!(inv.main_bag[0].as_ref().unwrap().item_id, "sword");
    }
}
