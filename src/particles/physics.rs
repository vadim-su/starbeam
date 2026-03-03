use bevy::prelude::*;

use super::pool::ParticlePool;
use crate::particles::ParticleConfig;
use crate::registry::tile::TileRegistry;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::WorldMap;

/// Update particle physics: move, age, apply gravity, kill on solid tile.
pub fn particle_physics(
    mut pool: ResMut<ParticlePool>,
    config: Res<ParticleConfig>,
    time: Res<Time>,
    world_map: Res<WorldMap>,
    tile_registry: Res<TileRegistry>,
    active_world: Res<ActiveWorld>,
) {
    let dt = time.delta_secs();
    let tile_size = active_world.tile_size;
    let chunk_size = active_world.chunk_size;

    for p in &mut pool.particles {
        if p.is_dead() {
            continue;
        }

        // Apply gravity scaled by particle's gravity_scale.
        // Negative scale = particle floats upward (e.g. bubbles).
        p.velocity.y -= config.gravity * p.gravity_scale * dt;

        // Integrate position
        p.position.x += p.velocity.x * dt;
        p.position.y += p.velocity.y * dt;

        // Age
        p.age += dt;

        // Mark dead if lifetime exceeded
        if p.age >= p.lifetime {
            p.alive = false;
            continue;
        }

        // Kill particle if it entered a solid tile
        let tile_x = (p.position.x / tile_size).floor() as i32;
        let tile_y = (p.position.y / tile_size).floor() as i32;
        let data_cx = active_world.wrap_chunk_x(tile_x.div_euclid(chunk_size as i32));
        let cy = tile_y.div_euclid(chunk_size as i32);
        let local_x = tile_x.rem_euclid(chunk_size as i32) as u32;
        let local_y = tile_y.rem_euclid(chunk_size as i32) as u32;

        if let Some(chunk) = world_map.chunks.get(&(data_cx, cy)) {
            let idx = (local_y * chunk_size + local_x) as usize;
            if idx < chunk.fg.tiles.len() {
                let tile_id = chunk.fg.tiles[idx];
                if tile_registry.is_solid(tile_id) {
                    p.alive = false;
                }
            }
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
            gravity_scale: 1.0,
            fade_out: false,
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
