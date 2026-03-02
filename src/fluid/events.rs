use bevy::prelude::*;

use crate::fluid::cell::FluidId;

/// Kind of water interaction that occurred.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImpactKind {
    /// Entity entered/exited water (player, NPC, item).
    Splash,
    /// Entity moving through water.
    Wake,
    /// Fluid stream falling onto standing fluid surface.
    Pour,
}

/// Fired when something interacts with a fluid surface.
///
/// Consumed by:
///   - Wave system: writes impulse into wave_velocity buffer
///   - Particle system: spawns splash/wake particles
#[derive(Message, Debug, Clone)]
pub struct WaterImpactEvent {
    /// World-space position of the impact.
    pub position: Vec2,
    /// Velocity of the impacting object.
    pub velocity: Vec2,
    /// Type of interaction.
    pub kind: ImpactKind,
    /// Which fluid was impacted.
    pub fluid_id: FluidId,
    /// Mass of the impacting object (affects splash strength).
    pub mass: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impact_event_fields() {
        let evt = WaterImpactEvent {
            position: Vec2::new(10.0, 20.0),
            velocity: Vec2::new(0.0, -50.0),
            kind: ImpactKind::Splash,
            fluid_id: FluidId(1),
            mass: 5.0,
        };
        assert_eq!(evt.kind, ImpactKind::Splash);
        assert_eq!(evt.fluid_id, FluidId(1));
        assert!((evt.mass - 5.0).abs() < f32::EPSILON);
    }
}
