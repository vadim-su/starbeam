use bevy::prelude::*;

/// A single hotbar slot with left/right hand items.
#[derive(Clone, Debug, Default)]
pub struct HotbarSlot {
    pub left_hand: Option<String>,
    pub right_hand: Option<String>,
}

/// Player hotbar component (Starbound-style).
#[derive(Component, Debug)]
pub struct Hotbar {
    pub slots: [HotbarSlot; 6],
    pub active_slot: usize,
    pub locked: bool,
}

impl Hotbar {
    pub fn new() -> Self {
        Self {
            slots: Default::default(),
            active_slot: 0,
            locked: false,
        }
    }

    pub fn select_slot(&mut self, slot: usize) {
        self.active_slot = slot % 6;
    }

    pub fn active_slot(&self) -> &HotbarSlot {
        &self.slots[self.active_slot]
    }

    pub fn get_item_for_hand(&self, is_left: bool) -> Option<&str> {
        let slot = self.active_slot();
        if is_left {
            slot.left_hand.as_deref()
        } else {
            slot.right_hand.as_deref()
        }
    }
}

impl Default for Hotbar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotbar_has_6_slots() {
        let hotbar = Hotbar::new();
        assert_eq!(hotbar.slots.len(), 6);
    }

    #[test]
    fn hotbar_active_slot_defaults_to_zero() {
        let hotbar = Hotbar::new();
        assert_eq!(hotbar.active_slot, 0);
    }

    #[test]
    fn hotbar_select_slot_wraps() {
        let mut hotbar = Hotbar::new();
        hotbar.select_slot(5);
        assert_eq!(hotbar.active_slot, 5);

        hotbar.select_slot(6); // Should wrap
        assert_eq!(hotbar.active_slot, 0);
    }
}
