use bevy::ecs::message::MessageReader;
use bevy::prelude::*;

use crate::fluid::cell::{FluidCell, FluidId};
use crate::fluid::events::{ImpactKind, WaterImpactEvent};
use crate::fluid::registry::FluidRegistry;
use crate::fluid::wave::{WaveBuffer, WaveState};
use crate::particles::pool::ParticlePool;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::WorldMap;

/// Configuration for CA-to-particle splash transitions.
///
/// Controls how much fluid mass is displaced from the CA grid into particles
/// on impact, and the visual properties of the spawned splash particles.
#[derive(Resource, Debug, Clone)]
pub struct SplashConfig {
    /// Fraction of cell mass displaced on a Splash impact.
    pub splash_displacement: f32,
    /// Particles spawned per unit of displaced mass (visual density).
    pub particles_per_mass: f32,
    /// Max lifetime of splash particles in seconds.
    pub particle_lifetime: f32,
    /// Visual radius of each particle in world units.
    pub particle_size: f32,
    /// Minimum velocity magnitude to trigger splash particles.
    pub min_splash_velocity: f32,
}

impl Default for SplashConfig {
    fn default() -> Self {
        Self {
            splash_displacement: 0.3,
            particles_per_mass: 15.0,
            particle_lifetime: 1.5,
            particle_size: 2.5,
            min_splash_velocity: 5.0,
        }
    }
}

/// Consume `WaterImpactEvent`s and displace CA fluid mass into splash particles.
///
/// For each impact event, removes a fraction of mass from the CA cell at the
/// impact position and spawns particles carrying that mass. Particles are
/// distributed in a fan-shaped arc above the impact point.
#[allow(clippy::too_many_arguments)]
pub fn spawn_splash_particles(
    mut events: MessageReader<WaterImpactEvent>,
    mut pool: ResMut<ParticlePool>,
    mut world_map: ResMut<WorldMap>,
    fluid_registry: Res<FluidRegistry>,
    active_world: Res<ActiveWorld>,
    splash_config: Res<SplashConfig>,
) {
    let chunk_size = active_world.chunk_size;
    let tile_size = active_world.tile_size;

    for event in events.read() {
        // Skip low-velocity splashes
        if event.kind == ImpactKind::Splash
            && event.velocity.length() < splash_config.min_splash_velocity
        {
            continue;
        }

        // Convert world position to chunk/local coords
        let tile_x = (event.position.x / tile_size).floor() as i32;
        let tile_y = (event.position.y / tile_size).floor() as i32;
        let cx = active_world.wrap_chunk_x(tile_x.div_euclid(chunk_size as i32));
        let cy = tile_y.div_euclid(chunk_size as i32);
        let local_x = tile_x.rem_euclid(chunk_size as i32) as u32;
        let local_y = tile_y.rem_euclid(chunk_size as i32) as u32;
        let idx = (local_y * chunk_size + local_x) as usize;

        // Get the CA cell at impact position
        let Some(chunk) = world_map.chunks.get(&(cx, cy)) else {
            continue;
        };
        if idx >= chunk.fluids.len() {
            continue;
        }
        let cell = chunk.fluids[idx];
        if cell.is_empty() {
            continue;
        }

        // Calculate displaced mass and particle count based on impact kind
        let (displaced, particle_count) = match event.kind {
            ImpactKind::Splash => {
                let raw = cell.mass * splash_config.splash_displacement;
                let displaced = raw.min(cell.mass - 0.01).max(0.0);
                let count = (displaced * splash_config.particles_per_mass)
                    .round()
                    .clamp(4.0, 20.0) as u32;
                (displaced, count)
            }
            ImpactKind::Wake => {
                let displaced = (cell.mass * 0.02).min(cell.mass - 0.01).max(0.0);
                (displaced, 2)
            }
            ImpactKind::Pour => {
                let displaced = (cell.mass * 0.05).min(cell.mass - 0.01).max(0.0);
                let count = (displaced * splash_config.particles_per_mass)
                    .round()
                    .clamp(1.0, 5.0) as u32;
                (displaced, count)
            }
        };

        if displaced <= 0.0 || particle_count == 0 {
            continue;
        }

        // Remove displaced mass from CA cell
        let chunk = world_map.chunks.get_mut(&(cx, cy)).unwrap();
        chunk.fluids[idx].mass -= displaced;
        if chunk.fluids[idx].mass < 0.001 {
            chunk.fluids[idx] = FluidCell::EMPTY;
        }

        // Get fluid color from registry (convert u8 RGBA to f32 RGBA)
        let fluid_def = fluid_registry.get(event.fluid_id);
        let color = [
            fluid_def.color[0] as f32 / 255.0,
            fluid_def.color[1] as f32 / 255.0,
            fluid_def.color[2] as f32 / 255.0,
            fluid_def.color[3] as f32 / 255.0,
        ];

        let mass_per_particle = displaced / particle_count as f32;
        let speed = event.velocity.length() * 0.3;

        // Fan-shaped velocity: angles from ~27deg to ~153deg (0.15pi to 0.85pi)
        for i in 0..particle_count {
            let t = if particle_count > 1 {
                i as f32 / (particle_count - 1) as f32
            } else {
                0.5
            };
            let angle = std::f32::consts::PI * (0.15 + t * 0.70); // 0.15pi .. 0.85pi
            let vx = angle.cos() * speed;
            let vy = angle.sin() * speed;

            pool.spawn(
                event.position,
                Vec2::new(vx, vy),
                mass_per_particle,
                event.fluid_id,
                splash_config.particle_lifetime,
                splash_config.particle_size,
                color,
            );
        }
    }
}

