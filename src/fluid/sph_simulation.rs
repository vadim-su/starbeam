use bevy::math::Vec2;
use bevy::prelude::Resource;

use crate::fluid::spatial_hash::SpatialHash;
use crate::fluid::sph_kernels::{poly6, spiky_gradient, viscosity_laplacian};
use crate::fluid::sph_particle::ParticleStore;

#[derive(Debug, Clone, Resource)]
pub struct SphConfig {
    pub smoothing_radius: f32,
    pub rest_density: f32,
    pub stiffness: f32,
    pub viscosity: f32,
    pub gravity: Vec2,
    pub particle_mass: f32,
    /// Tait EOS exponent (γ). Higher = more incompressible. 7 for water.
    pub eos_gamma: f32,
    /// XSPH velocity smoothing factor (0 = off, 0.5 = typical).
    pub xsph_factor: f32,
}

impl Default for SphConfig {
    fn default() -> Self {
        Self {
            smoothing_radius: 16.0,
            rest_density: 0.016,
            stiffness: 50.0,
            viscosity: 0.3,
            gravity: Vec2::new(0.0, -200.0),
            particle_mass: 1.0,
            eos_gamma: 7.0,
            xsph_factor: 0.3,
        }
    }
}

pub fn compute_density_pressure(store: &mut ParticleStore, config: &SphConfig, grid: &SpatialHash) {
    let h = config.smoothing_radius;
    let mut neighbors = Vec::new();
    for i in 0..store.len() {
        let pos_i = store.positions[i];
        let mut density = 0.0f32;
        grid.query_into(pos_i, &mut neighbors);
        for &j in &neighbors {
            let r = pos_i.distance(store.positions[j]);
            density += store.masses[j] * poly6(r, h);
        }
        store.densities[i] = density;
        // Tait equation of state: P = B * ((ρ/ρ₀)^γ - 1)
        // Much stiffer than linear EOS at high compression, nearly incompressible.
        let rho0 = config.rest_density.max(1e-8);
        let ratio = density / rho0;
        store.pressures[i] = config.stiffness * (ratio.powf(config.eos_gamma) - 1.0);
    }
}

pub fn compute_forces(store: &mut ParticleStore, config: &SphConfig, grid: &SpatialHash) {
    let h = config.smoothing_radius;
    // Minimum separation distance — particles closer than this get strong repulsion
    let min_dist = h * 0.25;
    let mut neighbors = Vec::new();
    for i in 0..store.len() {
        let pos_i = store.positions[i];
        let vel_i = store.velocities[i];
        let pressure_i = store.pressures[i];
        let density_i = store.densities[i];
        let mut f_pressure = Vec2::ZERO;
        let mut f_viscosity = Vec2::ZERO;
        grid.query_into(pos_i, &mut neighbors);
        for &j in &neighbors {
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

            // SPH pressure force
            if density_j > 1e-6 {
                let pressure_j = store.pressures[j];
                let pressure_avg = (pressure_i + pressure_j) * 0.5;
                f_pressure -=
                    dir * store.masses[j] * pressure_avg / density_j * spiky_gradient(r, h);
            }

            // Short-range repulsion: prevents interpenetration
            if r < min_dist {
                let overlap = (min_dist - r) / min_dist; // 1 at r=0, 0 at min_dist
                // Quadratic repulsion, scaled by stiffness
                let repulsion = config.stiffness * 4.0 * overlap * overlap;
                f_pressure += dir * repulsion;
            }

            // Viscosity
            if density_j > 1e-6 {
                let vel_j = store.velocities[j];
                f_viscosity +=
                    (vel_j - vel_i) * store.masses[j] / density_j * viscosity_laplacian(r, h);
            }
        }
        f_viscosity *= config.viscosity;
        let f_gravity = config.gravity * density_i;
        store.forces[i] = f_pressure + f_viscosity + f_gravity;
    }
}

/// XSPH velocity smoothing: smooths velocity field by blending with neighbors.
/// Prevents particle clustering and produces smoother flow.
pub fn xsph_smooth(store: &mut ParticleStore, config: &SphConfig, grid: &SpatialHash) {
    if config.xsph_factor <= 0.0 {
        return;
    }
    let h = config.smoothing_radius;
    let epsilon = config.xsph_factor;
    let n = store.len();
    // Compute corrections in a separate buffer to avoid read-write conflict
    let mut corrections: Vec<Vec2> = vec![Vec2::ZERO; n];
    let mut neighbors = Vec::new();
    for i in 0..n {
        let pos_i = store.positions[i];
        let vel_i = store.velocities[i];
        let mut correction = Vec2::ZERO;
        grid.query_into(pos_i, &mut neighbors);
        for &j in &neighbors {
            if i == j {
                continue;
            }
            let r = pos_i.distance(store.positions[j]);
            if r > h {
                continue;
            }
            let density_j = store.densities[j];
            if density_j > 1e-6 {
                let vel_j = store.velocities[j];
                correction += store.masses[j] / density_j * (vel_j - vel_i) * poly6(r, h);
            }
        }
        corrections[i] = correction * epsilon;
    }
    for i in 0..n {
        store.velocities[i] += corrections[i];
    }
}

