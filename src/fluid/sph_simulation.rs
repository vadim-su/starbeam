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
}

impl Default for SphConfig {
    fn default() -> Self {
        Self {
            smoothing_radius: 16.0,
            rest_density: 0.0,
            stiffness: 100.0,
            viscosity: 0.2,
            gravity: Vec2::new(0.0, -200.0),
            particle_mass: 1.0,
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
        // Clamp pressure >= 0: no attraction, only repulsion.
        // Negative pressure causes particles to collapse into a single point.
        store.pressures[i] = (config.stiffness * (density - config.rest_density)).max(0.0);
    }
}

pub fn compute_forces(store: &mut ParticleStore, config: &SphConfig, grid: &SpatialHash) {
    let h = config.smoothing_radius;
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
            if density_j > 1e-6 {
                let pressure_j = store.pressures[j];
                let pressure_avg = (pressure_i + pressure_j) * 0.5;
                // Standard SPH: f = -∑ m_j * P_avg / ρ_j * ∇W
                // spiky_gradient returns negative, dir points away from j
                // so we negate to get repulsive force (away from neighbors)
                f_pressure -=
                    dir * store.masses[j] * pressure_avg / density_j * spiky_gradient(r, h);
            }
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
            rest_density: 0.0,
            stiffness: 100.0,
            viscosity: 1.0,
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
    fn pressure_follows_equation_of_state() {
        let config = test_config();
        let mut store = two_particles_store(2.0);
        let grid = SpatialHash::from_positions(&store.positions, config.smoothing_radius);
        compute_density_pressure(&mut store, &config, &grid);
        // P = max(0, k * (rho - rho_0))
        let expected = (config.stiffness * (store.densities[0] - config.rest_density)).max(0.0);
        assert!(
            (store.pressures[0] - expected).abs() < 1e-6,
            "Pressure should follow clamped equation of state: got {}, expected {}",
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