/// Reabsorb particles back into the CA fluid grid.
///
/// For each alive particle carrying fluid mass, checks if the particle is
/// inside a CA cell of the same fluid type. If so, transfers the particle's
/// mass back to the cell, applies a wave impulse from the particle's velocity,
/// and marks the particle dead.
pub fn reabsorb_particles(
    mut pool: ResMut<ParticlePool>,
    mut world_map: ResMut<WorldMap>,
    mut wave_state: ResMut<WaveState>,
    active_world: Res<ActiveWorld>,
) {
    let chunk_size = active_world.chunk_size;
    let tile_size = active_world.tile_size;

    for i in 0..pool.particles.len() {
        let p = &pool.particles[i];
        if p.is_dead() || p.fluid_id == FluidId::NONE {
            continue;
        }

        // Convert particle position to chunk/local coords
        let tile_x = (p.position.x / tile_size).floor() as i32;
        let tile_y = (p.position.y / tile_size).floor() as i32;
        let cx = active_world.wrap_chunk_x(tile_x.div_euclid(chunk_size as i32));
        let cy = tile_y.div_euclid(chunk_size as i32);
        let local_x = tile_x.rem_euclid(chunk_size as i32) as u32;
        let local_y = tile_y.rem_euclid(chunk_size as i32) as u32;
        let idx = (local_y * chunk_size + local_x) as usize;

        // Check if particle is inside a cell with the same fluid type
        let should_reabsorb = {
            let Some(chunk) = world_map.chunks.get(&(cx, cy)) else {
                continue;
            };
            if idx >= chunk.fluids.len() {
                continue;
            }
            let cell = chunk.fluids[idx];
            !cell.is_empty() && cell.fluid_id == p.fluid_id
        };

        if !should_reabsorb {
            continue;
        }

        // Capture particle data before mutating
        let particle_mass = pool.particles[i].mass;
        let particle_velocity = pool.particles[i].velocity;

        // Add particle mass back to cell
        let chunk = world_map.chunks.get_mut(&(cx, cy)).unwrap();
        chunk.fluids[idx].mass += particle_mass;

        // Apply wave impulse from particle velocity
        let impulse = particle_velocity.y.abs() * 0.01;
        let buf = wave_state
            .buffers
            .entry((cx, cy))
            .or_insert_with(|| WaveBuffer::new(chunk_size));
        buf.apply_impulse(local_x, local_y, impulse);

        // Mark particle dead
        pool.particles[i].alive = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splash_config_defaults() {
        let cfg = SplashConfig::default();
        assert!(cfg.splash_displacement > 0.0);
        assert!(cfg.particle_lifetime > 0.0);
        assert!(cfg.particles_per_mass > 0.0);
    }
}
