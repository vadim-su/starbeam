//! Capsule and airlock marker components for planet-to-ship warping.
//!
//! When the player interacts with a capsule on a planet surface, they warp to
//! their ship.  When they interact with the airlock on their ship, they warp
//! back to the planet where their capsule is placed.

use bevy::prelude::*;

use crate::cosmos::address::CelestialAddress;

/// Tracks where the player's capsule is placed on a planet.
///
/// Inserted when the player interacts with a capsule, so that the airlock
/// interaction knows which planet (and position) to return to.
#[derive(Resource, Debug, Clone)]
pub struct CapsuleLocation {
    pub planet_address: CelestialAddress,
    pub planet_orbit: u32,
    pub tile_x: i32,
    pub tile_y: i32,
}

/// Marker component inserted on spawned capsule objects.
#[derive(Component, Debug)]
pub struct CapsuleMarker;

/// Marker component inserted on spawned airlock objects.
#[derive(Component, Debug)]
pub struct AirlockMarker;

/// Marker component inserted on spawned autopilot console objects.
#[derive(Component, Debug)]
pub struct AutopilotMarker;
