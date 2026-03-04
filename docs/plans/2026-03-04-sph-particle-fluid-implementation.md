# SPH Particle-Based Fluid Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace cellular automata fluid simulation with SPH particle-based physics for liquids (water, lava), keeping gases on CA.

**Architecture:** SPH simulation with spatial hashing for neighbor queries. Rendering via screen-space metaballs with pixelization pass. Existing FluidRegistry, events, and detectors adapted; CA cell/simulation/wave/splash modules replaced.

**Tech Stack:** Bevy 0.18, pure Rust SPH (no external crates), custom WGSL shaders.

---

### Task 1: SPH Kernel Functions

**Files:**
- Create: `src/fluid/sph_kernels.rs`

**Step 1: Write failing tests for SPH kernels**

```rust
// src/fluid/sph_kernels.rs — append at bottom

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn poly6_peak_at_zero() {
        let h = 10.0;
        let val = poly6(0.0, h);
        assert!(val > 0.0, "Poly6 at r=0 should be positive");
    }

    #[test]
    fn poly6_zero_at_boundary() {
        let h = 10.0;
        let val = poly6(h, h);
        assert!(val.abs() < 1e-6, "Poly6 at r=h should be ~0");
    }

    #[test]
    fn poly6_zero_beyond_boundary() {
        let h = 10.0;
        let val = poly6(h + 1.0, h);
        assert_eq!(val, 0.0);
    }

    #[test]
    fn spiky_grad_zero_at_zero() {
        let h = 10.0;
        let val = spiky_gradient(0.0, h);
        assert_eq!(val, 0.0, "Spiky gradient at r=0 should be 0");
    }

    #[test]
    fn spiky_grad_negative_inside() {
        let h = 10.0;
        let val = spiky_gradient(5.0, h);
        assert!(val < 0.0, "Spiky gradient should be negative (repulsive)");
    }

    #[test]
    fn viscosity_laplacian_positive_inside() {
        let h = 10.0;
        let val = viscosity_laplacian(5.0, h);
        assert!(val > 0.0, "Viscosity laplacian should be positive inside h");
    }

    #[test]
    fn viscosity_laplacian_zero_beyond() {
        let h = 10.0;
        let val = viscosity_laplacian(h + 1.0, h);
        assert_eq!(val, 0.0);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib fluid::sph_kernels -- --nocapture`
