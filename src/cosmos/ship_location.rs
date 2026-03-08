//! Ship manifest and navigation systems.
//!
//! `ShipManifest` is the central registry for all ships in the game. Each ship
//! has its own metadata (size, fuel, orbital location). Only the *active* ship
//! (the one the player is currently aboard) has its `ShipLocation` ticked for
//! real-time travel; the rest are "frozen".

use std::collections::HashMap;

use bevy::prelude::*;

use crate::cosmos::address::CelestialAddress;
use crate::cosmos::current::CurrentSystem;
use crate::cosmos::fuel::{self, orbit_biome_for_planet_type, ShipFuel};
use crate::registry::biome::{BiomeId, BiomeRegistry};
use crate::ui::star_map::NavigateToBody;

// ---------------------------------------------------------------------------
// GlobalBiome (unchanged)
// ---------------------------------------------------------------------------

/// When present, overrides biome detection for the entire world.
/// Used for ship worlds where the biome represents the ship's location
/// rather than the player's horizontal position.
#[derive(Resource, Debug)]
pub struct GlobalBiome {
    pub biome_id: BiomeId,
}

// ---------------------------------------------------------------------------
// ShipLocation (no longer a Resource — stored inside ShipMeta)
// ---------------------------------------------------------------------------

/// Tracks a ship's current orbital location.
#[derive(Debug, Clone)]
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

// ---------------------------------------------------------------------------
// Ship ownership
// ---------------------------------------------------------------------------

/// Who owns a ship.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShipOwner {
    Player(u64),
    Npc(u64),
}

// ---------------------------------------------------------------------------
// ShipMeta + ShipManifest
// ---------------------------------------------------------------------------

/// Per-ship metadata.
#[derive(Debug, Clone)]
pub struct ShipMeta {
    pub ship_id: u64,
    pub owner: ShipOwner,
    pub planet_type: String,
    pub width: i32,
    pub height: i32,
    pub location: ShipLocation,
    pub fuel: ShipFuel,
}

/// Central registry for all ships in the game.
#[derive(Resource, Debug)]
pub struct ShipManifest {
    pub ships: HashMap<u64, ShipMeta>,
    pub next_id: u64,
    /// Which ship the player is currently aboard (if any).
    pub active_ship: Option<u64>,
}

impl ShipManifest {
    /// Create a manifest with a single starter ship.
    pub fn with_starter_ship(orbit_address: CelestialAddress) -> Self {
        let mut ships = HashMap::new();
        ships.insert(
            0,
            ShipMeta {
                ship_id: 0,
                owner: ShipOwner::Player(0),
                planet_type: "ship".to_string(),
                width: 128,
                height: 64,
                location: ShipLocation::Orbit(orbit_address),
                fuel: ShipFuel::default(),
            },
        );
        Self {
            ships,
            next_id: 1,
            active_ship: Some(0),
        }
    }

    /// Get the active ship's metadata (immutable).
    pub fn active(&self) -> Option<&ShipMeta> {
        self.active_ship.and_then(|id| self.ships.get(&id))
    }

    /// Get the active ship's metadata (mutable).
    pub fn active_mut(&mut self) -> Option<&mut ShipMeta> {
        self.active_ship.and_then(|id| self.ships.get_mut(&id))
    }

    /// Get the `CelestialAddress` for a ship by id.
    pub fn address(&self, ship_id: u64) -> CelestialAddress {
        CelestialAddress::Ship { ship_id }
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Ticks ship travel progress for the active ship. When travel completes,
/// transitions from `InTransit` to `Orbit` and updates `GlobalBiome`.
pub fn tick_ship_travel(
    time: Res<Time>,
    manifest: Option<ResMut<ShipManifest>>,
    global_biome: Option<ResMut<GlobalBiome>>,
    current_system: Res<CurrentSystem>,
    biome_registry: Res<BiomeRegistry>,
) {
    let (Some(mut manifest), Some(mut global_biome)) = (manifest, global_biome) else {
        return;
    };

    let Some(ship) = manifest.active_mut() else {
        return;
    };

    let ShipLocation::InTransit {
        progress,
        duration,
        to,
        ..
    } = &mut ship.location
    else {
        return;
    };

    *progress += time.delta_secs() / *duration;

    if *progress >= 1.0 {
        let dest = to.clone();

        // Look up the destination's planet type to determine the orbit biome.
        let dest_orbit = dest.orbit();
        let orbit_biome_name = current_system
            .system
            .bodies
            .iter()
            .find(|b| b.address.orbit() == dest_orbit)
            .map(|b| orbit_biome_for_planet_type(&b.planet_type_id))
            .unwrap_or("deep_space");

        global_biome.biome_id = biome_registry.id_by_name(orbit_biome_name);
        info!(
            "Ship arrived at orbit {} — biome set to {}",
            dest_orbit.unwrap_or(0),
            orbit_biome_name
        );

        ship.location = ShipLocation::Orbit(dest);
    }
}

/// Handles `NavigateToBody` messages from the autopilot star map UI.
/// Consumes fuel from the active ship, sets biome to deep_space, starts InTransit.
pub fn handle_navigate(
    mut navigate_events: bevy::ecs::message::MessageReader<NavigateToBody>,
    manifest: Option<ResMut<ShipManifest>>,
    global_biome: Option<ResMut<GlobalBiome>>,
    current_system: Res<CurrentSystem>,
    biome_registry: Res<BiomeRegistry>,
) {
    let Some(nav) = navigate_events.read().last() else {
        return;
    };

    let (Some(mut manifest), Some(mut global_biome)) = (manifest, global_biome) else {
        return;
    };

    let Some(ship) = manifest.active_mut() else {
        return;
    };

    let ShipLocation::Orbit(from_addr) = ship.location.clone() else {
        warn!("NavigateToBody: ship is not in orbit, ignoring");
        return;
    };

    let from_orbit = from_addr.orbit().unwrap_or(0);
    let to_orbit = nav.orbit;

    // Find the destination body
    let Some(dest_body) = current_system
        .system
        .bodies
        .iter()
        .find(|b| b.address.orbit() == Some(to_orbit))
    else {
        warn!("NavigateToBody: no body at orbit {}", to_orbit);
        return;
    };

    // Consume fuel
    let cost = fuel::fuel_cost(from_orbit, to_orbit);
    if !ship.fuel.consume(cost) {
        warn!(
            "NavigateToBody: insufficient fuel ({:.0} needed, {:.0} available)",
            cost, ship.fuel.current
        );
        return;
    }

    // Set biome to deep_space for transit
    global_biome.biome_id = biome_registry.id_by_name("deep_space");

    // Compute travel duration
    let duration = fuel::travel_duration(from_orbit, to_orbit);

    info!(
        "Autopilot: navigating from orbit {} to orbit {} (cost: {:.0}, duration: {:.0}s)",
        from_orbit, to_orbit, cost, duration
    );

    ship.location = ShipLocation::InTransit {
        from: from_addr,
        to: dest_body.address.clone(),
        progress: 0.0,
        duration,
    };
}
