//! Position Based Fluids (PBF) simulation.
//!
//! Based on Macklin & Müller 2013 "Position Based Fluids".
//! Instead of computing forces and integrating, we solve density constraints
//! directly at the position level. This is unconditionally stable.
//!
//! Algorithm:
//! 1. Apply gravity to velocities, predict new positions
//! 2. Apply boundary constraints to predicted positions (tile collisions)
//! 3. For N solver iterations:
//!    a. Compute density at predicted positions
//!    b. Compute λ (Lagrange multiplier) from density constraint violation
//!    c. Compute position correction Δp from λ values
//!    d. Apply corrections, then re-apply boundary constraints
//! 4. Update velocities from position change
//! 5. Apply XSPH viscosity smoothing

use bevy::math::Vec2;
use bevy::prelude::Resource;

use crate::fluid::spatial_hash::SpatialHash;
use crate::fluid::sph_kernels::{poly6, spiky_gradient};
use crate::fluid::sph_particle::ParticleStore;

#[derive(Debug, Clone, Resource)]
pub struct SphConfig {
    pub smoothing_radius: f32,
    pub rest_density: f32,
    /// Number of constraint solver iterations (4-6 typical).
    pub solver_iterations: u32,
    /// Constraint Force Mixing — relaxation parameter in λ denominator.
    /// Higher = softer/more stable, lower = stiffer.
    pub epsilon: f32,
    pub viscosity: f32,
    pub gravity: Vec2,
    pub particle_mass: f32,
    /// Artificial pressure strength (prevents particle clumping at surface).
    pub surface_tension_k: f32,
}

impl Default for SphConfig {
    fn default() -> Self {
        Self {
            smoothing_radius: 16.0,
            rest_density: 0.018,
            solver_iterations: 4,
            epsilon: 0.05,
            viscosity: 0.01,
            gravity: Vec2::new(0.0, -200.0),
            particle_mass: 1.0,
            surface_tension_k: 0.1,
        }
    }
}

