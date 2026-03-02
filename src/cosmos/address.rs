//! Celestial addressing and deterministic seed derivation for the procedural universe.
//!
//! Every celestial body (planet, moon) is uniquely identified by a [`CelestialAddress`]
//! and all its procedural parameters are derived deterministically from a universe seed
//! via a hash-chain producing [`CelestialSeeds`].

use bevy::math::IVec2;

// ---------------------------------------------------------------------------
// Celestial address
// ---------------------------------------------------------------------------

/// Unique address for any celestial body in the universe.
///
/// Hierarchy: galaxy → system → orbit → optional satellite (moon).
#[derive(Clone, Hash, Eq, PartialEq, Debug)]
pub struct CelestialAddress {
    pub galaxy: IVec2,
    pub system: IVec2,
    pub orbit: u32,
    pub satellite: Option<u32>,
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
        let galaxy_seed = hash_combine(
            universe_seed,
            pack_coords(address.galaxy.x, address.galaxy.y),
        );
        let system_seed =
            hash_combine(galaxy_seed, pack_coords(address.system.x, address.system.y));
        let star_seed = hash_tag(system_seed, "star");

        let planet_seed = hash_combine(system_seed, address.orbit as u64);

        let body_seed = match address.satellite {
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
        CelestialAddress {
            galaxy: IVec2::new(0, 0),
            system: IVec2::new(3, -2),
            orbit: 2,
            satellite: None,
        }
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
        let addr_a = CelestialAddress {
            galaxy: IVec2::new(0, 0),
            system: IVec2::new(3, -2),
            orbit: 1,
            satellite: None,
        };
        let addr_b = CelestialAddress {
            galaxy: IVec2::new(0, 0),
            system: IVec2::new(3, -2),
            orbit: 2,
            satellite: None,
        };
        let a = CelestialSeeds::derive(UNIVERSE_SEED, &addr_a);
        let b = CelestialSeeds::derive(UNIVERSE_SEED, &addr_b);
        // Same system → same star_seed
        assert_eq!(a.star_seed, b.star_seed);
        // Different orbit → different body_seed
        assert_ne!(a.body_seed, b.body_seed);
    }

    #[test]
    fn moon_differs_from_planet() {
        let planet = CelestialAddress {
            galaxy: IVec2::new(0, 0),
            system: IVec2::new(1, 1),
            orbit: 3,
            satellite: None,
        };
        let moon = CelestialAddress {
            galaxy: IVec2::new(0, 0),
            system: IVec2::new(1, 1),
            orbit: 3,
            satellite: Some(0),
        };
        let p = CelestialSeeds::derive(UNIVERSE_SEED, &planet);
        let m = CelestialSeeds::derive(UNIVERSE_SEED, &moon);
        assert_ne!(p.body_seed, m.body_seed);
    }

    #[test]
    fn different_galaxy_different_seeds() {
        let addr_a = CelestialAddress {
            galaxy: IVec2::new(0, 0),
            system: IVec2::new(1, 1),
            orbit: 0,
            satellite: None,
        };
        let addr_b = CelestialAddress {
            galaxy: IVec2::new(1, 0),
            system: IVec2::new(1, 1),
            orbit: 0,
            satellite: None,
        };
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
