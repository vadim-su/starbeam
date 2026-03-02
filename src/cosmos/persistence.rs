//! World persistence — saves and restores per-planet modifications across warps.
//!
//! [`Universe`] is the global store keyed by [`CelestialAddress`].
//! Each visited world has a [`WorldSave`] containing only dirty (player-modified)
//! chunks and dropped items. Unmodified terrain is regenerated from seed.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::address::CelestialAddress;
use crate::world::chunk::ChunkData;

// ---------------------------------------------------------------------------
// Saved dropped item
// ---------------------------------------------------------------------------

/// A dropped item serialized for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedDroppedItem {
    pub item_id: String,
    pub count: u16,
    pub x: f32,
    pub y: f32,
    /// Remaining lifetime in seconds (max [`DROPPED_ITEM_LIFETIME_SECS`]).
    pub remaining_secs: f32,
}

/// Maximum lifetime for dropped items (30 minutes).
pub const DROPPED_ITEM_LIFETIME_SECS: f32 = 1800.0;

// ---------------------------------------------------------------------------
// Per-world save
// ---------------------------------------------------------------------------

/// Saved state of a single world (planet, moon, station, etc.).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct WorldSave {
    /// Only chunks modified by the player (dirty).
    /// Unmodified chunks are regenerated deterministically from seed.
    pub chunks: HashMap<(i32, i32), ChunkData>,
    /// Dropped items on the ground.
    pub dropped_items: Vec<SavedDroppedItem>,
    /// Game time when the player left this world (for offline simulation).
    pub left_at: Option<f64>,
}

// ---------------------------------------------------------------------------
// Universe (global store)
// ---------------------------------------------------------------------------

/// Global persistence store for all visited worlds.
///
/// Keyed by [`CelestialAddress`] — works for planets, moons, stations, etc.
/// All types derive `Serialize`/`Deserialize` for future disk persistence.
#[derive(Resource, Debug, Default, Serialize, Deserialize)]
pub struct Universe {
    pub planets: HashMap<CelestialAddress, WorldSave>,
}

// ---------------------------------------------------------------------------
// Dirty tracking
// ---------------------------------------------------------------------------

/// Tracks which chunks have been modified by the player on the current planet.
///
/// Populated by `set_tile()`, `place_object()`, `remove_object()`.
/// Used during warp to determine which chunks to save.
#[derive(Resource, Debug, Default)]
pub struct DirtyChunks(pub HashSet<(i32, i32)>);

// ---------------------------------------------------------------------------
// Pending dropped items (for respawn after warp)
// ---------------------------------------------------------------------------

/// Resource holding dropped items to respawn after arriving on a world.
///
/// Inserted during warp, consumed by `respawn_saved_dropped_items` on
/// `OnEnter(InGame)`.
#[derive(Resource, Default)]
pub struct PendingDroppedItems(pub Vec<SavedDroppedItem>);

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::IVec2;

    fn test_address() -> CelestialAddress {
        CelestialAddress::planet(IVec2::ZERO, IVec2::ZERO, 2)
    }

    #[test]
    fn universe_default_is_empty() {
        let u = Universe::default();
        assert!(u.planets.is_empty());
    }

    #[test]
    fn world_save_default_is_empty() {
        let s = WorldSave::default();
        assert!(s.chunks.is_empty());
        assert!(s.dropped_items.is_empty());
        assert!(s.left_at.is_none());
    }

    #[test]
    fn dirty_chunks_default_is_empty() {
        let d = DirtyChunks::default();
        assert!(d.0.is_empty());
    }

    #[test]
    fn universe_insert_and_get() {
        let mut u = Universe::default();
        let addr = test_address();
        u.planets.insert(addr.clone(), WorldSave::default());
        assert!(u.planets.contains_key(&addr));
        assert_eq!(u.planets.len(), 1);
    }

    #[test]
    fn different_addresses_are_independent() {
        let mut u = Universe::default();
        let a1 = CelestialAddress::planet(IVec2::ZERO, IVec2::ZERO, 1);
        let a2 = CelestialAddress::planet(IVec2::ZERO, IVec2::ZERO, 2);
        u.planets.insert(a1.clone(), WorldSave::default());
        u.planets.insert(a2.clone(), WorldSave::default());
        assert_eq!(u.planets.len(), 2);
    }

    #[test]
    fn dropped_item_lifetime_constant() {
        assert_eq!(DROPPED_ITEM_LIFETIME_SECS, 1800.0);
    }
}
