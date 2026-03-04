//! World persistence — saves and restores per-planet modifications across warps.
//!
//! [`Universe`] is the global store keyed by [`CelestialAddress`].
//! Each visited world has a [`WorldSave`] containing only dirty (player-modified)
//! chunks and dropped items. Unmodified terrain is regenerated from seed.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use bevy::sprite_render::MeshMaterial2d;
use serde::{Deserialize, Serialize};

use super::address::CelestialAddress;
use crate::item::{DroppedItem, ItemRegistry};
use crate::physics::{Bounce, Friction, Gravity, Grounded, TileCollider, Velocity};
use crate::ui::game_ui::icon_registry::ItemIconRegistry;
use crate::world::chunk::{ChunkData, WorldMap};
use crate::world::lit_sprite::{
    FallbackItemImage, FallbackLightmap, LitSprite, LitSpriteMaterial, SharedLitQuad,
};

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
// Save / Load
// ---------------------------------------------------------------------------

/// Save the current world's dirty chunks and dropped items into Universe.
///
/// Called during warp before clearing world data.
pub fn save_current_world(
    universe: &mut Universe,
    address: &CelestialAddress,
    world_map: &WorldMap,
    dirty_chunks: &DirtyChunks,
    dropped_items: Vec<SavedDroppedItem>,
    game_time: f64,
) {
    let save = universe.planets.entry(address.clone()).or_default();

    // Save dirty chunks (overwrite previous save for this world)
    save.chunks.clear();
    for &(cx, cy) in &dirty_chunks.0 {
        if let Some(chunk_data) = world_map.chunks.get(&(cx, cy)) {
            save.chunks.insert((cx, cy), chunk_data.clone());
        }
    }

    // Save dropped items
    save.dropped_items = dropped_items;

    // Record departure time
    save.left_at = Some(game_time);
}

