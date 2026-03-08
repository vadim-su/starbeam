use bevy::prelude::*;

use crate::cosmos::address::CelestialAddress;
use crate::registry::biome::BiomeId;

/// When present, overrides biome detection for the entire world.
/// Used for ship worlds where the biome represents the ship's location
/// rather than the player's horizontal position.
#[derive(Resource, Debug)]
pub struct GlobalBiome {
    pub biome_id: BiomeId,
}

/// Tracks the ship's current orbital location.
#[derive(Resource, Debug, Clone)]
pub enum ShipLocation {
    /// Ship is orbiting a celestial body.
    Orbit(CelestialAddress),
    /// Ship is travelling between bodies.
    InTransit {
        from: CelestialAddress,
        to: CelestialAddress,
        progress: f32,
        duration: f32,
    },
}