/// One PBF simulation step.
///
/// `boundary_fn` is called on each particle's (position, velocity) to enforce
/// tile collisions and world bounds. It is applied to predicted positions before
/// and during solver iterations to prevent particles from going through walls.
pub fn sph_step(
    store: &mut ParticleStore,
    config: &SphConfig,
    dt: f32,
    boundary_fn: &dyn Fn(&mut Vec2, &mut Vec2),
) {
    if store.is_empty() {
        return;
    }
    let n = store.len();
    let h = config.smoothing_radius;
    let mass = config.particle_mass;
    let rho0 = config.rest_density.max(1e-8);

    // --- Step 1: Predict positions ---
    let mut predicted = Vec::with_capacity(n);
    let mut pred_vel = Vec::with_capacity(n);
    for i in 0..n {
        let mut v = store.velocities[i] + config.gravity * dt;
        let mut p = store.positions[i] + v * dt;
        // Apply boundaries to predicted positions
        boundary_fn(&mut p, &mut v);
        predicted.push(p);
        pred_vel.push(v);
    }

    // Buffers for solver
    let mut lambdas = vec![0.0f32; n];
    let mut neighbors_buf = Vec::new();

    // --- Step 2: Solver iterations ---
    for _ in 0..config.solver_iterations {
        let grid = SpatialHash::from_positions(&predicted, h);

        // 2a: Compute density and lambda for each particle
        for i in 0..n {
            let pos_i = predicted[i];
            grid.query_into(pos_i, &mut neighbors_buf);

            // Compute density at predicted position
            let mut density = 0.0f32;
            for &j in &neighbors_buf {
                let r = pos_i.distance(predicted[j]);
                density += mass * poly6(r, h);
            }
            store.densities[i] = density;

            // Density constraint: C_i = ρ_i/ρ₀ - 1
            let constraint = density / rho0 - 1.0;

            // Only correct if over-dense (compressed). Skip if under-dense
            // to allow free surfaces without attracting particles together.
            if constraint <= 0.0 {
                lambdas[i] = 0.0;
                continue;
            }

            // Compute gradient denominator: Σ_k |∇_{p_k} C_i|²
            let mut sum_grad_sq = 0.0f32;
            let mut grad_i = Vec2::ZERO;

            for &j in &neighbors_buf {
                if j == i {
                    continue;
                }
                let diff = pos_i - predicted[j];
                let r = diff.length();
                if r < 1e-6 || r > h {
                    continue;
                }
                let dir = diff / r;
                let grad_w = spiky_gradient(r, h) * dir;
                let grad_j = -(mass / rho0) * grad_w;
                sum_grad_sq += grad_j.length_squared();
                grad_i += (mass / rho0) * grad_w;
            }
            sum_grad_sq += grad_i.length_squared();

            // λ_i = -C_i / (Σ|∇C|² + ε)
            lambdas[i] = -constraint / (sum_grad_sq + config.epsilon);
        }

        // 2b: Compute and apply position corrections
        for i in 0..n {
            let pos_i = predicted[i];
            grid.query_into(pos_i, &mut neighbors_buf);

            let mut delta_p = Vec2::ZERO;
            for &j in &neighbors_buf {
                if j == i {
                    continue;
                }
                let diff = pos_i - predicted[j];
                let r = diff.length();
                if r < 1e-6 || r > h {
                    continue;
                }
                let dir = diff / r;
                let grad_w = spiky_gradient(r, h) * dir;

                // Artificial pressure (tensile instability correction)
                let s_corr = if config.surface_tension_k > 0.0 {
                    let w_dq = poly6(0.3 * h, h);
                    if w_dq > 1e-10 {
                        let w_ratio = poly6(r, h) / w_dq;
                        -config.surface_tension_k * w_ratio.powi(4)
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };

                delta_p += (1.0 / rho0) * (lambdas[i] + lambdas[j] + s_corr) * grad_w;
            }
            predicted[i] += delta_p;

            // Re-apply boundary after correction
            boundary_fn(&mut predicted[i], &mut pred_vel[i]);
        }
    }

    // --- Step 3: Update velocities and positions ---
    let inv_dt = if dt > 1e-8 { 1.0 / dt } else { 0.0 };
    for i in 0..n {
        store.velocities[i] = (predicted[i] - store.positions[i]) * inv_dt;
        store.positions[i] = predicted[i];
    }

    // --- Step 4: XSPH viscosity smoothing ---
    if config.viscosity > 0.0 {
        let grid = SpatialHash::from_positions(&store.positions, h);
        let mut corrections: Vec<Vec2> = vec![Vec2::ZERO; n];
        for i in 0..n {
            let pos_i = store.positions[i];
            let vel_i = store.velocities[i];
            grid.query_into(pos_i, &mut neighbors_buf);
            let mut correction = Vec2::ZERO;
            for &j in &neighbors_buf {
                if j == i {
                    continue;
                }
                let r = pos_i.distance(store.positions[j]);
                if r > h {
                    continue;
                }
                correction += (store.velocities[j] - vel_i) * poly6(r, h);
            }
            corrections[i] = correction * config.viscosity;
        }
        for i in 0..n {
            store.velocities[i] += corrections[i];
        }
    }

    // Recompute densities/pressures for debug display
    {
        let grid = SpatialHash::from_positions(&store.positions, h);
        for i in 0..n {
            let pos_i = store.positions[i];
            grid.query_into(pos_i, &mut neighbors_buf);
            let mut density = 0.0f32;
            for &j in &neighbors_buf {
                let r = pos_i.distance(store.positions[j]);
                density += mass * poly6(r, h);
            }
            store.densities[i] = density;
            // Store constraint value for debug (0 = at rest, >0 = compressed)
            store.pressures[i] = density / rho0 - 1.0;
        }
    }

    store.mark_changed();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::cell::FluidId;
    use crate::fluid::sph_particle::{Particle, ParticleStore};
    use bevy::math::Vec2;

    fn test_config() -> SphConfig {
        SphConfig {
            smoothing_radius: 16.0,
            rest_density: 0.018,
            solver_iterations: 4,
            epsilon: 0.05,
            viscosity: 0.01,
            gravity: Vec2::new(0.0, -98.0),
            particle_mass: 1.0,
            surface_tension_k: 0.1,
        }
    }

    fn two_particles_store(dist: f32) -> ParticleStore {
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(0.0, 0.0), FluidId(1), 1.0));
        store.add(Particle::new(Vec2::new(dist, 0.0), FluidId(1), 1.0));
        store
    }

    // No-op boundary for tests
    fn no_boundary(_pos: &mut Vec2, _vel: &mut Vec2) {}

    #[test]
    fn gravity_pulls_down() {
        let config = test_config();
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(0.0, 100.0), FluidId(1), 1.0));
        let y_before = store.positions[0].y;
        sph_step(&mut store, &config, 1.0 / 60.0, &no_boundary);
        assert!(
            store.positions[0].y < y_before,
            "Particle should fall: {} -> {}",
            y_before,
            store.positions[0].y
        );
    }

    #[test]
    fn close_particles_pushed_apart() {
        let config = test_config();
        let mut store = two_particles_store(2.0);
        let dist_before = store.positions[0].distance(store.positions[1]);
        let mut config_no_grav = config.clone();
        config_no_grav.gravity = Vec2::ZERO;
        for _ in 0..10 {
            sph_step(&mut store, &config_no_grav, 1.0 / 60.0, &no_boundary);
        }
        let dist_after = store.positions[0].distance(store.positions[1]);
        assert!(
            dist_after > dist_before,
            "Close particles should spread: {dist_before} -> {dist_after}"
        );
    }

    #[test]
    fn step_full_cycle() {
        let config = test_config();
        let mut store = two_particles_store(8.0);
        let initial_y = store.positions[0].y;
        sph_step(&mut store, &config, 1.0 / 60.0, &no_boundary);
        assert_ne!(store.positions[0].y, initial_y);
    }

    #[test]
    fn boundary_fn_is_called() {
        let config = test_config();
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(50.0, 50.0), FluidId(1), 1.0));
        // Boundary: clamp y >= 40
        let boundary = |pos: &mut Vec2, vel: &mut Vec2| {
            if pos.y < 40.0 {
                pos.y = 40.0;
                vel.y = vel.y.abs() * 0.3;
            }
        };
        // Run enough steps for particle to fall
        for _ in 0..100 {
            sph_step(&mut store, &config, 1.0 / 60.0, &boundary);
        }
        assert!(
            store.positions[0].y >= 39.9,
            "Particle should be above floor: {}",
            store.positions[0].y
        );
    }
}
