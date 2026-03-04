use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

/// Tracks which fluid tiles need simulation this tick.
#[derive(Resource, Default)]
pub struct ActiveFluids {
    /// Tiles to process this tick (world coordinates).
    pub current: HashSet<(i32, i32)>,
    /// Tiles to add to current set next tick (buffered to avoid mutation during iteration).
    pub pending_wake: HashSet<(i32, i32)>,
    /// Settling counter: how many ticks a tile has been unchanged.
    /// Removed when tile goes to sleep.
    pub settle_ticks: HashMap<(i32, i32), u8>,
}

impl ActiveFluids {
    /// Move pending_wake into current for this tick's processing.
    pub fn swap_pending(&mut self) {
        self.current.extend(self.pending_wake.drain());
    }

    /// Wake a tile for next tick's simulation.
    pub fn wake(&mut self, x: i32, y: i32) {
        self.pending_wake.insert((x, y));
    }

    /// Wake a tile and its 4 neighbors.
    pub fn wake_with_neighbors(&mut self, x: i32, y: i32) {
        for (dx, dy) in [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)] {
            self.pending_wake.insert((x + dx, y + dy));
        }
    }

    /// Increment settle counter. Returns true if tile should go to sleep.
    pub fn tick_settle(&mut self, pos: (i32, i32)) -> bool {
        let counter = self.settle_ticks.entry(pos).or_insert(0);
        *counter += 1;
        *counter >= 3
    }

    /// Reset settle counter (tile changed this tick).
    pub fn reset_settle(&mut self, pos: (i32, i32)) {
        self.settle_ticks.remove(&pos);
    }

    /// Remove a tile from active tracking entirely.
    pub fn sleep(&mut self, pos: (i32, i32)) {
        self.current.remove(&pos);
        self.settle_ticks.remove(&pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_pending_moves_to_current() {
        let mut af = ActiveFluids::default();
        af.wake(10, 20);
        af.wake(30, 40);
        assert!(af.current.is_empty());
        af.swap_pending();
        assert_eq!(af.current.len(), 2);
        assert!(af.pending_wake.is_empty());
    }

    #[test]
    fn wake_with_neighbors_adds_5_tiles() {
        let mut af = ActiveFluids::default();
        af.wake_with_neighbors(5, 5);
        assert_eq!(af.pending_wake.len(), 5);
        assert!(af.pending_wake.contains(&(5, 5)));
        assert!(af.pending_wake.contains(&(4, 5)));
        assert!(af.pending_wake.contains(&(6, 5)));
        assert!(af.pending_wake.contains(&(5, 4)));
        assert!(af.pending_wake.contains(&(5, 6)));
    }

    #[test]
    fn settle_counter_sleeps_after_3() {
        let mut af = ActiveFluids::default();
        let pos = (1, 1);
        assert!(!af.tick_settle(pos)); // 1
        assert!(!af.tick_settle(pos)); // 2
        assert!(af.tick_settle(pos)); // 3 → sleep
    }

    #[test]
    fn reset_settle_clears_counter() {
        let mut af = ActiveFluids::default();
        let pos = (1, 1);
        af.tick_settle(pos);
        af.tick_settle(pos);
        af.reset_settle(pos);
        assert!(!af.tick_settle(pos)); // back to 1
    }
}