Expected: compilation error (module doesn't exist yet)

**Step 3: Implement SPH kernels**

```rust
// src/fluid/sph_kernels.rs
use std::f32::consts::PI;

/// Poly6 kernel — used for density estimation.
/// W_poly6(r, h) = 315 / (64 * pi * h^9) * (h^2 - r^2)^3   for r <= h
pub fn poly6(r: f32, h: f32) -> f32 {
    if r > h {
        return 0.0;
    }
    let h2 = h * h;
    let r2 = r * r;
    let diff = h2 - r2;
    let coeff = 4.0 / (PI * h.powi(8)); // 2D normalization
    coeff * diff.powi(3)
}

/// Spiky kernel gradient magnitude — used for pressure forces.
/// Returns scalar: multiply by (r_vec / |r|) to get vector.
/// Negative = repulsive (pressure pushes apart).
pub fn spiky_gradient(r: f32, h: f32) -> f32 {
    if r > h || r < 1e-6 {
        return 0.0;
    }
    let diff = h - r;
    let coeff = -10.0 / (PI * h.powi(5)); // 2D normalization
    coeff * diff * diff
}

/// Viscosity kernel Laplacian — used for viscosity forces.
pub fn viscosity_laplacian(r: f32, h: f32) -> f32 {
    if r > h {
        return 0.0;
    }
    let coeff = 40.0 / (PI * h.powi(5)); // 2D normalization
    coeff * (h - r)
}
```

**Step 4: Register module and run tests**

Add `pub mod sph_kernels;` to `src/fluid/mod.rs`.

Run: `cargo test --lib fluid::sph_kernels -- --nocapture`
Expected: all 7 tests pass

**Step 5: Commit**

```bash
git add src/fluid/sph_kernels.rs src/fluid/mod.rs
git commit -m "feat(fluid): add SPH kernel functions (poly6, spiky, viscosity)"
```

---

### Task 2: Spatial Hash Grid

**Files:**
- Create: `src/fluid/spatial_hash.rs`

**Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec2;

    #[test]
    fn empty_grid_returns_no_neighbors() {
        let grid = SpatialHash::new(10.0);
        let neighbors = grid.query(Vec2::ZERO);
        assert!(neighbors.is_empty());
    }

    #[test]
    fn insert_and_find_self() {
        let mut grid = SpatialHash::new(10.0);
        grid.insert(0, Vec2::new(5.0, 5.0));
        let neighbors = grid.query(Vec2::new(5.0, 5.0));
        assert!(neighbors.contains(&0));
    }

    #[test]
    fn find_neighbor_in_adjacent_cell() {
        let mut grid = SpatialHash::new(10.0);
        grid.insert(0, Vec2::new(9.0, 5.0));  // near cell boundary
        grid.insert(1, Vec2::new(11.0, 5.0)); // just across boundary
        let neighbors = grid.query(Vec2::new(9.0, 5.0));
        assert!(neighbors.contains(&0));
        assert!(neighbors.contains(&1));
    }

    #[test]
    fn far_particle_not_found() {
        let mut grid = SpatialHash::new(10.0);
        grid.insert(0, Vec2::new(0.0, 0.0));
        grid.insert(1, Vec2::new(100.0, 100.0));
        let neighbors = grid.query(Vec2::new(0.0, 0.0));
        assert!(neighbors.contains(&0));
        assert!(!neighbors.contains(&1));
    }

    #[test]
    fn clear_removes_all() {
        let mut grid = SpatialHash::new(10.0);
        grid.insert(0, Vec2::new(5.0, 5.0));
        grid.clear();
        let neighbors = grid.query(Vec2::new(5.0, 5.0));
        assert!(neighbors.is_empty());
    }

    #[test]
    fn build_from_positions() {
        let positions = vec![Vec2::new(1.0, 1.0), Vec2::new(2.0, 2.0), Vec2::new(100.0, 100.0)];
        let grid = SpatialHash::from_positions(&positions, 10.0);
        let near = grid.query(Vec2::new(1.5, 1.5));
        assert!(near.contains(&0));
        assert!(near.contains(&1));
        assert!(!near.contains(&2));
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib fluid::spatial_hash -- --nocapture`
Expected: compilation error

**Step 3: Implement spatial hash**

```rust
// src/fluid/spatial_hash.rs
use bevy::math::Vec2;
use std::collections::HashMap;

/// Grid-based spatial hash for O(1) neighbor lookups.
pub struct SpatialHash {
    cell_size: f32,
    inv_cell_size: f32,
    cells: HashMap<(i32, i32), Vec<usize>>,
}

impl SpatialHash {
    pub fn new(cell_size: f32) -> Self {
        Self {
            cell_size,
            inv_cell_size: 1.0 / cell_size,
            cells: HashMap::new(),
        }
    }

    pub fn from_positions(positions: &[Vec2], cell_size: f32) -> Self {
        let mut grid = Self::new(cell_size);
        for (i, pos) in positions.iter().enumerate() {
            grid.insert(i, *pos);
        }
        grid
    }

    fn cell_coord(&self, pos: Vec2) -> (i32, i32) {
        (
            (pos.x * self.inv_cell_size).floor() as i32,
            (pos.y * self.inv_cell_size).floor() as i32,
        )
    }

    pub fn insert(&mut self, index: usize, pos: Vec2) {
        let coord = self.cell_coord(pos);
        self.cells.entry(coord).or_default().push(index);
    }

    /// Returns indices of all particles in the 3x3 neighborhood of the given position.
    pub fn query(&self, pos: Vec2) -> Vec<usize> {
        let (cx, cy) = self.cell_coord(pos);
        let mut result = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(indices) = self.cells.get(&(cx + dx, cy + dy)) {
                    result.extend_from_slice(indices);
                }
            }
        }
        result
    }

    /// Returns a reference to indices in a specific cell (for iteration without allocation).
    pub fn cell(&self, cx: i32, cy: i32) -> &[usize] {
        self.cells.get(&(cx, cy)).map_or(&[], |v| v.as_slice())
    }

    pub fn clear(&mut self) {
        self.cells.clear();
    }
}
```

**Step 4: Register module and run tests**

Add `pub mod spatial_hash;` to `src/fluid/mod.rs`.

Run: `cargo test --lib fluid::spatial_hash -- --nocapture`
Expected: all 6 tests pass

**Step 5: Commit**

```bash
git add src/fluid/spatial_hash.rs src/fluid/mod.rs
git commit -m "feat(fluid): add spatial hash grid for SPH neighbor queries"
```

---

### Task 3: SPH Particle Data + Storage

**Files:**
- Create: `src/fluid/sph_particle.rs`

**Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec2;
    use crate::fluid::cell::FluidId;

    #[test]
    fn new_particle_at_rest() {
        let p = Particle::new(Vec2::new(10.0, 20.0), FluidId(1), 1.0);
        assert_eq!(p.position, Vec2::new(10.0, 20.0));
        assert_eq!(p.velocity, Vec2::ZERO);
        assert_eq!(p.fluid_id, FluidId(1));
        assert_eq!(p.mass, 1.0);
    }

    #[test]
    fn particle_store_add_and_count() {
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::ZERO, FluidId(1), 1.0));
        store.add(Particle::new(Vec2::ONE, FluidId(1), 1.0));
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn particle_store_remove_by_swap() {
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(1.0, 0.0), FluidId(1), 1.0));
        store.add(Particle::new(Vec2::new(2.0, 0.0), FluidId(1), 1.0));
        store.add(Particle::new(Vec2::new(3.0, 0.0), FluidId(1), 1.0));
        store.remove_swap(0);
        assert_eq!(store.len(), 2);
        // Last element was swapped into index 0
        assert_eq!(store.positions[0], Vec2::new(3.0, 0.0));
    }

    #[test]
    fn particle_store_clear() {
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::ZERO, FluidId(1), 1.0));
        store.clear();
        assert_eq!(store.len(), 0);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib fluid::sph_particle -- --nocapture`
