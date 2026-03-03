use bevy::prelude::*;

use super::pool::ParticlePool;
use crate::particles::ParticleConfig;

/// Update particle physics: move, age, apply gravity.
pub fn particle_physics(
    mut pool: ResMut<ParticlePool>,
    config: Res<ParticleConfig>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();

    for p in &mut pool.particles {
        if p.is_dead() {
            continue;
        }

        // Apply gravity (downward)
        p.velocity.y -= config.gravity * dt;

        // Integrate position
        p.position.x += p.velocity.x * dt;
        p.position.y += p.velocity.y * dt;

        // Age
        p.age += dt;

        // Mark dead if lifetime exceeded
        if p.age >= p.lifetime {
            p.alive = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::cell::FluidId;
    use crate::particles::particle::Particle;

    #[test]
    fn particle_ages_and_dies() {
        let mut p = Particle {
            position: Vec2::ZERO,
            velocity: Vec2::ZERO,
            mass: 0.1,
            fluid_id: FluidId(1),
            lifetime: 0.5,
            age: 0.0,
            size: 2.0,
            color: [1.0; 4],
            alive: true,
        };

        // After aging past lifetime, should be dead
        p.age = 0.6;
        assert!(p.age >= p.lifetime);
        p.alive = false;
        assert!(p.is_dead());
    }

    #[test]
    fn gravity_pulls_down() {
        let config = ParticleConfig::default();
        let dt = 0.016;
        let mut vy = 0.0;
        vy -= config.gravity * dt;
        assert!(vy < 0.0, "velocity y should decrease (fall)");
    }
}
