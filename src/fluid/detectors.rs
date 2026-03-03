use bevy::prelude::*;

use crate::fluid::cell::FluidId;
use crate::fluid::events::{ImpactKind, WaterImpactEvent};
use crate::physics::Velocity;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::WorldMap;

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
/// Compares the entity's current tile fluid with `FluidContactState::last_fluid`.
/// Emits a `WaterImpactEvent` with `ImpactKind::Splash` on transitions between
/// air and fluid (in either direction).
pub fn detect_entity_water_entry(
    mut events: MessageWriter<WaterImpactEvent>,
    mut query: Query<(&Transform, &Velocity, &mut FluidContactState)>,
    world_map: Res<WorldMap>,
    active_world: Res<ActiveWorld>,
) {
    let tile_size = active_world.tile_size;
    let chunk_size = active_world.chunk_size;

    for (transform, velocity, mut contact) in &mut query {
        let pos = transform.translation.truncate();

        // Convert world position to tile coordinates
        let tile_x = (pos.x / tile_size).floor() as i32;
        let tile_y = (pos.y / tile_size).floor() as i32;

        // Convert to data chunk coordinates (wrapping X for cylindrical worlds).
        // cy is not wrapped — chunks outside world bounds simply won't exist in
        // the map, and unwrap_or(FluidId::NONE) handles that gracefully.
        let data_cx = active_world.wrap_chunk_x(tile_x.div_euclid(chunk_size as i32));
        let cy = tile_y.div_euclid(chunk_size as i32);

        // Local coordinates within the chunk
        let local_x = tile_x.rem_euclid(chunk_size as i32) as u32;
        let local_y = tile_y.rem_euclid(chunk_size as i32) as u32;

        // Look up current fluid at entity position
        let current_fluid = world_map
            .chunks
            .get(&(data_cx, cy))
            .map(|chunk| {
                let idx = (local_y * chunk_size + local_x) as usize;
                if idx < chunk.fluids.len() && !chunk.fluids[idx].is_empty() {
                    chunk.fluids[idx].fluid_id
                } else {
                    FluidId::NONE
                }
            })
            .unwrap_or(FluidId::NONE);

        let was_in_fluid = contact.last_fluid != FluidId::NONE;
        let now_in_fluid = current_fluid != FluidId::NONE;

        // Emit splash on air→fluid or fluid→air transitions
        if was_in_fluid != now_in_fluid {
            let impact_fluid = if now_in_fluid {
                current_fluid
            } else {
                // On exit, current_fluid is NONE — preserve the fluid we just left
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
/// Throttled to every 0.15 seconds via a `SwimThrottle` resource accumulator.
/// Only fires for entities currently submerged in fluid and moving faster than
/// 10.0 px/s.
pub fn detect_entity_swimming(
    mut events: MessageWriter<WaterImpactEvent>,
    query: Query<(&Transform, &Velocity, &FluidContactState)>,
    time: Res<Time>,
    mut throttle: ResMut<SwimThrottle>,
) {
    // Throttle: only run every ~0.15 seconds.
    // Uses an accumulator rather than elapsed_secs() % interval to avoid
    // float precision loss at large elapsed times.
    throttle.0 += time.delta_secs();
    if throttle.0 < 0.15 {
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
        if speed <= 10.0 {
            continue;
        }

        let pos = transform.translation.truncate();
        events.write(WaterImpactEvent {
            position: pos,
            velocity: Vec2::new(velocity.x, velocity.y),
            kind: ImpactKind::Wake,
            fluid_id: contact.last_fluid,
            mass: 1.0,
        });
    }
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
        // No transition: both in fluid → no splash
        let last = FluidId(1);
        let current = FluidId(1);
        let transition = (last != FluidId::NONE) != (current != FluidId::NONE);
        assert!(!transition, "same fluid state should not trigger splash");

        // No transition: both air → no splash
        let last = FluidId::NONE;
        let current = FluidId::NONE;
        let transition = (last != FluidId::NONE) != (current != FluidId::NONE);
        assert!(!transition, "both air should not trigger splash");
    }
}