Expected: compilation error

**Step 3: Implement particle storage (SoA layout for cache efficiency)**

```rust
// src/fluid/sph_particle.rs
use bevy::math::Vec2;
use bevy::prelude::Resource;
use crate::fluid::cell::FluidId;

/// Convenience struct for creating particles. Not stored directly.
pub struct Particle {
    pub position: Vec2,
    pub velocity: Vec2,
    pub fluid_id: FluidId,
    pub mass: f32,
}

impl Particle {
    pub fn new(position: Vec2, fluid_id: FluidId, mass: f32) -> Self {
        Self {
            position,
            velocity: Vec2::ZERO,
            fluid_id,
            mass,
        }
    }
}

/// SoA (Structure of Arrays) particle storage for cache-friendly SPH computation.
#[derive(Resource, Default)]
pub struct ParticleStore {
    pub positions: Vec<Vec2>,
    pub velocities: Vec<Vec2>,
    pub densities: Vec<f32>,
    pub pressures: Vec<f32>,
    pub forces: Vec<Vec2>,
    pub fluid_ids: Vec<FluidId>,
    pub masses: Vec<f32>,
}

impl ParticleStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            positions: Vec::with_capacity(cap),
            velocities: Vec::with_capacity(cap),
            densities: Vec::with_capacity(cap),
            pressures: Vec::with_capacity(cap),
            forces: Vec::with_capacity(cap),
            fluid_ids: Vec::with_capacity(cap),
            masses: Vec::with_capacity(cap),
        }
    }

    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    pub fn add(&mut self, p: Particle) {
        self.positions.push(p.position);
        self.velocities.push(p.velocity);
        self.densities.push(0.0);
        self.pressures.push(0.0);
        self.forces.push(Vec2::ZERO);
        self.fluid_ids.push(p.fluid_id);
        self.masses.push(p.mass);
    }

    /// Remove particle at index by swapping with last. O(1).
    pub fn remove_swap(&mut self, index: usize) {
        self.positions.swap_remove(index);
        self.velocities.swap_remove(index);
        self.densities.swap_remove(index);
        self.pressures.swap_remove(index);
        self.forces.swap_remove(index);
        self.fluid_ids.swap_remove(index);
        self.masses.swap_remove(index);
    }

    pub fn clear(&mut self) {
        self.positions.clear();
        self.velocities.clear();
        self.densities.clear();
        self.pressures.clear();
        self.forces.clear();
        self.fluid_ids.clear();
        self.masses.clear();
    }
}
```

**Step 4: Register module and run tests**

Add `pub mod sph_particle;` to `src/fluid/mod.rs`.

Run: `cargo test --lib fluid::sph_particle -- --nocapture`
Expected: all 4 tests pass

**Step 5: Commit**

```bash
git add src/fluid/sph_particle.rs src/fluid/mod.rs
git commit -m "feat(fluid): add SoA particle storage for SPH simulation"
```

---

### Task 4: SPH Simulation Core

**Files:**
- Create: `src/fluid/sph_simulation.rs`
- Modify: `src/fluid/mod.rs`

