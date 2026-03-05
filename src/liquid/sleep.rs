use std::collections::{HashMap, HashSet};

/// Tracks which tiles are "active" (need simulation).
/// Sleeping tiles are not processed until woken.
#[derive(Default)]
pub struct SleepTracker {
    /// Set of active tile coordinates (world tile_x, tile_y).
    active: HashSet<(i32, i32)>,
    /// Tiles that have been stable for consecutive steps.
    stable_count: HashMap<(i32, i32), u8>,
}

const SLEEP_THRESHOLD: u8 = 5;
const MAX_ACTIVE_PER_STEP: usize = 20_000;

impl SleepTracker {
    pub fn wake(&mut self, tile_x: i32, tile_y: i32) {
        self.active.insert((tile_x, tile_y));
        self.stable_count.remove(&(tile_x, tile_y));
    }

    pub fn wake_region(&mut self, min_x: i32, min_y: i32, max_x: i32, max_y: i32) {
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                self.wake(x, y);
            }
        }
    }

    /// Wake a tile and its 4 neighbors.
    pub fn wake_with_neighbors(&mut self, tile_x: i32, tile_y: i32) {
        self.wake(tile_x, tile_y);
        self.wake(tile_x + 1, tile_y);
        self.wake(tile_x - 1, tile_y);
        self.wake(tile_x, tile_y + 1);
        self.wake(tile_x, tile_y - 1);
    }

    /// Mark a tile as stable this step. If stable long enough, put it to sleep.
    pub fn mark_stable(&mut self, tile_x: i32, tile_y: i32) {
        let count = self.stable_count.entry((tile_x, tile_y)).or_insert(0);
        *count = count.saturating_add(1);
        if *count >= SLEEP_THRESHOLD {
            self.active.remove(&(tile_x, tile_y));
            self.stable_count.remove(&(tile_x, tile_y));
        }
    }

    /// Mark a tile as changed — reset stable count and wake neighbors.
    pub fn mark_changed(&mut self, tile_x: i32, tile_y: i32) {
        self.stable_count.remove(&(tile_x, tile_y));
        self.wake_with_neighbors(tile_x, tile_y);
    }

    /// Iterator over active tiles, capped at MAX_ACTIVE_PER_STEP.
    pub fn active_tiles(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.active.iter().copied().take(MAX_ACTIVE_PER_STEP)
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Remove tiles outside the simulation bounds.
    pub fn cull_outside(&mut self, min_x: i32, min_y: i32, max_x: i32, max_y: i32) {
        self.active
            .retain(|&(x, y)| x >= min_x && x <= max_x && y >= min_y && y <= max_y);
    }
}