pub fn integrate(store: &mut ParticleStore, dt: f32) {
    for i in 0..store.len() {
        let density = store.densities[i].max(1e-6);
        let acceleration = store.forces[i] / density;
        store.velocities[i] += acceleration * dt;
        store.positions[i] += store.velocities[i] * dt;
    }
    store.mark_changed();
}

pub fn sph_step(store: &mut ParticleStore, config: &SphConfig, dt: f32) {
    if store.is_empty() {
        return;
    }
    let grid = SpatialHash::from_positions(&store.positions, config.smoothing_radius);
    compute_density_pressure(store, config, &grid);
    compute_forces(store, config, &grid);
    integrate(store, dt);
    // XSPH smoothing uses the same spatial hash (positions barely moved)
    xsph_smooth(store, config, &grid);
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
            rest_density: 0.016,
            stiffness: 50.0,
            viscosity: 0.3,
            gravity: Vec2::new(0.0, -98.0),
            particle_mass: 1.0,
            eos_gamma: 7.0,
            xsph_factor: 0.3,
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
        let grid_close = SpatialHash::from_positions(&close.positions, config.smoothing_radius);
        let grid_far = SpatialHash::from_positions(&far.positions, config.smoothing_radius);
        compute_density_pressure(&mut close, &config, &grid_close);
        compute_density_pressure(&mut far, &config, &grid_far);
        assert!(
            close.densities[0] > far.densities[0],
            "Closer particles should have higher density: {} vs {}",
            close.densities[0],
            far.densities[0]
        );
    }

    #[test]
    fn pressure_follows_tait_eos() {
        let config = test_config();
        let mut store = two_particles_store(2.0);
        let grid = SpatialHash::from_positions(&store.positions, config.smoothing_radius);
        compute_density_pressure(&mut store, &config, &grid);
        // P = B * ((ρ/ρ₀)^γ - 1)
        let ratio = store.densities[0] / config.rest_density;
        let expected = config.stiffness * (ratio.powf(config.eos_gamma) - 1.0);
        assert!(
            (store.pressures[0] - expected).abs() < 1e-3,
            "Pressure should follow Tait EOS: got {}, expected {}",
            store.pressures[0],
            expected
        );
    }

    #[test]
    fn gravity_pulls_down() {
        let config = test_config();
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(0.0, 100.0), FluidId(1), 1.0));
        let grid = SpatialHash::from_positions(&store.positions, config.smoothing_radius);
        compute_density_pressure(&mut store, &config, &grid);
        compute_forces(&mut store, &config, &grid);
        assert!(store.forces[0].y < 0.0, "Gravity should pull down");
    }

    #[test]
    fn pressure_pushes_apart() {
        // Need enough particles close together to exceed rest_density and create
        // positive pressure. Place a cluster of 5 particles within small area.
        let config = test_config();
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(0.0, 0.0), FluidId(1), 1.0));
        store.add(Particle::new(Vec2::new(2.0, 0.0), FluidId(1), 1.0));
        store.add(Particle::new(Vec2::new(1.0, 1.0), FluidId(1), 1.0));
        store.add(Particle::new(Vec2::new(1.0, -1.0), FluidId(1), 1.0));
        store.add(Particle::new(Vec2::new(1.0, 0.0), FluidId(1), 1.0));
        let grid = SpatialHash::from_positions(&store.positions, config.smoothing_radius);
        compute_density_pressure(&mut store, &config, &grid);
        compute_forces(&mut store, &config, &grid);
        // Outermost particle (0 at x=0) should be pushed left (away from cluster center)
        assert!(
            store.forces[0].x < 0.0,
            "Left particle pushed left: {}",
            store.forces[0].x
        );
    }

    #[test]
    fn integrate_moves_particle() {
        let config = test_config();
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(0.0, 100.0), FluidId(1), 1.0));
        let grid = SpatialHash::from_positions(&store.positions, config.smoothing_radius);
        compute_density_pressure(&mut store, &config, &grid);
        compute_forces(&mut store, &config, &grid);
        integrate(&mut store, 1.0 / 60.0);
        assert!(store.positions[0].y < 100.0, "Particle should fall");
        assert!(store.velocities[0].y < 0.0, "Velocity should be downward");
    }

    #[test]
    fn step_full_cycle() {
        let config = test_config();
        let mut store = two_particles_store(8.0);
        let initial_y = store.positions[0].y;
        sph_step(&mut store, &config, 1.0 / 60.0);
        assert_ne!(store.positions[0].y, initial_y);
    }
}
