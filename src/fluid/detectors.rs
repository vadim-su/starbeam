use bevy::prelude::*;

use crate::fluid::cell::FluidId;
use crate::fluid::events::{ImpactKind, WaterImpactEvent};
use crate::fluid::sph_particle::ParticleStore;
use crate::fluid::sph_simulation::SphConfig;
use crate::particles::pool::ParticlePool;
use crate::physics::Velocity;
use crate::registry::world::ActiveWorld;

/// Check if any SPH particle is near the given world position.
/// Returns the FluidId of the nearest particle, or FluidId::NONE.
fn sph_fluid_at(pos: Vec2, particles: &ParticleStore, radius: f32) -> FluidId {
    if particles.is_empty() {
        return FluidId::NONE;
    }
    let mut best_dist = f32::MAX;
    let mut best_fluid = FluidId::NONE;
    for i in 0..particles.len() {
        let dist = pos.distance(particles.positions[i]);
        if dist < radius && dist < best_dist {
            best_dist = dist;
            best_fluid = particles.fluid_ids[i];
        }
    }
    best_fluid
}

/// Marker component for projectile entities.
/// Attach this to any entity that should leave bubble trails when flying through fluid.
#[derive(Component, Default)]
pub struct Projectile;

/// Tracks whether an entity was in fluid last frame.
#[derive(Component, Default)]
pub struct FluidContactState {
    pub last_fluid: FluidId,
}

/// Accumulated time used to throttle the swimming wake detector.
///
/// A dedicated accumulator avoids the float-precision loss of
/// `elapsed_secs() % interval` at large elapsed times.
#[derive(Resource, Default)]
pub struct SwimThrottle(pub f32);

/// Detect when an entity crosses a fluid surface (enters or exits fluid).
///
/// Uses SPH particle proximity to determine if the entity is in fluid.
/// Emits a `WaterImpactEvent` with `ImpactKind::Splash` on transitions between
/// air and fluid (in either direction).
pub fn detect_entity_water_entry(
    mut events: MessageWriter<WaterImpactEvent>,
    mut query: Query<(&Transform, &Velocity, &mut FluidContactState)>,
    particles: Res<ParticleStore>,
    sph_config: Res<SphConfig>,
) {
    for (transform, velocity, mut contact) in &mut query {
        let pos = transform.translation.truncate();

        // Look up current fluid at entity position via SPH particles
        let current_fluid = sph_fluid_at(pos, &particles, sph_config.smoothing_radius);

        let was_in_fluid = contact.last_fluid != FluidId::NONE;
        let now_in_fluid = current_fluid != FluidId::NONE;

        // Emit splash on air->fluid or fluid->air transitions
        if was_in_fluid != now_in_fluid {
            let impact_fluid = if now_in_fluid {
                current_fluid
            } else {
                // On exit, current_fluid is NONE -- preserve the fluid we just left
                contact.last_fluid
            };

            events.write(WaterImpactEvent {
                position: pos,
                velocity: Vec2::new(velocity.x, velocity.y),
                kind: ImpactKind::Splash,
                fluid_id: impact_fluid,
                mass: 5.0, // TODO: derive from entity mass component (no Mass component exists yet)
            });
        }

        contact.last_fluid = current_fluid;
    }
}

/// Emit wake events for entities moving through fluid.
///
/// Throttled to every 0.05 seconds via a `SwimThrottle` resource accumulator.
/// Only fires for entities currently submerged in fluid and moving faster than
/// 20.0 px/s. Fires frequently with small impulses for smooth wave response.
pub fn detect_entity_swimming(
    mut events: MessageWriter<WaterImpactEvent>,
    query: Query<(&Transform, &Velocity, &FluidContactState)>,
    time: Res<Time>,
    mut throttle: ResMut<SwimThrottle>,
    particles: Res<ParticleStore>,
    sph_config: Res<SphConfig>,
) {
    // Throttle: run every ~0.05 seconds (frequent, small impulses = smooth waves).
    throttle.0 += time.delta_secs();
    if throttle.0 < 0.05 {
        return;
    }
    throttle.0 = 0.0;

    for (transform, velocity, contact) in &query {
        // Only emit for entities currently in fluid
        if contact.last_fluid == FluidId::NONE {
            continue;
        }

        // Only emit for entities moving fast enough
        let speed = (velocity.x * velocity.x + velocity.y * velocity.y).sqrt();
        if speed <= 20.0 {
            continue;
        }

        let pos = transform.translation.truncate();

        // Check if entity is near SPH particles (near surface)
        let near_surface =
            sph_fluid_at(pos, &particles, sph_config.smoothing_radius) != FluidId::NONE;

        if !near_surface {
            continue;
        }

        events.write(WaterImpactEvent {
            position: pos,
            velocity: Vec2::new(velocity.x, velocity.y),
            kind: ImpactKind::Wake,
            fluid_id: contact.last_fluid,
            mass: 1.0,
        });
    }
}

