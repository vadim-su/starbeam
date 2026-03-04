//! Position Based Fluids (PBF) simulation.
//!
//! Based on Macklin & Müller 2013 "Position Based Fluids".
//! Instead of computing forces and integrating, we solve density constraints
//! directly at the position level. This is unconditionally stable.
//!
//! Algorithm:
//! 1. Apply gravity to velocities, predict new positions
//! 2. For N solver iterations:
//!    a. Compute density at predicted positions
//!    b. Compute λ (Lagrange multiplier) from density constraint violation
//!    c. Compute position correction Δp from λ values
//!    d. Apply corrections to predicted positions
//! 3. Update velocities from position change
//! 4. Apply XSPH viscosity smoothing

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
    /// Prevents division by zero and controls constraint stiffness.
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
            epsilon: 600.0,
            viscosity: 0.01,
            gravity: Vec2::new(0.0, -200.0),
            particle_mass: 1.0,
            surface_tension_k: 0.1,
        }
    }
}

/// One PBF simulation step.
pub fn sph_step(store: &mut ParticleStore, config: &SphConfig, dt: f32) {
    if store.is_empty() {
        return;
    }
    let n = store.len();
    let h = config.smoothing_radius;

    // --- Step 1: Predict positions ---
    // Apply gravity to velocities and compute predicted positions.
    let mut predicted = Vec::with_capacity(n);
    for i in 0..n {
        store.velocities[i] += config.gravity * dt;
        predicted.push(store.positions[i] + store.velocities[i] * dt);
    }

    // Buffers for solver
    let mut lambdas = vec![0.0f32; n];
    let mut neighbors_buf = Vec::new();

    // --- Step 2: Solver iterations ---
    for _ in 0..config.solver_iterations {
        // Build spatial hash from predicted positions
        let grid = SpatialHash::from_positions(&predicted, h);

        // 2a: Compute density and lambda for each particle
        for i in 0..n {
            let pos_i = predicted[i];
            grid.query_into(pos_i, &mut neighbors_buf);

            // Compute density at predicted position
            let mut density = 0.0f32;
            for &j in &neighbors_buf {
                let r = pos_i.distance(predicted[j]);
                density += config.particle_mass * poly6(r, h);
            }
            store.densities[i] = density;

            // Density constraint: C_i = ρ_i/ρ₀ - 1
            let constraint = density / config.rest_density - 1.0;

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
                // ∇_{p_j} C_i = -(m_j/ρ₀) * ∇W(p_i - p_j, h)
                // spiky_gradient returns scalar (negative), dir points i→j direction
                // ∇_{p_i} W = spiky_gradient(r,h) * dir
                // ∇_{p_j} W = -spiky_gradient(r,h) * dir
                // So ∇_{p_j} C_i = -(m/ρ₀) * (-spiky_gradient * dir) = (m/ρ₀) * spiky_gradient * dir
                let grad_w = spiky_gradient(r, h) * dir;
                let grad_j = -(config.particle_mass / config.rest_density) * grad_w;
                sum_grad_sq += grad_j.length_squared();
                // Accumulate ∇_{p_i} C_i = (1/ρ₀) * Σ_j m_j * ∇_{p_i}W
                grad_i += (config.particle_mass / config.rest_density) * grad_w;
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
                // s_corr = -k * (W(r,h) / W(Δq,h))^n
                let s_corr = if config.surface_tension_k > 0.0 {
                    let w_dq = poly6(0.3 * h, h); // reference kernel value at 0.3h
                    if w_dq > 1e-10 {
                        let w_ratio = poly6(r, h) / w_dq;
                        -config.surface_tension_k * w_ratio * w_ratio * w_ratio * w_ratio // n=4
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };

                // Δp_i = (1/ρ₀) * Σ_j (λ_i + λ_j + s_corr) * ∇W
                delta_p += (1.0 / config.rest_density)
                    * (lambdas[i] + lambdas[j] + s_corr)
                    * grad_w;
            }
            predicted[i] += delta_p;
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

    // Recompute densities/pressures for debug display (using final positions)
    {
        let grid = SpatialHash::from_positions(&store.positions, h);
        for i in 0..n {
            let pos_i = store.positions[i];
            grid.query_into(pos_i, &mut neighbors_buf);
            let mut density = 0.0f32;
            for &j in &neighbors_buf {
                let r = pos_i.distance(store.positions[j]);
                density += config.particle_mass * poly6(r, h);
            }
            store.densities[i] = density;
            store.pressures[i] = density / config.rest_density - 1.0; // constraint value for debug
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
            epsilon: 600.0,
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

    #[test]
    fn gravity_pulls_down() {
        let config = test_config();
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(0.0, 100.0), FluidId(1), 1.0));
        let y_before = store.positions[0].y;
        sph_step(&mut store, &config, 1.0 / 60.0);
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
        let mut store = two_particles_store(2.0); // very close
        let dist_before = store.positions[0].distance(store.positions[1]);
        // Run several steps for constraint to take effect
        for _ in 0..10 {
            sph_step(&mut store, &config, 1.0 / 60.0);
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
        sph_step(&mut store, &config, 1.0 / 60.0);
        assert_ne!(store.positions[0].y, initial_y);
    }

    #[test]
    fn density_near_rest_for_well_spaced_particles() {
        let config = test_config();
        // Place particles on a grid with spacing = 8 (h/2)
        let mut store = ParticleStore::new();
        for x in 0..4 {
            for y in 0..4 {
                store.add(Particle::new(
                    Vec2::new(x as f32 * 8.0, y as f32 * 8.0),
                    FluidId(1),
                    1.0,
                ));
            }
        }
        // Run one step to compute density
        let mut config_no_gravity = config.clone();
        config_no_gravity.gravity = Vec2::ZERO;
        sph_step(&mut store, &config_no_gravity, 1.0 / 60.0);
        // Center particles should have density near rest_density
        let center = 5; // particle at (8, 8) — has neighbors on all sides
        let ratio = store.densities[center] / config.rest_density;
        assert!(
            ratio > 0.5 && ratio < 3.0,
            "Center density ratio should be reasonable: {ratio} (density={}, rest={})",
            store.densities[center],
            config.rest_density
        );
    }
}
