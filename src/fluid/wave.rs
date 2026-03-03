use bevy::prelude::*;

use std::collections::{HashMap, HashSet};

use crate::fluid::cell::FluidCell;

// ---- Wave Buffer ----

/// Per-chunk wave state for dynamic surface waves.
///
/// Stores a height field and velocity field over a 2D grid of cells.
/// The wave equation is iterated via `step()`, which propagates waves
/// through non-empty fluid cells using a 4-connected neighborhood.
#[derive(Debug, Clone)]
pub struct WaveBuffer {
    pub height: Vec<f32>,
    pub velocity: Vec<f32>,
    prev_height: Vec<f32>,
    pub chunk_size: u32,
}

impl WaveBuffer {
    /// Create a new zeroed wave buffer for a chunk of the given size.
    pub fn new(chunk_size: u32) -> Self {
        let len = (chunk_size * chunk_size) as usize;
        Self {
            height: vec![0.0; len],
            velocity: vec![0.0; len],
            prev_height: vec![0.0; len],
            chunk_size,
        }
    }

    /// Returns true if all heights and velocities are near zero.
    pub fn is_calm(&self, epsilon: f32) -> bool {
        self.height
            .iter()
            .zip(self.velocity.iter())
            .all(|(h, v)| h.abs() < epsilon && v.abs() < epsilon)
    }

    /// Add an impulse to the velocity at the given local cell coordinates.
    /// The impulse is clamped to `[-max_impulse, max_impulse]` before being applied.
    pub fn apply_impulse(&mut self, local_x: u32, local_y: u32, impulse: f32, max_impulse: f32) {
        let idx = (local_y * self.chunk_size + local_x) as usize;
        if idx < self.velocity.len() {
            self.velocity[idx] += impulse.clamp(-max_impulse, max_impulse);
        }
    }

    /// Run one iteration of the wave equation.
    ///
    /// For each fluid cell, computes the average height of 4-connected
    /// non-empty fluid neighbors, then updates velocity and height.
    /// Empty cells are zeroed out. Heights are clamped to `[-max_height, max_height]`.
    pub fn step(&mut self, fluids: &[FluidCell], config: &WaveConfig) {
        let size = self.chunk_size;
        let len = (size * size) as usize;

        // Swap buffers: prev_height now holds last tick's heights,
        // and height becomes the write target (avoids per-tick clone).
        std::mem::swap(&mut self.height, &mut self.prev_height);

        for i in 0..len {
            if fluids[i].is_empty() {
                self.height[i] = 0.0;
                self.velocity[i] = 0.0;
                continue;
            }

            let x = (i as u32) % size;
            let y = (i as u32) / size;

            let mut sum = 0.0;
            let mut count = 0u32;

            if x > 0 {
                let ni = (y * size + (x - 1)) as usize;
                if !fluids[ni].is_empty() {
                    sum += self.prev_height[ni];
                    count += 1;
                }
            }
            if x + 1 < size {
                let ni = (y * size + (x + 1)) as usize;
                if !fluids[ni].is_empty() {
                    sum += self.prev_height[ni];
                    count += 1;
                }
            }
            if y > 0 {
                let ni = ((y - 1) * size + x) as usize;
                if !fluids[ni].is_empty() {
                    sum += self.prev_height[ni];
                    count += 1;
                }
            }
            if y + 1 < size {
                let ni = ((y + 1) * size + x) as usize;
                if !fluids[ni].is_empty() {
                    sum += self.prev_height[ni];
                    count += 1;
                }
            }

            if count > 0 {
                let avg = sum / count as f32;
                self.velocity[i] += (avg - self.prev_height[i]) * config.speed;
            }

            self.velocity[i] *= config.damping;

            // Nonlinear damping: large waves decay faster to prevent "flying into space"
            if self.prev_height[i].abs() > config.max_height * config.high_wave_threshold {
                self.velocity[i] *= config.high_wave_damping / config.damping;
            }

            // Damping on both channels independently (not on their sum):
            // - velocity *= damping dissipates kinetic energy
            // - prev * damping dissipates potential energy (standing waves in bounded domain)
            self.height[i] = self.prev_height[i] * config.damping + self.velocity[i];
            self.height[i] = self.height[i].clamp(-config.max_height, config.max_height);

            if self.height[i].abs() < config.epsilon && self.velocity[i].abs() < config.epsilon {
                self.height[i] = 0.0;
                self.velocity[i] = 0.0;
            }
        }
    }
}

// ---- Wave Configuration ----

/// Configuration for wave propagation.
#[derive(Resource, Debug, Clone)]
pub struct WaveConfig {
    /// Wave propagation speed factor.
    pub speed: f32,
    /// Damping factor applied to velocity each step (0..1).
    pub damping: f32,
    /// Threshold below which values are considered zero.
    pub epsilon: f32,
    /// Maximum absolute wave height (clamped).
    pub max_height: f32,
    /// Maximum impulse magnitude (clamped on input).
    pub max_impulse: f32,
    /// Fraction of max_height above which extra damping kicks in.
    pub high_wave_threshold: f32,
    /// Damping factor for waves above threshold.
    pub high_wave_damping: f32,
}