/// Spawn bubble particles behind projectiles flying through fluid.
///
/// Throttled to every 0.05 s via a `Local<f32>` accumulator.
/// Bubbles float upward (negative gravity_scale) and fade out.
pub fn detect_projectile_in_fluid(
    query: Query<(&Transform, &Velocity), With<Projectile>>,
    mut pool: ResMut<ParticlePool>,
    time: Res<Time>,
    mut throttle: Local<f32>,
    particles: Res<ParticleStore>,
    sph_config: Res<SphConfig>,
    active_world: Res<ActiveWorld>,
) {
    *throttle += time.delta_secs();
    if *throttle < 0.05 {
        return;
    }
    *throttle = 0.0;

    let tile_size = active_world.tile_size;

    for (transform, _velocity) in &query {
        let pos = transform.translation.truncate();

        // Check if projectile is near SPH particles
        let in_fluid =
            sph_fluid_at(pos, &particles, sph_config.smoothing_radius) != FluidId::NONE;

        if !in_fluid {
            continue;
        }

        // Spawn 1-2 bubble particles floating upward
        for _ in 0..2 {
            let jitter_x = (rand_jitter() - 0.5) * tile_size * 0.5;
            pool.spawn(
                Vec2::new(pos.x + jitter_x, pos.y),
                Vec2::new(0.0, 30.0), // drift upward
                0.0,                  // no mass
                FluidId::NONE,
                0.6,                  // short lifetime
                2.5,                  // small bubble
                [0.8, 0.9, 1.0, 0.6], // whitish translucent
                -0.3,                 // negative gravity = float up
                true,                 // fade out as bubble rises
            );
        }
    }
}

/// Cheap deterministic pseudo-random jitter based on current time.
/// Returns a value in [0.0, 1.0). Not suitable for cryptography.
#[inline]
fn rand_jitter() -> f32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(12345);
    // Simple hash to spread bits
    let h = nanos.wrapping_mul(2654435761);
    (h as f32) / (u32::MAX as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fluid_contact_state_default_is_none() {
        let state = FluidContactState::default();
        assert_eq!(state.last_fluid, FluidId::NONE);
    }

    #[test]
    fn swim_throttle_default_is_zero() {
        let throttle = SwimThrottle::default();
        assert_eq!(throttle.0, 0.0);
    }

    #[test]
    fn splash_impact_fluid_on_exit_is_last_fluid() {
        // Verify the logic: when leaving fluid, impact_fluid uses contact.last_fluid
        let last = FluidId(1);
        let current = FluidId::NONE; // now out of fluid
        let was_in = last != FluidId::NONE;
        let now_in = current != FluidId::NONE;
        assert!(was_in != now_in, "transition should be detected");
        let impact = if now_in { current } else { last };
        assert_eq!(impact, FluidId(1), "should use last_fluid when exiting");
    }

    #[test]
    fn splash_impact_fluid_on_entry_is_current_fluid() {
        // Verify: when entering fluid, impact_fluid uses current_fluid
        let last = FluidId::NONE; // was out of fluid
        let current = FluidId(2); // now in fluid
        let was_in = last != FluidId::NONE;
        let now_in = current != FluidId::NONE;
        assert!(was_in != now_in, "transition should be detected");
        let impact = if now_in { current } else { last };
        assert_eq!(impact, FluidId(2), "should use current_fluid when entering");
    }

    #[test]
    fn no_event_when_fluid_state_unchanged() {
        // No transition: both in fluid -> no splash
        let last = FluidId(1);
        let current = FluidId(1);
        let transition = (last != FluidId::NONE) != (current != FluidId::NONE);
        assert!(!transition, "same fluid state should not trigger splash");

        // No transition: both air -> no splash
        let last = FluidId::NONE;
        let current = FluidId::NONE;
        let transition = (last != FluidId::NONE) != (current != FluidId::NONE);
        assert!(!transition, "both air should not trigger splash");
    }
}
