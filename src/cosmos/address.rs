//! Celestial addressing and deterministic seed derivation for the procedural universe.
//!
//! Every celestial body (planet, moon, station, asteroid, ship) is uniquely
//! identified by a [`CelestialAddress`] and all its procedural parameters are
//! derived deterministically from a universe seed via a hash-chain producing
//! [`CelestialSeeds`].

use bevy::math::IVec2;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Celestial address
// ---------------------------------------------------------------------------

/// Unique address for any celestial body in the universe.
///
/// Each variant captures the minimal coordinates needed for that location type.
/// Only `Planet` and `Moon` are constructed today; the remaining variants exist
/// to support future content without another migration.
#[derive(Clone, Hash, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum CelestialAddress {
    Planet {
        galaxy: IVec2,
        system: IVec2,
        orbit: u32,
    },
    Moon {
        galaxy: IVec2,
        system: IVec2,
        orbit: u32,
        satellite: u32,
    },
    Station {
        galaxy: IVec2,
        system: IVec2,
        station_id: u32,
    },
    Asteroid {
        galaxy: IVec2,
        system: IVec2,
        belt: u32,
        index: u32,
    },
    Ship {
        ship_id: u64,
    },
}

impl CelestialAddress {
    pub fn planet(galaxy: IVec2, system: IVec2, orbit: u32) -> Self {
        Self::Planet {
            galaxy,
            system,
            orbit,
        }
    }

    pub fn moon(galaxy: IVec2, system: IVec2, orbit: u32, satellite: u32) -> Self {
        Self::Moon {
            galaxy,
            system,
            orbit,
            satellite,
        }
    }

    pub fn galaxy(&self) -> Option<IVec2> {
        match self {
            Self::Planet { galaxy, .. }
            | Self::Moon { galaxy, .. }
            | Self::Station { galaxy, .. }
            | Self::Asteroid { galaxy, .. } => Some(*galaxy),
            Self::Ship { .. } => None,
        }
    }

    pub fn system(&self) -> Option<IVec2> {
        match self {
            Self::Planet { system, .. }
            | Self::Moon { system, .. }
            | Self::Station { system, .. }
            | Self::Asteroid { system, .. } => Some(*system),
            Self::Ship { .. } => None,
        }
    }

    pub fn orbit(&self) -> Option<u32> {
        match self {
            Self::Planet { orbit, .. } | Self::Moon { orbit, .. } => Some(*orbit),
            _ => None,
        }
    }