impl Default for WaveConfig {
    fn default() -> Self {
        Self {
            speed: 0.15,   // was 0.4 — much calmer wave propagation
            damping: 0.96, // was 0.98 — waves die out faster
            epsilon: 0.001,
            max_height: 0.5,          // ~4px displacement (half a tile)
            max_impulse: 1.0,         // tighter clamp on input impulse
            high_wave_threshold: 0.6, // kick in extra damping earlier
            high_wave_damping: 0.85,  // more aggressive large-wave damping
        }
    }
}

// ---- Wave State ----

/// Holds wave buffers for all active chunks.
#[derive(Resource, Default)]
pub struct WaveState {
    pub buffers: HashMap<(i32, i32), WaveBuffer>,
}

// ---- Cross-chunk boundary reconciliation ----

/// Reconcile wave heights and velocities at boundaries between horizontally adjacent active chunks.
///
/// For each pair of horizontally adjacent active chunks, averages the wave heights
/// and velocities at the shared boundary column (right edge of left chunk ↔ left edge of right chunk).
/// Uses wrapping via `rem_euclid(width_chunks)`.
pub fn reconcile_wave_boundaries(
    wave_state: &mut WaveState,
    active_chunks: &HashSet<(i32, i32)>,
    chunk_size: u32,
    width_chunks: i32,
) {
    // Collect boundary averages first to avoid borrow conflicts.
    // Each entry: (left_chunk, right_chunk, row, averaged_height, averaged_velocity)
    let mut updates: Vec<((i32, i32), (i32, i32), u32, f32, f32)> = Vec::new();

    let mut processed: HashSet<((i32, i32), (i32, i32))> = HashSet::new();

    for &(cx, cy) in active_chunks {
        let right_cx = (cx + 1).rem_euclid(width_chunks);
        let left_key = (cx, cy);
        let right_key = (right_cx, cy);

        let pair_key = (left_key, right_key);
        if !processed.insert(pair_key) {
            continue;
        }

        if !active_chunks.contains(&right_key) {
            continue;
        }

        let left_has = wave_state.buffers.contains_key(&left_key);
        let right_has = wave_state.buffers.contains_key(&right_key);

        // Skip if neither side has a buffer.
        if !left_has && !right_has {
            continue;
        }

        // Create buffer for the side that doesn't have one yet,
        // so waves propagate across the boundary.
        if !left_has {
            wave_state
                .buffers
                .insert(left_key, WaveBuffer::new(chunk_size));
        }
        if !right_has {
            wave_state
                .buffers
                .insert(right_key, WaveBuffer::new(chunk_size));
        }

        let left_buf = wave_state.buffers.get(&left_key).unwrap();
        let right_buf = wave_state.buffers.get(&right_key).unwrap();

        for local_y in 0..chunk_size {
            let left_idx = (local_y * chunk_size + (chunk_size - 1)) as usize;
            let right_idx = (local_y * chunk_size) as usize;

            let avg_h = (left_buf.height[left_idx] + right_buf.height[right_idx]) * 0.5;
            let avg_v = (left_buf.velocity[left_idx] + right_buf.velocity[right_idx]) * 0.5;
            updates.push((left_key, right_key, local_y, avg_h, avg_v));
        }
    }

    // Apply all collected averages
    for (left_key, right_key, local_y, avg_h, avg_v) in updates {
        let left_idx = (local_y * chunk_size + (chunk_size - 1)) as usize;
        let right_idx = (local_y * chunk_size) as usize;

        if let Some(buf) = wave_state.buffers.get_mut(&left_key) {
            buf.height[left_idx] = avg_h;
            buf.velocity[left_idx] = avg_v;
        }
        if let Some(buf) = wave_state.buffers.get_mut(&right_key) {
            buf.height[right_idx] = avg_h;
            buf.velocity[right_idx] = avg_v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::cell::FluidId;

    /// Helper: returns a fluids grid where every cell is water.
    fn all_water(chunk_size: u32) -> Vec<FluidCell> {
        let len = (chunk_size * chunk_size) as usize;
        vec![FluidCell::new(FluidId(1), 1.0); len]
    }

    #[test]
    fn new_buffer_is_calm() {
        let buf = WaveBuffer::new(16);
        assert!(buf.is_calm(0.001));
    }

    #[test]
    fn impulse_creates_wave() {
        let mut buf = WaveBuffer::new(16);
        buf.apply_impulse(8, 8, 1.0, 2.0);

        assert!(!buf.is_calm(0.001));

        let idx = (8 * 16 + 8) as usize;
        assert!((buf.velocity[idx] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn wave_propagates_to_neighbors() {
        let config = WaveConfig::default();
        let chunk_size = 16u32;
        let fluids = all_water(chunk_size);
        let mut buf = WaveBuffer::new(chunk_size);

        buf.apply_impulse(8, 8, 2.0, 2.0);

        // Run several steps to let the wave propagate
        for _ in 0..10 {
            buf.step(&fluids, &config);
        }

        // Check that neighbors of (8,8) have non-zero height
        let left = (8 * chunk_size + 7) as usize;
        let right = (8 * chunk_size + 9) as usize;
        let down = (7 * chunk_size + 8) as usize;
        let up = (9 * chunk_size + 8) as usize;

        assert!(
            buf.height[left].abs() > 0.001,
            "left neighbor should have wave"
        );
        assert!(
            buf.height[right].abs() > 0.001,
            "right neighbor should have wave"
        );
        assert!(
            buf.height[down].abs() > 0.001,
            "down neighbor should have wave"
        );
        assert!(buf.height[up].abs() > 0.001, "up neighbor should have wave");
    }

    #[test]
    fn wave_does_not_propagate_through_empty() {
        let config = WaveConfig::default();
        let chunk_size = 8u32;
        let mut fluids = all_water(chunk_size);

        // Create a wall of empty cells at x=4 (blocking left-right propagation)
        for y in 0..chunk_size {
            let idx = (y * chunk_size + 4) as usize;
            fluids[idx] = FluidCell::EMPTY;
        }

        let mut buf = WaveBuffer::new(chunk_size);
        // Impulse on the left side at (2, 4)
        buf.apply_impulse(2, 4, 2.0, 2.0);

        for _ in 0..20 {
            buf.step(&fluids, &config);
        }

        // Cells on the right side of the wall should remain calm
        for y in 0..chunk_size {
            for x in 5..chunk_size {
                let idx = (y * chunk_size + x) as usize;
                assert!(
                    buf.height[idx].abs() < 0.001,
                    "cell ({x}, {y}) should be calm but height={}",
                    buf.height[idx]
                );
            }
        }
    }

    #[test]
    fn wave_decays_to_calm() {
        let config = WaveConfig::default();
        let chunk_size = 16u32;
        let fluids = all_water(chunk_size);
        let mut buf = WaveBuffer::new(chunk_size);

        buf.apply_impulse(8, 8, 2.0, 2.0);

        for _ in 0..1000 {
            buf.step(&fluids, &config);
        }

        assert!(
            buf.is_calm(config.epsilon),
            "buffer should be calm after many steps"
        );
    }

    #[test]
    fn max_height_clamped() {
        let config = WaveConfig::default();
        let chunk_size = 8u32;
        let fluids = all_water(chunk_size);
        let mut buf = WaveBuffer::new(chunk_size);

        // Apply a huge impulse
        buf.apply_impulse(4, 4, 100.0, 200.0);
        buf.step(&fluids, &config);

        let idx = (4 * chunk_size + 4) as usize;
        assert!(
            buf.height[idx].abs() <= config.max_height,
            "height {} should be clamped to max_height {}",
            buf.height[idx],
            config.max_height
        );
    }

    #[test]
    fn impulse_is_clamped() {
        let mut buf = WaveBuffer::new(8);
        buf.apply_impulse(4, 4, 100.0, 3.0);
        let idx = (4 * 8 + 4) as usize;
        assert!(
            (buf.velocity[idx] - 3.0).abs() < 1e-5,
            "impulse should be clamped to max_impulse=3.0, got {}",
            buf.velocity[idx]
        );
    }

    #[test]
    fn reconcile_averages_velocity_at_boundary() {
        let chunk_size = 4u32;
        let mut wave_state = WaveState::default();
        let mut active = HashSet::new();

        active.insert((0, 0));
        active.insert((1, 0));

        let mut left = WaveBuffer::new(chunk_size);
        let mut right = WaveBuffer::new(chunk_size);

        // Set velocity at boundary: left's right edge has vel=2.0, right's left edge has vel=0.0
        let left_idx = (0 * chunk_size + (chunk_size - 1)) as usize;
        let right_idx = (0 * chunk_size + 0) as usize;
        left.velocity[left_idx] = 2.0;
        right.velocity[right_idx] = 0.0;

        wave_state.buffers.insert((0, 0), left);
        wave_state.buffers.insert((1, 0), right);

        reconcile_wave_boundaries(&mut wave_state, &active, chunk_size, 2);

        let left_vel = wave_state.buffers[&(0, 0)].velocity[left_idx];
        let right_vel = wave_state.buffers[&(1, 0)].velocity[right_idx];
        assert!(
            (left_vel - 1.0).abs() < 1e-5,
            "left boundary velocity should be averaged to 1.0, got {left_vel}"
        );
        assert!(
            (right_vel - 1.0).abs() < 1e-5,
            "right boundary velocity should be averaged to 1.0, got {right_vel}"
        );
    }
}
