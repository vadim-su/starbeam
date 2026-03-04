use bevy::math::Vec2;
use bevy::prelude::Resource;
use crate::fluid::cell::FluidId;

pub struct Particle {
    pub position: Vec2,
    pub velocity: Vec2,
    pub fluid_id: FluidId,
    pub mass: f32,
}

impl Particle {
    pub fn new(position: Vec2, fluid_id: FluidId, mass: f32) -> Self {
        Self { position, velocity: Vec2::ZERO, fluid_id, mass }
    }
}

#[derive(Resource, Default)]
pub struct ParticleStore {
    pub positions: Vec<Vec2>,
    pub velocities: Vec<Vec2>,
    pub densities: Vec<f32>,
    pub pressures: Vec<f32>,
    pub forces: Vec<Vec2>,
    pub fluid_ids: Vec<FluidId>,
    pub masses: Vec<f32>,
    /// Monotonically increasing counter that increments on any mutation.
    /// Used to skip expensive rebuilds (e.g. mesh) when nothing changed.
    pub generation: u64,
}

impl ParticleStore {
    pub fn new() -> Self { Self::default() }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            positions: Vec::with_capacity(cap),
            velocities: Vec::with_capacity(cap),
            densities: Vec::with_capacity(cap),
            pressures: Vec::with_capacity(cap),
            forces: Vec::with_capacity(cap),
            fluid_ids: Vec::with_capacity(cap),
            masses: Vec::with_capacity(cap),
            generation: 0,
        }
    }

    pub fn len(&self) -> usize { self.positions.len() }
    pub fn is_empty(&self) -> bool { self.positions.is_empty() }

    pub fn add(&mut self, p: Particle) {
        self.positions.push(p.position);
        self.velocities.push(p.velocity);
        self.densities.push(0.0);
        self.pressures.push(0.0);
        self.forces.push(Vec2::ZERO);
        self.fluid_ids.push(p.fluid_id);
        self.masses.push(p.mass);
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn remove_swap(&mut self, index: usize) {
        self.positions.swap_remove(index);
        self.velocities.swap_remove(index);
        self.densities.swap_remove(index);
        self.pressures.swap_remove(index);
        self.forces.swap_remove(index);
        self.fluid_ids.swap_remove(index);
        self.masses.swap_remove(index);
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn clear(&mut self) {
        self.positions.clear();
        self.velocities.clear();
        self.densities.clear();
        self.pressures.clear();
        self.forces.clear();
        self.fluid_ids.clear();
        self.masses.clear();
        self.generation = self.generation.wrapping_add(1);
    }

    /// Bump generation counter after external mutation (e.g. physics step modifying positions).
    pub fn mark_changed(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
}

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
