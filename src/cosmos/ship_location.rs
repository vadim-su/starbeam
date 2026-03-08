use bevy::prelude::*;

use crate::cosmos::address::CelestialAddress;
use crate::cosmos::current::CurrentSystem;
use crate::cosmos::fuel::{self, orbit_biome_for_planet_type, ShipFuel};
use crate::registry::biome::{BiomeId, BiomeRegistry};
use crate::ui::star_map::NavigateToBody;

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

/// Ticks ship travel progress. When travel completes, transitions from
/// `InTransit` to `Orbit` and updates the `GlobalBiome` to the destination's
/// orbit biome.
pub fn tick_ship_travel(
    time: Res<Time>,
    mut location: ResMut<ShipLocation>,
    mut global_biome: ResMut<GlobalBiome>,
    current_system: Res<CurrentSystem>,
    biome_registry: Res<BiomeRegistry>,
) {
    let ShipLocation::InTransit {
        progress,
        duration,
        to,
        ..
    } = location.as_mut()
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

        *location = ShipLocation::Orbit(dest);
    }
}

/// Handles `NavigateToBody` messages from the autopilot star map UI.
/// Consumes fuel, sets biome to deep_space, and starts InTransit.
pub fn handle_navigate(
    mut navigate_events: bevy::ecs::message::MessageReader<NavigateToBody>,
    mut location: ResMut<ShipLocation>,
    mut ship_fuel: ResMut<ShipFuel>,
    mut global_biome: ResMut<GlobalBiome>,
    current_system: Res<CurrentSystem>,
    biome_registry: Res<BiomeRegistry>,
) {
    let Some(nav) = navigate_events.read().last() else {
        return;
    };

    let ShipLocation::Orbit(from_addr) = location.clone() else {
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
    if !ship_fuel.consume(cost) {
        warn!(
            "NavigateToBody: insufficient fuel ({:.0} needed, {:.0} available)",
            cost, ship_fuel.current
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

    *location = ShipLocation::InTransit {
        from: from_addr,
        to: dest_body.address.clone(),
        progress: 0.0,
        duration,
    };
}