**Step 1: Write failing tests for density and pressure**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::cell::FluidId;
    use crate::fluid::sph_particle::{Particle, ParticleStore};
    use bevy::math::Vec2;

    fn test_config() -> SphConfig {
        SphConfig {
            smoothing_radius: 16.0,
            rest_density: 1.0,
            stiffness: 50.0,
            viscosity: 0.1,
            gravity: Vec2::new(0.0, -98.0),
            particle_mass: 1.0,
        }
    }

    fn two_particles_store(dist: f32) -> ParticleStore {
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(0.0, 0.0), FluidId(1), 1.0));
        store.add(Particle::new(Vec2::new(dist, 0.0), FluidId(1), 1.0));
        store
    }

    #[test]
    fn density_increases_with_proximity() {
        let config = test_config();
        let mut close = two_particles_store(4.0);
        let mut far = two_particles_store(12.0);

        compute_density_pressure(&mut close, &config);
        compute_density_pressure(&mut far, &config);

        assert!(
            close.densities[0] > far.densities[0],
            "Closer particles should have higher density: {} vs {}",
            close.densities[0], far.densities[0]
        );
    }

    #[test]
    fn pressure_positive_when_compressed() {
        let config = test_config();
        let mut store = two_particles_store(2.0);
        compute_density_pressure(&mut store, &config);
        // Density should exceed rest_density => pressure > 0
        assert!(store.pressures[0] > 0.0, "Pressure should be positive when compressed");
    }

    #[test]
    fn gravity_pulls_down() {
        let config = test_config();
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(0.0, 100.0), FluidId(1), 1.0));

        compute_density_pressure(&mut store, &config);
        compute_forces(&mut store, &config);

        assert!(store.forces[0].y < 0.0, "Gravity should pull down");
    }

    #[test]
    fn pressure_pushes_apart() {
        let config = test_config();
        let mut store = two_particles_store(4.0);

        compute_density_pressure(&mut store, &config);
        compute_forces(&mut store, &config);

        // Particle 0 at x=0 should be pushed left (negative x) by particle 1
        // Particle 1 at x=4 should be pushed right (positive x) by particle 0
        // (Ignoring gravity for the x-component)
        assert!(store.forces[0].x < 0.0, "Left particle should be pushed left: {}", store.forces[0].x);
        assert!(store.forces[1].x > 0.0, "Right particle should be pushed right: {}", store.forces[1].x);
    }

    #[test]
    fn integrate_moves_particle() {
        let config = test_config();
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(0.0, 100.0), FluidId(1), 1.0));

        compute_density_pressure(&mut store, &config);
        compute_forces(&mut store, &config);
        integrate(&mut store, 1.0 / 60.0);

        // Should have moved down due to gravity
        assert!(store.positions[0].y < 100.0, "Particle should fall");
        assert!(store.velocities[0].y < 0.0, "Velocity should be downward");
    }

    #[test]
    fn step_full_cycle() {
        let config = test_config();
        let mut store = two_particles_store(8.0);
        let initial_y = store.positions[0].y;

        sph_step(&mut store, &config, 1.0 / 60.0);

        // After one step, particles should have moved (at least due to gravity)
        assert_ne!(store.positions[0].y, initial_y);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib fluid::sph_simulation -- --nocapture`
Expected: compilation error

**Step 3: Implement SPH simulation**

```rust
// src/fluid/sph_simulation.rs
use bevy::math::Vec2;
use bevy::prelude::Resource;

use crate::fluid::spatial_hash::SpatialHash;
use crate::fluid::sph_kernels::{poly6, spiky_gradient, viscosity_laplacian};
use crate::fluid::sph_particle::ParticleStore;

/// SPH simulation parameters.
#[derive(Debug, Clone, Resource)]
pub struct SphConfig {
    /// Smoothing radius (h). Determines neighbor search distance.
    pub smoothing_radius: f32,
    /// Target density at rest.
    pub rest_density: f32,
    /// Pressure stiffness coefficient (k). Higher = less compressible.
    pub stiffness: f32,
    /// Viscosity coefficient (mu). Higher = thicker fluid.
    pub viscosity: f32,
    /// Gravity vector.
    pub gravity: Vec2,
    /// Default mass per particle.
    pub particle_mass: f32,
}

impl Default for SphConfig {
    fn default() -> Self {
        Self {
            smoothing_radius: 16.0,
            rest_density: 1.0,
            stiffness: 50.0,
            viscosity: 0.1,
            gravity: Vec2::new(0.0, -98.0),
            particle_mass: 1.0,
        }
    }
}

/// Compute density and pressure for all particles.
pub fn compute_density_pressure(store: &mut ParticleStore, config: &SphConfig) {
    let h = config.smoothing_radius;
    let grid = SpatialHash::from_positions(&store.positions, h);

    for i in 0..store.len() {
        let pos_i = store.positions[i];
        let mut density = 0.0f32;

        for &j in &grid.query(pos_i) {
            let r = pos_i.distance(store.positions[j]);
            density += store.masses[j] * poly6(r, h);
        }

        store.densities[i] = density;
        // Equation of state: P = k * (rho - rho_0)
        store.pressures[i] = config.stiffness * (density - config.rest_density);
    }
}

/// Compute forces: pressure + viscosity + gravity.
pub fn compute_forces(store: &mut ParticleStore, config: &SphConfig) {
    let h = config.smoothing_radius;
    let grid = SpatialHash::from_positions(&store.positions, h);

    for i in 0..store.len() {
        let pos_i = store.positions[i];
        let vel_i = store.velocities[i];
        let pressure_i = store.pressures[i];
        let density_i = store.densities[i];

        let mut f_pressure = Vec2::ZERO;
        let mut f_viscosity = Vec2::ZERO;

        for &j in &grid.query(pos_i) {
            if i == j {
                continue;
            }

            let pos_j = store.positions[j];
            let diff = pos_i - pos_j;
            let r = diff.length();

            if r < 1e-6 || r > h {
                continue;
            }

            let dir = diff / r;
            let density_j = store.densities[j];

            // Pressure force (symmetric: average of both pressures)
            if density_j > 1e-6 {
                let pressure_j = store.pressures[j];
                let pressure_avg = (pressure_i + pressure_j) * 0.5;
                f_pressure += dir * store.masses[j] * pressure_avg / density_j
                    * spiky_gradient(r, h);
            }

            // Viscosity force
            if density_j > 1e-6 {
                let vel_j = store.velocities[j];
                f_viscosity += (vel_j - vel_i) * store.masses[j] / density_j
                    * viscosity_laplacian(r, h);
            }
        }

        f_viscosity *= config.viscosity;

        // Gravity (force = mass * g, but we apply f/density later, so store mass*g)
        let f_gravity = config.gravity * density_i;

        store.forces[i] = f_pressure + f_viscosity + f_gravity;
    }
}

/// Symplectic Euler integration.
pub fn integrate(store: &mut ParticleStore, dt: f32) {
    for i in 0..store.len() {
        let density = store.densities[i].max(1e-6);
        let acceleration = store.forces[i] / density;
        store.velocities[i] += acceleration * dt;
        store.positions[i] += store.velocities[i] * dt;
    }
}

/// One full SPH simulation step.
pub fn sph_step(store: &mut ParticleStore, config: &SphConfig, dt: f32) {
    if store.is_empty() {
        return;
    }
    compute_density_pressure(store, config);
    compute_forces(store, config);
    integrate(store, dt);
}
```

**Step 4: Register module and run tests**

Add `pub mod sph_simulation;` to `src/fluid/mod.rs`.

Run: `cargo test --lib fluid::sph_simulation -- --nocapture`
Expected: all 7 tests pass

**Step 5: Commit**

```bash
git add src/fluid/sph_simulation.rs src/fluid/mod.rs
git commit -m "feat(fluid): implement SPH simulation core (density, pressure, forces, integration)"
```

---

### Task 5: Tile Collision for Particles

**Files:**
- Create: `src/fluid/sph_collision.rs`

**Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec2;

    #[test]
    fn particle_above_floor_unaffected() {
        let mut pos = Vec2::new(50.0, 50.0);
        let mut vel = Vec2::new(0.0, -10.0);
        let is_solid = |_x: i32, _y: i32| -> bool { false };
        resolve_tile_collision(&mut pos, &mut vel, 8.0, &is_solid, 0.3);
        // No solid tiles, position unchanged
        assert_eq!(pos, Vec2::new(50.0, 50.0));
    }

    #[test]
    fn particle_in_solid_pushed_out() {
        let mut pos = Vec2::new(12.0, 12.0); // Inside tile (1,1)
        let mut vel = Vec2::new(0.0, -10.0);
        // Tile (1,1) is solid, rest are not
        let is_solid = |x: i32, y: i32| -> bool { x == 1 && y == 1 };
        resolve_tile_collision(&mut pos, &mut vel, 8.0, &is_solid, 0.3);
        // Should be pushed out to nearest edge
        let tx = (pos.x / 8.0).floor() as i32;
        let ty = (pos.y / 8.0).floor() as i32;
        assert!(
            !(tx == 1 && ty == 1),
            "Particle should be outside solid tile, got ({}, {})",
            tx, ty
        );
    }

    #[test]
    fn velocity_reflected_on_collision() {
        let mut pos = Vec2::new(12.0, 12.0);
        let mut vel = Vec2::new(0.0, -50.0);
        let is_solid = |x: i32, y: i32| -> bool { x == 1 && y == 1 };
        resolve_tile_collision(&mut pos, &mut vel, 8.0, &is_solid, 0.3);
        // Velocity should be dampened
        assert!(vel.length() < 50.0, "Velocity should be reduced after collision");
    }

    #[test]
    fn world_boundary_bottom() {
        let mut pos = Vec2::new(50.0, -5.0);
        let mut vel = Vec2::new(0.0, -10.0);
        enforce_world_bounds(&mut pos, &mut vel, 0.0, 1000.0, 0.0, 500.0);
        assert!(pos.y >= 0.0);
        assert!(vel.y >= 0.0); // Reflected
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib fluid::sph_collision -- --nocapture`
Expected: compilation error

**Step 3: Implement collision**

```rust
// src/fluid/sph_collision.rs
use bevy::math::Vec2;

/// Resolve collision of a single particle with the tile grid.
/// `is_solid` takes tile coordinates (gx, gy) and returns true if solid.
pub fn resolve_tile_collision(
    pos: &mut Vec2,
    vel: &mut Vec2,
    tile_size: f32,
    is_solid: &dyn Fn(i32, i32) -> bool,
    restitution: f32,
) {
    let tx = (pos.x / tile_size).floor() as i32;
    let ty = (pos.y / tile_size).floor() as i32;

    if !is_solid(tx, ty) {
        return;
    }

    // Find nearest non-solid edge
    let tile_left = tx as f32 * tile_size;
    let tile_right = tile_left + tile_size;
    let tile_bottom = ty as f32 * tile_size;
    let tile_top = tile_bottom + tile_size;

    let dist_left = (pos.x - tile_left).abs();
    let dist_right = (tile_right - pos.x).abs();
    let dist_bottom = (pos.y - tile_bottom).abs();
    let dist_top = (tile_top - pos.y).abs();

    let min_dist = dist_left.min(dist_right).min(dist_bottom).min(dist_top);

    if min_dist == dist_left && !is_solid(tx - 1, ty) {
        pos.x = tile_left - 0.01;
        vel.x = -vel.x.abs() * restitution;
    } else if min_dist == dist_right && !is_solid(tx + 1, ty) {
        pos.x = tile_right + 0.01;
        vel.x = vel.x.abs() * restitution;
    } else if min_dist == dist_bottom && !is_solid(tx, ty - 1) {
        pos.y = tile_bottom - 0.01;
        vel.y = -vel.y.abs() * restitution;
    } else if min_dist == dist_top && !is_solid(tx, ty + 1) {
        pos.y = tile_top + 0.01;
        vel.y = vel.y.abs() * restitution;
    } else {
        // Surrounded by solids — zero velocity, push to nearest edge anyway
        pos.x = tile_left - 0.01;
        *vel = Vec2::ZERO;
    }
}

/// Clamp particle to world boundaries.
pub fn enforce_world_bounds(
    pos: &mut Vec2,
    vel: &mut Vec2,
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
) {
    if pos.x < min_x {
        pos.x = min_x;
        vel.x = vel.x.abs() * 0.3;
    } else if pos.x > max_x {
        pos.x = max_x;
        vel.x = -vel.x.abs() * 0.3;
    }
    if pos.y < min_y {
        pos.y = min_y;
        vel.y = vel.y.abs() * 0.3;
    } else if pos.y > max_y {
        pos.y = max_y;
        vel.y = -vel.y.abs() * 0.3;
    }
}
```

**Step 4: Register module and run tests**

Add `pub mod sph_collision;` to `src/fluid/mod.rs`.

Run: `cargo test --lib fluid::sph_collision -- --nocapture`
Expected: all 4 tests pass

**Step 5: Commit**

```bash
git add src/fluid/sph_collision.rs src/fluid/mod.rs
git commit -m "feat(fluid): add tile collision and world bounds for SPH particles"
```

---

### Task 6: Wire SPH into Bevy Systems

**Files:**
- Modify: `src/fluid/mod.rs` (plugin registration)
- Modify: `src/fluid/systems.rs` (add SPH system functions)

**Step 1: Add SPH config and particle store as resources**

In `src/fluid/mod.rs`, update `FluidPlugin::build`:
- Add `.init_resource::<sph_particle::ParticleStore>()`
- Add `.init_resource::<sph_simulation::SphConfig>()`
- Add new system `systems::sph_fluid_simulation` in the chain replacing `systems::fluid_simulation`

**Step 2: Create `sph_fluid_simulation` system in `systems.rs`**

```rust
/// SPH-based fluid simulation system (replaces CA `fluid_simulation`).
pub fn sph_fluid_simulation(
    time: Res<Time>,
    config: Res<SphConfig>,
    mut accumulator: ResMut<FluidTickAccumulator>,
    mut particles: ResMut<ParticleStore>,
    world_map: ResMut<WorldMap>,
    active_world: Res<ActiveWorld>,
) {
    let dt = 1.0 / 60.0; // Fixed timestep
    accumulator.accumulator += time.delta_secs();

    let max_ticks = 4u32;
    let mut ticks = 0u32;

    while accumulator.accumulator >= dt && ticks < max_ticks {
        accumulator.accumulator -= dt;
        ticks += 1;

        sph_step(&mut particles, &config, dt);

        // Tile collisions
        let tile_size = active_world.tile_size;
        for i in 0..particles.len() {
            resolve_tile_collision(
                &mut particles.positions[i],
                &mut particles.velocities[i],
                tile_size,
                &|gx, gy| world_map.is_solid(gx, gy),
                0.2,
            );
        }
    }
}
```

**Step 3: Update system chain in mod.rs**

Replace `systems::fluid_simulation` with `systems::sph_fluid_simulation` in the `.chain()`.
Keep detectors, rebuild_meshes for now (they'll be updated in later tasks).

**Step 4: Verify it compiles**

Run: `cargo check`
Expected: compiles (may have warnings about unused CA code)

**Step 5: Commit**

```bash
git add src/fluid/mod.rs src/fluid/systems.rs
git commit -m "feat(fluid): wire SPH simulation into Bevy system chain"
```

---

### Task 7: SPH Particle Rendering (Metaballs Shader)

**Files:**
- Create: `src/fluid/sph_render.rs`
- Modify: `assets/engine/shaders/fluid.wgsl`
- Modify: `src/fluid/systems.rs` (mesh rebuild)

**Step 1: Create particle mesh builder**

```rust
// src/fluid/sph_render.rs
use bevy::asset::RenderAssetUsages;
use bevy::math::Vec2;
use bevy::prelude::*;
use bevy::render::mesh::Mesh;

use crate::fluid::cell::FluidId;
use crate::fluid::registry::FluidRegistry;
use crate::fluid::sph_particle::ParticleStore;

/// Z-depth for fluid particles (between tiles z=0 and entities).
pub const FLUID_Z: f32 = 0.5;

/// Build a mesh of quads for all particles in a given chunk region.
/// Each particle becomes a screen-aligned quad (2 triangles, 6 vertices).
pub fn build_particle_mesh(
    particles: &ParticleStore,
    chunk_world_min: Vec2,
    chunk_world_max: Vec2,
    particle_radius: f32,
    registry: &FluidRegistry,
) -> Option<Mesh> {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();

    let margin = particle_radius * 2.0;
    let min = chunk_world_min - Vec2::splat(margin);
    let max = chunk_world_max + Vec2::splat(margin);

    for i in 0..particles.len() {
        let pos = particles.positions[i];
        if pos.x < min.x || pos.x > max.x || pos.y < min.y || pos.y > max.y {
            continue;
        }

        let fid = particles.fluid_ids[i];
        if fid == FluidId::NONE {
            continue;
        }

        let def = registry.get(fid);
        let color = [
            def.color[0] as f32 / 255.0,
            def.color[1] as f32 / 255.0,
            def.color[2] as f32 / 255.0,
            def.color[3] as f32 / 255.0,
        ];

        let r = particle_radius;
        // Quad corners: bottom-left, bottom-right, top-right, top-left
        let corners = [
            ([-r, -r], [0.0, 0.0]),
            ([r, -r], [1.0, 0.0]),
            ([r, r], [1.0, 1.0]),
            ([-r, r], [0.0, 1.0]),
        ];
        // Two triangles: 0-1-2, 0-2-3
        let indices = [0, 1, 2, 0, 2, 3];

        for &idx in &indices {
            let (offset, uv) = corners[idx];
            positions.push([pos.x + offset[0], pos.y + offset[1], FLUID_Z]);
            colors.push(color);
            uvs.push(uv);
        }
    }

    if positions.is_empty() {
        return None;
    }

    let mut mesh = Mesh::new(
        bevy::render::render_resource::PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);

    Some(mesh)
}
```

**Step 2: Update shader for circle SDF rendering**

Update `assets/engine/shaders/fluid.wgsl` vertex and fragment to render particle quads as soft circles. The UV goes from [0,0] to [1,1] per quad — fragment uses `distance(uv, vec2(0.5))` for SDF.

**Step 3: Update `fluid_rebuild_meshes` in systems.rs**

Replace the cell-based mesh building with calls to `build_particle_mesh()`.

**Step 4: Verify rendering works**

Run: `cargo run`
Expected: particles visible as soft circles, falling under gravity

**Step 5: Commit**

```bash
git add src/fluid/sph_render.rs src/fluid/systems.rs src/fluid/mod.rs assets/engine/shaders/fluid.wgsl
git commit -m "feat(fluid): particle mesh rendering with circle SDF shader"
```

---

### Task 8: Adapt Entity-Fluid Detection

**Files:**
- Modify: `src/fluid/detectors.rs`

**Step 1: Add spatial hash query for entity-particle proximity**

Replace tile-based fluid lookup with proximity check against particle store using the spatial hash. Entity is "in fluid" if any particle is within `smoothing_radius` of entity position.

**Step 2: Update `detect_entity_water_entry`**

Change from `world_map.fluids_at(tile_x, tile_y)` to spatial query on `ParticleStore`.

**Step 3: Update `detect_entity_swimming` and `detect_projectile_in_fluid`**

Same pattern — use spatial hash proximity query.

**Step 4: Test entity detection works in-game**

Run: `cargo run`
Expected: splash events fire when player enters water particles

**Step 5: Commit**

```bash
git add src/fluid/detectors.rs
git commit -m "feat(fluid): adapt entity-fluid detection for SPH particles"
```

---

### Task 9: Adapt Fluid Reactions for SPH

**Files:**
- Modify: `src/fluid/reactions.rs`

**Step 1: Create particle-based reaction check**

Replace cell-adjacency reaction logic with particle-proximity check. When two particles of different fluid types are within `smoothing_radius`, check reaction registry. If reaction found, remove both particles and place result tile/fluid.

**Step 2: Remove density displacement pass**

`resolve_density_displacement_global()` is no longer needed — SPH pressure handles buoyancy.

**Step 3: Test reactions**

Add water particles near lava particles, verify reaction events fire.

**Step 4: Commit**

```bash
git add src/fluid/reactions.rs
git commit -m "feat(fluid): adapt fluid reactions for SPH particle proximity"
```

---

### Task 10: Cleanup CA Code + Gas Separation

**Files:**
- Modify: `src/fluid/mod.rs`
- Modify: `src/fluid/systems.rs`

**Step 1: Separate gas CA simulation**

Move gas-only CA logic into a separate system `gas_ca_simulation` that only processes `is_gas == true` fluids. This keeps steam/toxic_gas/smoke on the old CA while liquids use SPH.

**Step 2: Remove unused CA imports and dead code**

Remove references to `FluidCell`, `FluidSlot`, old `simulate_tick` for liquids. Keep `cell.rs` for gas CA.

**Step 3: Remove wave.rs and splash.rs from system chain**

SPH handles waves and splashes naturally through particle dynamics.

**Step 4: Verify everything compiles and runs**

Run: `cargo test && cargo run`
Expected: all tests pass, game runs with SPH water + CA gases

**Step 5: Commit**

```bash
git add src/fluid/mod.rs src/fluid/systems.rs
git commit -m "refactor(fluid): separate gas CA from SPH liquids, remove unused wave/splash"
```

---

### Task 11: Debug Overlay for SPH

**Files:**
- Modify: `src/fluid/debug_overlay.rs`
- Modify: `src/fluid/debug.rs`

**Step 1: Add SPH debug info to overlay**

Show particle count, average density, average pressure, FPS impact.

**Step 2: Update debug fluid placement**

`debug_place_fluid` should spawn SPH particles instead of writing CA cells (for liquid types).

**Step 3: Commit**

```bash
git add src/fluid/debug_overlay.rs src/fluid/debug.rs
git commit -m "feat(fluid): update debug overlay and placement for SPH particles"
```

---

### Task 12: Performance Tuning + Final Polish

**Files:**
- Modify: `src/fluid/sph_simulation.rs`
- Modify: `src/fluid/spatial_hash.rs`

**Step 1: Optimize spatial hash rebuild**

Pre-allocate capacity, reuse allocations between frames. Use `clear()` + re-insert instead of `new()`.

**Step 2: Add particle sleep optimization**

Particles with `velocity.length() < epsilon` for N frames skip force computation.

**Step 3: Benchmark**

Run with 10K, 20K, 50K particles and measure frame times.

**Step 4: Commit**

```bash
git add src/fluid/sph_simulation.rs src/fluid/spatial_hash.rs
git commit -m "perf(fluid): optimize SPH spatial hash and add particle sleep"
```
