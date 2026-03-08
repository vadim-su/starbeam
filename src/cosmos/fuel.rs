//! Ship fuel resource — tracks fuel for autopilot navigation.
//!
//! Fuel is consumed when the player uses the autopilot console to travel
//! between celestial bodies. The cost is proportional to the orbit distance.

use bevy::prelude::*;

/// Ship fuel resource. Exists only when on a ship world.
#[derive(Resource, Debug, Clone)]
pub struct ShipFuel {
    pub current: f32,
    pub capacity: f32,
}

impl Default for ShipFuel {
    fn default() -> Self {
        Self {
            current: 100.0,
            capacity: 100.0,
        }
    }
}

impl ShipFuel {
    /// Try to consume `amount` fuel. Returns `true` if successful.
    pub fn consume(&mut self, amount: f32) -> bool {
        if self.current >= amount {
            self.current -= amount;
            true
        } else {
            false
        }
    }
}

/// Compute fuel cost to travel between two orbits.
/// Cost = |target_orbit - current_orbit| * 20.0
pub fn fuel_cost(from_orbit: u32, to_orbit: u32) -> f32 {
    (to_orbit as f32 - from_orbit as f32).abs() * 20.0
}

/// Compute travel duration in seconds between two orbits.
/// Duration = |target_orbit - current_orbit| * 10.0 seconds
pub fn travel_duration(from_orbit: u32, to_orbit: u32) -> f32 {
    (to_orbit as f32 - from_orbit as f32).abs() * 10.0
}

/// Map a planet type to its orbit biome name.
pub fn orbit_biome_for_planet_type(planet_type: &str) -> &str {
    match planet_type {
        "garden" => "orbit_garden",
        "barren" => "orbit_barren",
        _ => "deep_space",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consume_enough_fuel() {
        let mut fuel = ShipFuel {
            current: 50.0,
            capacity: 100.0,
        };
        assert!(fuel.consume(30.0));
        assert!((fuel.current - 20.0).abs() < f32::EPSILON);
    }

    #[test]
    fn consume_not_enough_fuel() {
        let mut fuel = ShipFuel {
            current: 10.0,
            capacity: 100.0,
        };
        assert!(!fuel.consume(30.0));
        assert!((fuel.current - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn fuel_cost_calculation() {
        assert!((fuel_cost(1, 3) - 40.0).abs() < f32::EPSILON);
        assert!((fuel_cost(3, 1) - 40.0).abs() < f32::EPSILON);
        assert!((fuel_cost(2, 2) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn travel_duration_calculation() {
        assert!((travel_duration(1, 3) - 20.0).abs() < f32::EPSILON);
        assert!((travel_duration(3, 1) - 20.0).abs() < f32::EPSILON);
    }

    #[test]
    fn orbit_biome_mapping() {
        assert_eq!(orbit_biome_for_planet_type("garden"), "orbit_garden");
        assert_eq!(orbit_biome_for_planet_type("barren"), "orbit_barren");
        assert_eq!(orbit_biome_for_planet_type("unknown"), "deep_space");
    }
}