/// Pre-populate WorldMap with saved dirty chunks and return filtered dropped items.
///
/// Called during warp after clearing world data but before chunk generation.
/// Returns dropped items with elapsed time subtracted (expired items filtered out).
pub fn load_world_save(
    universe: &Universe,
    address: &CelestialAddress,
    world_map: &mut WorldMap,
    dirty_chunks: &mut DirtyChunks,
    current_game_time: f64,
) -> Vec<SavedDroppedItem> {
    dirty_chunks.0.clear();

    let Some(save) = universe.planets.get(address) else {
        return Vec::new();
    };

    // Pre-populate dirty chunks into WorldMap
    for (&coords, chunk_data) in &save.chunks {
        world_map.chunks.insert(coords, chunk_data.clone());
        dirty_chunks.0.insert(coords);
    }

    // Compute elapsed time and filter dropped items
    let elapsed = save
        .left_at
        .map(|t| (current_game_time - t) as f32)
        .unwrap_or(0.0)
        .max(0.0);

    save.dropped_items
        .iter()
        .filter_map(|item| {
            let remaining = item.remaining_secs - elapsed;
            if remaining > 0.0 {
                Some(SavedDroppedItem {
                    remaining_secs: remaining,
                    ..item.clone()
                })
            } else {
                None
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Respawn saved dropped items
// ---------------------------------------------------------------------------

/// Dropped item display size in pixels (icons are 16×16).
const DROPPED_ITEM_SIZE: f32 = 16.0;
/// Fallback size for items without an icon.
const DROPPED_ITEM_FALLBACK_SIZE: f32 = 8.0;

/// Respawn saved dropped items after arriving on a world via warp.
///
/// Runs on `OnEnter(InGame)`. Consumes [`PendingDroppedItems`] and spawns
/// grounded [`DroppedItem`] entities with their remaining lifetime timers.
/// Items are spawned at their saved positions with no velocity (already
/// grounded), using the same `LitSpriteMaterial` setup as fresh tile drops.
#[allow(clippy::too_many_arguments)]
pub fn respawn_saved_dropped_items(
    mut commands: Commands,
    pending: Res<PendingDroppedItems>,
    item_registry: Res<ItemRegistry>,
    icon_registry: Res<ItemIconRegistry>,
    quad: Res<SharedLitQuad>,
    fallback_lm: Res<FallbackLightmap>,
    fallback_img: Res<FallbackItemImage>,
    mut lit_materials: ResMut<Assets<LitSpriteMaterial>>,
) {
    if pending.0.is_empty() {
        commands.remove_resource::<PendingDroppedItems>();
        return;
    }

    info!("Respawning {} saved dropped items", pending.0.len());

    for saved in &pending.0 {
        // Resolve sprite texture from icon registry
        let (sprite_image, size) = item_registry
            .by_name(&saved.item_id)
            .and_then(|id| icon_registry.get(id).cloned())
            .map(|img| (img, DROPPED_ITEM_SIZE))
            .unwrap_or_else(|| (fallback_img.0.clone(), DROPPED_ITEM_FALLBACK_SIZE));

        let material = lit_materials.add(LitSpriteMaterial {
            sprite: sprite_image,
            lightmap: fallback_lm.0.clone(),
            lightmap_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
            sprite_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
        });

        commands.spawn((
            DroppedItem {
                item_id: saved.item_id.clone(),
                count: saved.count,
                lifetime: Timer::from_seconds(saved.remaining_secs, TimerMode::Once),
            },
            LitSprite,
            Velocity::default(),
            Gravity(400.0),
            Grounded(true),
            TileCollider {
                width: 4.0,
                height: 4.0,
            },
            Friction(0.9),
            Bounce(0.3),
            Mesh2d(quad.0.clone()),
            MeshMaterial2d(material),
            Transform::from_translation(Vec3::new(saved.x, saved.y, 1.0))
                .with_scale(Vec3::new(size, size, 1.0)),
        ));
    }

    commands.remove_resource::<PendingDroppedItems>();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::{FluidCell, FluidId};
    use crate::registry::tile::TileId;
    use crate::world::chunk::TileLayer;
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

    #[test]
    fn save_and_load_round_trip() {
        let mut universe = Universe::default();
        let addr = test_address();
        let mut world_map = WorldMap::default();
        let mut dirty = DirtyChunks::default();

        // Mark chunk (0, 0) as dirty and put some data in WorldMap
        dirty.0.insert((0, 0));
        // We can't easily create a full ChunkData without terrain gen,
        // but we can verify the save/load mechanism works with the HashMap
        // by checking that after save+clear+load, dirty chunks are restored.

        save_current_world(&mut universe, &addr, &world_map, &dirty, vec![], 100.0);
        assert!(universe.planets.contains_key(&addr));
        assert_eq!(universe.planets[&addr].left_at, Some(100.0));

        // Clear and reload
        world_map.chunks.clear();
        dirty.0.clear();
        let items = load_world_save(&universe, &addr, &mut world_map, &mut dirty, 200.0);
        assert!(items.is_empty()); // no dropped items saved
    }

    #[test]
    fn load_subtracts_elapsed_from_dropped_items() {
        let mut universe = Universe::default();
        let addr = test_address();
        universe.planets.insert(
            addr.clone(),
            WorldSave {
                chunks: HashMap::new(),
                dropped_items: vec![
                    SavedDroppedItem {
                        item_id: "dirt".into(),
                        count: 5,
                        x: 100.0,
                        y: 200.0,
                        remaining_secs: 600.0,
                    },
                    SavedDroppedItem {
                        item_id: "stone".into(),
                        count: 1,
                        x: 50.0,
                        y: 50.0,
                        remaining_secs: 100.0,
                    },
                ],
                left_at: Some(1000.0),
            },
        );

        let mut world_map = WorldMap::default();
        let mut dirty = DirtyChunks::default();

        // 200 seconds elapsed
        let items = load_world_save(&universe, &addr, &mut world_map, &mut dirty, 1200.0);
        assert_eq!(items.len(), 1); // stone expired (100 - 200 < 0)
        assert_eq!(items[0].item_id, "dirt");
        assert!((items[0].remaining_secs - 400.0).abs() < 0.1);
    }

    #[test]
    fn load_nonexistent_world_returns_empty() {
        let universe = Universe::default();
        let addr = test_address();
        let mut world_map = WorldMap::default();
        let mut dirty = DirtyChunks::default();
        let items = load_world_save(&universe, &addr, &mut world_map, &mut dirty, 0.0);
        assert!(items.is_empty());
        assert!(dirty.0.is_empty());
    }

    #[test]
    fn save_dropped_items_preserved() {
        let mut universe = Universe::default();
        let addr = test_address();
        let world_map = WorldMap::default();
        let dirty = DirtyChunks::default();

        let items = vec![SavedDroppedItem {
            item_id: "torch".into(),
            count: 3,
            x: 50.0,
            y: 100.0,
            remaining_secs: 1000.0,
        }];

        save_current_world(&mut universe, &addr, &world_map, &dirty, items, 500.0);

        let save = &universe.planets[&addr];
        assert_eq!(save.dropped_items.len(), 1);
        assert_eq!(save.dropped_items[0].item_id, "torch");
        assert_eq!(save.dropped_items[0].count, 3);
    }

    #[test]
    fn multiple_worlds_independent() {
        let mut universe = Universe::default();
        let addr1 = CelestialAddress::planet(IVec2::ZERO, IVec2::ZERO, 1);
        let addr2 = CelestialAddress::planet(IVec2::ZERO, IVec2::ZERO, 2);

        let world_map = WorldMap::default();
        let dirty = DirtyChunks::default();

        let items1 = vec![SavedDroppedItem {
            item_id: "dirt".into(),
            count: 1,
            x: 0.0,
            y: 0.0,
            remaining_secs: 500.0,
        }];
        let items2 = vec![SavedDroppedItem {
            item_id: "stone".into(),
            count: 2,
            x: 10.0,
            y: 10.0,
            remaining_secs: 300.0,
        }];

        save_current_world(&mut universe, &addr1, &world_map, &dirty, items1, 100.0);
        save_current_world(&mut universe, &addr2, &world_map, &dirty, items2, 100.0);

        assert_eq!(universe.planets.len(), 2);
        assert_eq!(universe.planets[&addr1].dropped_items[0].item_id, "dirt");
        assert_eq!(universe.planets[&addr2].dropped_items[0].item_id, "stone");
    }

    #[test]
    fn dropped_items_timer_frozen_while_away() {
        // If elapsed == 0 (load at same game_time as left_at), timer stays intact.
        let mut universe = Universe::default();
        let addr = test_address();
        universe.planets.insert(
            addr.clone(),
            WorldSave {
                chunks: HashMap::new(),
                dropped_items: vec![SavedDroppedItem {
                    item_id: "torch".into(),
                    count: 1,
                    x: 0.0,
                    y: 0.0,
                    remaining_secs: 600.0,
                }],
                left_at: Some(1000.0),
            },
        );

        let mut world_map = WorldMap::default();
        let mut dirty = DirtyChunks::default();

        // Return at the exact same game time — no elapsed time
        let items = load_world_save(&universe, &addr, &mut world_map, &mut dirty, 1000.0);
        assert_eq!(items.len(), 1);
        assert!((items[0].remaining_secs - 600.0).abs() < f32::EPSILON);
    }

    #[test]
    fn dirty_chunks_cleared_and_repopulated_on_load() {
        let mut universe = Universe::default();
        let addr = test_address();

        // Pre-populate universe with saved chunks at (3, 4) and (5, 6)
        let chunk_size = 32;
        let len = (chunk_size * chunk_size) as usize;
        let chunk_a = ChunkData {
            fg: TileLayer::new_air(len),
            bg: TileLayer::new_air(len),
            fluids: vec![FluidCell::EMPTY; len],
            objects: Vec::new(),
            occupancy: vec![None; len],
            damage: vec![0; len],
        };
        let chunk_b = chunk_a.clone();

        let mut save = WorldSave::default();
        save.chunks.insert((3, 4), chunk_a);
        save.chunks.insert((5, 6), chunk_b);
        save.left_at = Some(100.0);
        universe.planets.insert(addr.clone(), save);

        let mut world_map = WorldMap::default();
        let mut dirty = DirtyChunks::default();

        // Dirty chunks initially has unrelated data
        dirty.0.insert((99, 99));

        load_world_save(&universe, &addr, &mut world_map, &mut dirty, 200.0);

        // Dirty chunks should contain exactly the saved coords, not the old one
        assert_eq!(dirty.0.len(), 2);
        assert!(dirty.0.contains(&(3, 4)));
        assert!(dirty.0.contains(&(5, 6)));
        assert!(!dirty.0.contains(&(99, 99)));
    }

    #[test]
    fn round_trip_chunk_data_preserved() {
        let mut universe = Universe::default();
        let addr = test_address();
        let chunk_size = 32;
        let len = (chunk_size * chunk_size) as usize;

        // Create a chunk with a non-air tile at position (1, 2)
        let mut fg = TileLayer::new_air(len);
        fg.set(1, 2, TileId(42), chunk_size);

        let chunk = ChunkData {
            fg,
            bg: TileLayer::new_air(len),
            fluids: vec![FluidCell::EMPTY; len],
            objects: Vec::new(),
            occupancy: vec![None; len],
            damage: vec![0; len],
        };

        let mut world_map = WorldMap::default();
        world_map.chunks.insert((0, 0), chunk);

        let mut dirty = DirtyChunks::default();
        dirty.0.insert((0, 0));

        // Save
        save_current_world(&mut universe, &addr, &world_map, &dirty, vec![], 100.0);

        // Clear world state
        world_map.chunks.clear();
        dirty.0.clear();
        assert!(world_map.chunk(0, 0).is_none());

        // Load
        load_world_save(&universe, &addr, &mut world_map, &mut dirty, 200.0);

        // Verify chunk data was restored with the modified tile
        let restored = world_map.chunk(0, 0).expect("chunk should be restored");
        assert_eq!(restored.fg.get(1, 2, chunk_size), TileId(42));
        assert_eq!(restored.fg.get(0, 0, chunk_size), TileId::AIR);
    }

    #[test]
    fn save_overwrites_previous_save() {
        let mut universe = Universe::default();
        let addr = test_address();
        let world_map = WorldMap::default();
        let dirty = DirtyChunks::default();

        let items_v1 = vec![SavedDroppedItem {
            item_id: "dirt".into(),
            count: 1,
            x: 0.0,
            y: 0.0,
            remaining_secs: 500.0,
        }];
        save_current_world(&mut universe, &addr, &world_map, &dirty, items_v1, 100.0);
        assert_eq!(universe.planets[&addr].dropped_items.len(), 1);
        assert_eq!(universe.planets[&addr].left_at, Some(100.0));

        // Save again with different data — should overwrite
        let items_v2 = vec![
            SavedDroppedItem {
                item_id: "stone".into(),
                count: 2,
                x: 10.0,
                y: 10.0,
                remaining_secs: 300.0,
            },
            SavedDroppedItem {
                item_id: "wood".into(),
                count: 5,
                x: 20.0,
                y: 20.0,
                remaining_secs: 200.0,
            },
        ];
        save_current_world(&mut universe, &addr, &world_map, &dirty, items_v2, 500.0);

        assert_eq!(universe.planets[&addr].dropped_items.len(), 2);
        assert_eq!(universe.planets[&addr].dropped_items[0].item_id, "stone");
        assert_eq!(universe.planets[&addr].left_at, Some(500.0));
    }

    #[test]
    fn load_with_no_left_at_treats_elapsed_as_zero() {
        let mut universe = Universe::default();
        let addr = test_address();
        universe.planets.insert(
            addr.clone(),
            WorldSave {
                chunks: HashMap::new(),
                dropped_items: vec![SavedDroppedItem {
                    item_id: "coal".into(),
                    count: 3,
                    x: 0.0,
                    y: 0.0,
                    remaining_secs: 900.0,
                }],
                left_at: None, // no departure time recorded
            },
        );

        let mut world_map = WorldMap::default();
        let mut dirty = DirtyChunks::default();

        let items = load_world_save(&universe, &addr, &mut world_map, &mut dirty, 99999.0);
        assert_eq!(items.len(), 1);
        // With no left_at, elapsed is 0, so remaining should be unchanged
        assert!((items[0].remaining_secs - 900.0).abs() < f32::EPSILON);
    }

    #[test]
    fn save_only_dirty_chunks_not_all() {
        let mut universe = Universe::default();
        let addr = test_address();
        let chunk_size = 32;
        let len = (chunk_size * chunk_size) as usize;

        let chunk = ChunkData {
            fg: TileLayer::new_air(len),
            bg: TileLayer::new_air(len),
            fluids: vec![FluidCell::EMPTY; len],
            objects: Vec::new(),
            occupancy: vec![None; len],
            damage: vec![0; len],
        };

        let mut world_map = WorldMap::default();
        // WorldMap has chunks at (0,0), (1,0), (2,0)
        world_map.chunks.insert((0, 0), chunk.clone());
        world_map.chunks.insert((1, 0), chunk.clone());
        world_map.chunks.insert((2, 0), chunk);

        // But only (1, 0) is dirty
        let mut dirty = DirtyChunks::default();
        dirty.0.insert((1, 0));

        save_current_world(&mut universe, &addr, &world_map, &dirty, vec![], 100.0);

        // Only the dirty chunk should be saved
        let save = &universe.planets[&addr];
        assert_eq!(save.chunks.len(), 1);
        assert!(save.chunks.contains_key(&(1, 0)));
        assert!(!save.chunks.contains_key(&(0, 0)));
        assert!(!save.chunks.contains_key(&(2, 0)));
    }

    #[test]
    fn round_trip_preserves_fluid_data() {
        let mut universe = Universe::default();
        let addr = test_address();
        let chunk_size = 32;
        let len = (chunk_size * chunk_size) as usize;

        let mut fluids = vec![FluidCell::EMPTY; len];
        fluids[0] = FluidCell::new(FluidId(1), 0.8);
        fluids[5] = FluidCell::new(FluidId(2), 1.5);

        let chunk = ChunkData {
            fg: TileLayer::new_air(len),
            bg: TileLayer::new_air(len),
            fluids,
            objects: Vec::new(),
            occupancy: vec![None; len],
            damage: vec![0; len],
        };

        let mut world_map = WorldMap::default();
        world_map.chunks.insert((0, 0), chunk);

        let mut dirty = DirtyChunks::default();
        dirty.0.insert((0, 0));

        // Save
        save_current_world(&mut universe, &addr, &world_map, &dirty, vec![], 100.0);

        // Clear and reload
        world_map.chunks.clear();
        dirty.0.clear();
        load_world_save(&universe, &addr, &mut world_map, &mut dirty, 200.0);

        let restored = world_map.chunk(0, 0).expect("chunk should be restored");
        assert_eq!(restored.fluids[0].fluid_id(), FluidId(1));
        assert!((restored.fluids[0].mass() - 0.8).abs() < f32::EPSILON);
        assert_eq!(restored.fluids[5].fluid_id(), FluidId(2));
        assert!((restored.fluids[5].mass() - 1.5).abs() < f32::EPSILON);
        assert!(restored.fluids[1].is_empty());
    }

    #[test]
    fn old_save_without_fluids_deserializes_with_empty_vec() {
        // Simulate an old save format: ChunkData RON without a `fluids` field.
        // Thanks to #[serde(default)], the `fluids` field should default to an empty Vec.
        let ron_str = r#"(
            fg: (tiles: [], bitmasks: []),
            bg: (tiles: [], bitmasks: []),
            objects: [],
            occupancy: [],
            damage: [],
        )"#;
        let chunk: ChunkData = ron::from_str(ron_str).unwrap();
        assert!(chunk.fluids.is_empty());
    }
}
