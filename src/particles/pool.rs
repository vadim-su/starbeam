use bevy::prelude::*;

use super::particle::Particle;
use crate::fluid::cell::FluidId;

/// Global configuration for the particle system.
#[derive(Resource, Debug, Clone)]
pub struct ParticleConfig {
    pub max_particles: usize,
    pub gravity: f32,
}

impl Default for ParticleConfig {
    fn default() -> Self {
        Self {
            max_particles: 3000,
            gravity: 980.0,
        }
    }
}

/// Object-pool for particles. Uses ring-buffer search for O(1)-amortised
/// allocation and force-recycles the oldest particle when at capacity.
#[derive(Resource)]
pub struct ParticlePool {
    pub particles: Vec<Particle>,
    next_free: usize,
}

impl ParticlePool {
    /// Creates an empty pool with the given capacity limit.
    pub fn new(capacity: usize) -> Self {
        Self {
            particles: Vec::with_capacity(capacity),
            next_free: 0,
        }
    }

    /// Spawn a new particle, returning its index in the pool.
    ///
    /// Strategy:
    /// 1. Ring-buffer scan for a dead slot starting at `next_free`.
    /// 2. If none found and vec length < capacity, push a new entry.
    /// 3. If at capacity, force-kill the oldest (max age) particle and reuse.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        &mut self,
        position: Vec2,
        velocity: Vec2,
        mass: f32,
        fluid_id: FluidId,
        lifetime: f32,
        size: f32,
        color: [f32; 4],
    ) -> Option<usize> {
        let len = self.particles.len();
        let capacity = self.particles.capacity();

        // 1. Ring-buffer search for a dead slot.
        if len > 0 {
            for i in 0..len {
                let idx = (self.next_free + i) % len;
                if self.particles[idx].is_dead() {
                    self.init_particle(
                        idx, position, velocity, mass, fluid_id, lifetime, size, color,
                    );
                    self.next_free = (idx + 1) % len.max(1);
                    return Some(idx);
                }
            }
        }

        // 2. Grow vec if under capacity.
        if len < capacity {
            let idx = len;
            self.particles.push(Self::make_particle(
                position, velocity, mass, fluid_id, lifetime, size, color,
            ));
            self.next_free = (idx + 1) % self.particles.len().max(1);
            return Some(idx);
        }

        // 3. At capacity — force-kill the oldest particle (max age) and reuse.
        if len == 0 {
            return None;
        }
        let oldest_idx = self
            .particles
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                a.age
                    .partial_cmp(&b.age)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
            .unwrap();

        self.init_particle(
            oldest_idx, position, velocity, mass, fluid_id, lifetime, size, color,
        );
        self.next_free = (oldest_idx + 1) % len.max(1);
        Some(oldest_idx)
    }

    /// Count of currently alive particles.
    pub fn alive_count(&self) -> usize {
        self.particles.iter().filter(|p| !p.is_dead()).count()
    }

    // ── helpers ──────────────────────────────────────────────────────

    fn make_particle(
        position: Vec2,
        velocity: Vec2,
        mass: f32,
        fluid_id: FluidId,
        lifetime: f32,
        size: f32,
        color: [f32; 4],
    ) -> Particle {
        Particle {
            position,
            velocity,
            mass,
            fluid_id,
            lifetime,
            age: 0.0,
            size,
            color,
            alive: true,
        }
    }

    fn init_particle(
        &mut self,
        idx: usize,
        position: Vec2,
        velocity: Vec2,
        mass: f32,
        fluid_id: FluidId,
        lifetime: f32,
        size: f32,
        color: [f32; 4],
    ) {
        let p = &mut self.particles[idx];
        p.position = position;
        p.velocity = velocity;
        p.mass = mass;
        p.fluid_id = fluid_id;
        p.lifetime = lifetime;
        p.age = 0.0;
        p.size = size;
        p.color = color;
        p.alive = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::cell::FluidId;

    #[test]
    fn spawn_and_count() {
        let mut pool = ParticlePool::new(10);
        pool.spawn(Vec2::ZERO, Vec2::ZERO, 0.1, FluidId(1), 1.0, 2.0, [0.0; 4]);
        assert_eq!(pool.alive_count(), 1);
    }

    #[test]
    fn dead_particles_recycled() {
        let mut pool = ParticlePool::new(2);
        let idx0 = pool
            .spawn(Vec2::ZERO, Vec2::ZERO, 0.1, FluidId(1), 1.0, 2.0, [0.0; 4])
            .unwrap();
        pool.particles[idx0].alive = false;
        let idx1 = pool
            .spawn(Vec2::ONE, Vec2::ONE, 0.2, FluidId(1), 2.0, 3.0, [1.0; 4])
            .unwrap();
        assert_eq!(idx1, idx0, "should reuse dead slot");
        assert_eq!(pool.alive_count(), 1);
    }

    #[test]
    fn pool_capacity_forces_recycle() {
        let mut pool = ParticlePool::new(3);
        for _ in 0..3 {
            pool.spawn(Vec2::ZERO, Vec2::ZERO, 0.1, FluidId(1), 1.0, 2.0, [0.0; 4]);
        }
        assert_eq!(pool.alive_count(), 3);
        let idx = pool.spawn(Vec2::ZERO, Vec2::ZERO, 0.1, FluidId(1), 1.0, 2.0, [0.0; 4]);
        assert!(idx.is_some());
        assert_eq!(pool.alive_count(), 3); // still 3, one was force-recycled
    }

    #[test]
    fn particle_age_ratio() {
        let p = Particle {
            position: Vec2::ZERO,
            velocity: Vec2::ZERO,
            mass: 0.0,
            fluid_id: FluidId::NONE,
            lifetime: 2.0,
            age: 1.0,
            size: 1.0,
            color: [1.0; 4],
            alive: true,
        };
        assert!((p.age_ratio() - 0.5).abs() < f32::EPSILON);
    }
}