    pub fn satellite(&self) -> Option<u32> {
        match self {
            Self::Moon { satellite, .. } => Some(*satellite),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Celestial seeds
// ---------------------------------------------------------------------------

/// Deterministic seeds derived from a universe seed + address via hash-chain.
///
/// Each seed governs a different aspect of procedural generation for the body.
#[derive(Debug, Clone)]
pub struct CelestialSeeds {
    pub galaxy_seed: u64,
    pub system_seed: u64,
    pub star_seed: u64,
    pub body_seed: u64,
    pub terrain_seed: u64,
    pub daynight_seed: u64,
    pub biome_seed: u64,
}

impl CelestialSeeds {
    /// Derive all seeds for a celestial body from a universe seed and address.
    ///
    /// Hash-chain:
    /// ```text
    /// universe_seed
    ///   → hash(universe_seed, galaxy.x, galaxy.y)           → galaxy_seed
    ///     → hash(galaxy_seed, system.x, system.y)            → system_seed
    ///       → hash(system_seed, "star")                      → star_seed
    ///       → hash(system_seed, orbit_index)                 → planet_seed
    ///         → hash(planet_seed, "terrain")                 → terrain_seed
    ///         → hash(planet_seed, "daynight")                → daynight_seed
    ///         → hash(planet_seed, "biomes")                  → biome_seed
    ///         → hash(planet_seed, satellite_index)            → moon_seed
    /// ```
    ///
    /// For moons the body_seed is double-hashed:
    /// `hash(hash(system_seed, orbit), satellite_index)`.
    pub fn derive(universe_seed: u64, address: &CelestialAddress) -> Self {
        let (galaxy, system, orbit, satellite) = match address {
            CelestialAddress::Planet {
                galaxy,
                system,
                orbit,
            } => (*galaxy, *system, *orbit, None),
            CelestialAddress::Moon {
                galaxy,
                system,
                orbit,
                satellite,
            } => (*galaxy, *system, *orbit, Some(*satellite)),
            CelestialAddress::Station {
                galaxy,
                system,
                station_id,
            } => (*galaxy, *system, *station_id, None),
            CelestialAddress::Asteroid {
                galaxy,
                system,
                belt,
                index,
            } => (*galaxy, *system, *belt, Some(*index)),
            CelestialAddress::Ship { ship_id } => {
                (IVec2::ZERO, IVec2::ZERO, *ship_id as u32, None)
            }
        };

        let galaxy_seed = hash_combine(universe_seed, pack_coords(galaxy.x, galaxy.y));
        let system_seed = hash_combine(galaxy_seed, pack_coords(system.x, system.y));
        let star_seed = hash_tag(system_seed, "star");
        let planet_seed = hash_combine(system_seed, orbit as u64);

        let body_seed = match satellite {
            Some(sat) => hash_combine(planet_seed, sat as u64),
            None => planet_seed,
        };

        let terrain_seed = hash_tag(body_seed, "terrain");
        let daynight_seed = hash_tag(body_seed, "daynight");
        let biome_seed = hash_tag(body_seed, "biomes");

        Self {
            galaxy_seed,
            system_seed,
            star_seed,
            body_seed,
            terrain_seed,
            daynight_seed,
            biome_seed,
        }
    }

    /// Convenience: truncate `terrain_seed` to `u32` for Perlin noise compatibility.
    pub fn terrain_seed_u32(&self) -> u32 {
        self.terrain_seed as u32
    }
}

// ---------------------------------------------------------------------------
// SplitMix64-based hash combiners
// ---------------------------------------------------------------------------

/// Combine a seed with a value using a SplitMix64 mixing step.
fn hash_combine(seed: u64, value: u64) -> u64 {
    splitmix64(seed.wrapping_add(value))
}

/// Combine a seed with a string tag by hashing the tag bytes into a u64 first.
fn hash_tag(seed: u64, tag: &str) -> u64 {
    let tag_hash = tag
        .bytes()
        .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
    hash_combine(seed, tag_hash)
}

/// Pack two `i32` coordinates into a single `u64`.
fn pack_coords(x: i32, y: i32) -> u64 {
    ((x as u32 as u64) << 32) | (y as u32 as u64)
}

/// Single SplitMix64 mixing step — deterministic, fast, non-cryptographic.
fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e3779b97f4a7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const UNIVERSE_SEED: u64 = 12345;

    fn test_address() -> CelestialAddress {
        CelestialAddress::planet(IVec2::new(0, 0), IVec2::new(3, -2), 2)
    }

    #[test]
    fn derive_is_deterministic() {
        let addr = test_address();
        let a = CelestialSeeds::derive(UNIVERSE_SEED, &addr);
        let b = CelestialSeeds::derive(UNIVERSE_SEED, &addr);
        assert_eq!(a.galaxy_seed, b.galaxy_seed);
        assert_eq!(a.system_seed, b.system_seed);
        assert_eq!(a.star_seed, b.star_seed);
        assert_eq!(a.body_seed, b.body_seed);
        assert_eq!(a.terrain_seed, b.terrain_seed);
        assert_eq!(a.daynight_seed, b.daynight_seed);
        assert_eq!(a.biome_seed, b.biome_seed);
    }

    #[test]
    fn different_universe_seed_different_result() {
        let addr = test_address();
        let a = CelestialSeeds::derive(UNIVERSE_SEED, &addr);
        let b = CelestialSeeds::derive(99999, &addr);
        assert_ne!(a.galaxy_seed, b.galaxy_seed);
    }

    #[test]
    fn different_orbit_different_body_seed() {
        let addr_a = CelestialAddress::planet(IVec2::new(0, 0), IVec2::new(3, -2), 1);
        let addr_b = CelestialAddress::planet(IVec2::new(0, 0), IVec2::new(3, -2), 2);
        let a = CelestialSeeds::derive(UNIVERSE_SEED, &addr_a);
        let b = CelestialSeeds::derive(UNIVERSE_SEED, &addr_b);
        // Same system → same star_seed
        assert_eq!(a.star_seed, b.star_seed);
        // Different orbit → different body_seed
        assert_ne!(a.body_seed, b.body_seed);
    }

    #[test]
    fn moon_differs_from_planet() {
        let planet = CelestialAddress::planet(IVec2::new(0, 0), IVec2::new(1, 1), 3);
        let moon = CelestialAddress::moon(IVec2::new(0, 0), IVec2::new(1, 1), 3, 0);
        let p = CelestialSeeds::derive(UNIVERSE_SEED, &planet);
        let m = CelestialSeeds::derive(UNIVERSE_SEED, &moon);
        assert_ne!(p.body_seed, m.body_seed);
    }

    #[test]
    fn different_galaxy_different_seeds() {
        let addr_a = CelestialAddress::planet(IVec2::new(0, 0), IVec2::new(1, 1), 0);
        let addr_b = CelestialAddress::planet(IVec2::new(1, 0), IVec2::new(1, 1), 0);
        let a = CelestialSeeds::derive(UNIVERSE_SEED, &addr_a);
        let b = CelestialSeeds::derive(UNIVERSE_SEED, &addr_b);
        assert_ne!(a.galaxy_seed, b.galaxy_seed);
        assert_ne!(a.system_seed, b.system_seed);
        assert_ne!(a.star_seed, b.star_seed);
        assert_ne!(a.body_seed, b.body_seed);
        assert_ne!(a.terrain_seed, b.terrain_seed);
        assert_ne!(a.daynight_seed, b.daynight_seed);
        assert_ne!(a.biome_seed, b.biome_seed);
    }

    #[test]
    fn sub_seeds_differ_from_each_other() {
        let addr = test_address();
        let s = CelestialSeeds::derive(UNIVERSE_SEED, &addr);
        assert_ne!(s.terrain_seed, s.daynight_seed);
        assert_ne!(s.daynight_seed, s.biome_seed);
        assert_ne!(s.terrain_seed, s.biome_seed);
    }

    #[test]
    fn terrain_seed_u32_truncates() {
        let addr = test_address();
        let s = CelestialSeeds::derive(UNIVERSE_SEED, &addr);
        assert_eq!(s.terrain_seed_u32(), s.terrain_seed as u32);
    }
}
