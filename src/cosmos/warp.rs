//! Planet warp system — switches the active world to a different body in the
//! current star system.
//!
//! Flow: `WarpToBody` event → save current world → clear world state → rebuild
//! `ActiveWorld` + `DayNightConfig` → transition to `LoadingBiomes` to reload
//! biome pipeline.

use bevy::prelude::*;

use crate::cosmos::address::{CelestialAddress, CelestialSeeds};
use crate::cosmos::current::CurrentSystem;
use crate::cosmos::ship_location::{GlobalBiome, ShipLocation};
use crate::cosmos::persistence::{
    save_current_world, DirtyChunks, PendingDroppedItems, SavedDroppedItem, Universe,
};
use crate::item::DroppedItem;
use crate::object::spawn::PlacedObjectEntity;
use crate::parallax::spawn::{ParallaxLayerConfig, ParallaxTile};
use crate::registry::world::ActiveWorld;
use crate::registry::AppState;
use crate::world::chunk::{ChunkCoord, LoadedChunks, WorldMap};
use crate::world::day_night::WorldTime;
use crate::world::rc_lighting::{RcInputData, RcLightingConfig};
use crate::world::terrain_gen::TerrainNoiseCache;

use crate::registry::loading::LoadingBiomeAssets;

/// Message requesting a warp to a specific orbit index.
#[derive(Message, Debug)]
pub struct WarpToBody {
    pub orbit: u32,
}

/// Message requesting a warp to the player's ship.
#[derive(Message, Debug)]
pub struct WarpToShip;

/// Marker resource: when present, the player should be teleported to the
/// surface of the new world on the next `InGame` enter.
#[derive(Resource)]
pub struct NeedsRespawn;

/// System that handles planet warping.
#[allow(clippy::too_many_arguments)]
pub fn handle_warp(
    mut commands: Commands,
    mut warp_events: bevy::ecs::message::MessageReader<WarpToBody>,
    current_system: Res<CurrentSystem>,
    mut world_map: ResMut<WorldMap>,
    mut loaded_chunks: ResMut<LoadedChunks>,
    chunk_entities: Query<Entity, With<ChunkCoord>>,
    parallax_entities: Query<Entity, With<ParallaxLayerConfig>>,
    parallax_tiles: Query<Entity, With<ParallaxTile>>,
    asset_server: Res<AssetServer>,
    mut next_state: ResMut<NextState<AppState>>,
    rc_state: (ResMut<RcLightingConfig>, ResMut<RcInputData>),
    persistence: (ResMut<Universe>, Res<DirtyChunks>),
    despawn_queries: (
        Query<Entity, With<PlacedObjectEntity>>,
        Query<Entity, With<DroppedItem>>,
    ),
    dropped_items_for_save: Query<(&DroppedItem, &Transform)>,
    active_world: Option<Res<ActiveWorld>>,
    time: Res<Time>,
) {
    let Some(warp) = warp_events.read().last() else {
        return;
    };

    let Some(body) = current_system
        .system
        .bodies
        .iter()
        .find(|b| b.address.orbit() == Some(warp.orbit))
    else {
        warn!("WarpToBody: no body at orbit {}", warp.orbit);
        return;
    };

    info!(
        "Warping to orbit {} — {} ({}×{})",
        body.address.orbit().unwrap_or(0),
        body.planet_type_id,
        body.width_tiles,
        body.height_tiles
    );

    let (mut rc_config, mut rc_input) = rc_state;
    let (mut universe, dirty_chunks) = persistence;
    let (object_entity_query, dropped_entity_query) = despawn_queries;

    // --- 0. SAVE current world ---
    let game_time = time.elapsed_secs_f64();

    if let Some(ref current_active) = active_world {
        let dropped_items_to_save: Vec<SavedDroppedItem> = dropped_items_for_save
            .iter()
            .map(|(item, transform)| SavedDroppedItem {
                item_id: item.item_id.clone(),
                count: item.count,
                x: transform.translation.x,
                y: transform.translation.y,
                remaining_secs: item.lifetime.remaining_secs(),
            })
            .collect();

        save_current_world(
            &mut universe,
            &current_active.address,
            &world_map,
            &dirty_chunks,
            dropped_items_to_save,
            game_time,
        );
    }

    // --- 1. Despawn all chunk entities ---
    for entity in &chunk_entities {
        commands.entity(entity).despawn();
    }
    // --- 2. Despawn all parallax entities ---
    for entity in &parallax_tiles {
        commands.entity(entity).despawn();
    }
    for entity in &parallax_entities {
        commands.entity(entity).despawn();
    }
    // --- 3. Despawn object entities (BUG FIX: these survived warp before) ---
    for entity in &object_entity_query {
        commands.entity(entity).despawn();
    }
    // --- 4. Despawn dropped item entities ---
    for entity in &dropped_entity_query {
        commands.entity(entity).despawn();
    }

    // --- 5. Clear world data ---
    world_map.chunks.clear();
    loaded_chunks.map.clear();

    // Remove ship-specific resources when warping away from a ship.
    // GlobalBiome will be re-inserted by check_biomes_loaded if the
    // destination is also a ship world.
    if let Some(ref aw) = active_world {
        if matches!(aw.address, CelestialAddress::Ship { .. }) {
            commands.remove_resource::<GlobalBiome>();
            commands.remove_resource::<ShipLocation>();
        }
    }

    // --- 6. Compute pending dropped items for the destination world ---
    let pending_items = if let Some(save) = universe.planets.get(&body.address) {
        let elapsed = save
            .left_at
            .map(|t| (game_time - t) as f32)
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
    } else {
        Vec::new()
    };
    commands.insert_resource(PendingDroppedItems(pending_items));

    // --- 7. Rebuild ActiveWorld ---
    let seeds = CelestialSeeds::derive(current_system.universe_seed, &body.address);
    let new_active_world = ActiveWorld {
        address: body.address.clone(),
        seeds: seeds.clone(),
        width_tiles: body.width_tiles,
        height_tiles: body.height_tiles,
        chunk_size: current_system.chunk_size,
        tile_size: current_system.tile_size,
        chunk_load_radius: current_system.chunk_load_radius,
        seed: seeds.terrain_seed_u32(),
        planet_type: body.planet_type_id.clone(),
        wrap_x: body.wrap_x,
    };
    commands.insert_resource(TerrainNoiseCache::new(new_active_world.seed));
    commands.insert_resource(new_active_world);

    // --- 8. Rebuild DayNightConfig + WorldTime ---
    let day_night_config = body.day_night.clone();
    let wt = WorldTime::from_config(&day_night_config);
    commands.insert_resource(day_night_config);
    commands.insert_resource(wt);

    // --- 9. Kick off biome loading for the new planet ---
    let planet_handle = asset_server.load::<crate::registry::assets::PlanetTypeAsset>(format!(
        "worlds/planet_types/{0}/{0}.planet.ron",
        body.planet_type_id
    ));
    commands.insert_resource(LoadingBiomeAssets {
        planet_type: planet_handle,
        biomes: Vec::new(),
        parallax_configs: Vec::new(),
    });

    // --- 10. Reset RC lighting state ---
    *rc_config = RcLightingConfig::default();
    *rc_input = RcInputData::default();

    // --- 11. Track ship location when warping to a ship world ---
    if matches!(body.address, CelestialAddress::Ship { .. }) {
        if let Some(ref aw) = active_world {
            commands.insert_resource(ShipLocation::Orbit(aw.address.clone()));
        }
    }

    // --- 12. Mark player for respawn on new world surface ---
    commands.insert_resource(NeedsRespawn);

    // --- 13. Transition to LoadingBiomes state ---
    next_state.set(AppState::LoadingBiomes);

    info!(
        "Warp complete — loading biomes for {} at orbit {}",
        body.planet_type_id,
        body.address.orbit().unwrap_or(0)
    );
}

/// System that handles warping to the player's ship.
///
/// Ships are not part of the star system's generated bodies, so this handler
/// constructs the ship's `GeneratedBody` manually and performs the same
/// save-clear-rebuild cycle as `handle_warp`.
#[allow(clippy::too_many_arguments)]
pub fn handle_warp_to_ship(
    mut commands: Commands,
    mut warp_events: bevy::ecs::message::MessageReader<WarpToShip>,
    current_system: Res<CurrentSystem>,
    mut world_map: ResMut<WorldMap>,
    mut loaded_chunks: ResMut<LoadedChunks>,
    chunk_entities: Query<Entity, With<ChunkCoord>>,
    parallax_entities: Query<Entity, With<ParallaxLayerConfig>>,
    parallax_tiles: Query<Entity, With<ParallaxTile>>,
    asset_server: Res<AssetServer>,
    mut next_state: ResMut<NextState<AppState>>,
    rc_state: (ResMut<RcLightingConfig>, ResMut<RcInputData>),
    persistence: (ResMut<Universe>, Res<DirtyChunks>),
    despawn_queries: (
        Query<Entity, With<PlacedObjectEntity>>,
        Query<Entity, With<DroppedItem>>,
    ),
    dropped_items_for_save: Query<(&DroppedItem, &Transform)>,
    active_world: Option<Res<ActiveWorld>>,
    time: Res<Time>,
) {
    if warp_events.read().last().is_none() {
        return;
    }

    let ship_address = CelestialAddress::Ship { owner_id: 0 };
    let ship_planet_type = "ship".to_string();
    let ship_width = 128;
    let ship_height = 64;

    info!(
        "Warping to ship — {} ({}×{})",
        ship_planet_type, ship_width, ship_height
    );

    let (mut rc_config, mut rc_input) = rc_state;
    let (mut universe, dirty_chunks) = persistence;
    let (object_entity_query, dropped_entity_query) = despawn_queries;

    // --- 0. SAVE current world ---
    let game_time = time.elapsed_secs_f64();

    if let Some(ref current_active) = active_world {
        let dropped_items_to_save: Vec<SavedDroppedItem> = dropped_items_for_save
            .iter()
            .map(|(item, transform)| SavedDroppedItem {
                item_id: item.item_id.clone(),
                count: item.count,
                x: transform.translation.x,
                y: transform.translation.y,
                remaining_secs: item.lifetime.remaining_secs(),
            })
            .collect();

        save_current_world(
            &mut universe,
            &current_active.address,
            &world_map,
            &dirty_chunks,
            dropped_items_to_save,
            game_time,
        );
    }

    // --- 1. Despawn all chunk entities ---
    for entity in &chunk_entities {
        commands.entity(entity).despawn();
    }
    // --- 2. Despawn all parallax entities ---
    for entity in &parallax_tiles {
        commands.entity(entity).despawn();
    }
    for entity in &parallax_entities {
        commands.entity(entity).despawn();
    }
    // --- 3. Despawn object entities ---
    for entity in &object_entity_query {
        commands.entity(entity).despawn();
    }
    // --- 4. Despawn dropped item entities ---
    for entity in &dropped_entity_query {
        commands.entity(entity).despawn();
    }

    // --- 5. Clear world data ---
    world_map.chunks.clear();
    loaded_chunks.map.clear();

    // Remove ship-specific resources from previous ship (if any)
    if let Some(ref aw) = active_world {
        if matches!(aw.address, CelestialAddress::Ship { .. }) {
            commands.remove_resource::<GlobalBiome>();
            commands.remove_resource::<ShipLocation>();
        }
    }

    // --- 6. Compute pending dropped items for the ship world ---
    let pending_items = if let Some(save) = universe.planets.get(&ship_address) {
        let elapsed = save
            .left_at
            .map(|t| (game_time - t) as f32)
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
    } else {
        Vec::new()
    };
    commands.insert_resource(PendingDroppedItems(pending_items));

    // --- 7. Rebuild ActiveWorld for the ship ---
    let seeds = CelestialSeeds::derive(current_system.universe_seed, &ship_address);
    let new_active_world = ActiveWorld {
        address: ship_address,
        seeds: seeds.clone(),
        width_tiles: ship_width,
        height_tiles: ship_height,
        chunk_size: current_system.chunk_size,
        tile_size: current_system.tile_size,
        chunk_load_radius: current_system.chunk_load_radius,
        seed: seeds.terrain_seed_u32(),
        planet_type: ship_planet_type.clone(),
        wrap_x: false,
    };
    commands.insert_resource(TerrainNoiseCache::new(new_active_world.seed));
    commands.insert_resource(new_active_world);

    // --- 8. DayNightConfig for ship (permanent "day" lighting) ---
    let day_night_config = crate::world::day_night::DayNightConfig {
        cycle_duration_secs: 3600.0,
        dawn_ratio: 0.0,
        day_ratio: 1.0,
        sunset_ratio: 0.0,
        night_ratio: 0.0,
        sun_colors: [[1.0, 1.0, 1.0]; 4],
        sun_intensities: [0.8; 4],
        ambient_mins: [0.3; 4],
        sky_colors: [[0.0, 0.0, 0.0, 1.0]; 4],
        danger_multipliers: [0.0; 4],
        temperature_modifiers: [0.0; 4],
    };
    let wt = WorldTime::from_config(&day_night_config);
    commands.insert_resource(day_night_config);
    commands.insert_resource(wt);

    // --- 9. Kick off biome loading for the ship world ---
    let planet_handle = asset_server.load::<crate::registry::assets::PlanetTypeAsset>(format!(
        "worlds/planet_types/{0}/{0}.planet.ron",
        ship_planet_type
    ));
    commands.insert_resource(LoadingBiomeAssets {
        planet_type: planet_handle,
        biomes: Vec::new(),
        parallax_configs: Vec::new(),
    });

    // --- 10. Reset RC lighting state ---
    *rc_config = RcLightingConfig::default();
    *rc_input = RcInputData::default();

    // --- 11. Track ship location: orbiting the planet we just left ---
    if let Some(ref aw) = active_world {
        commands.insert_resource(ShipLocation::Orbit(aw.address.clone()));
    }

    // --- 12. Mark player for respawn on the ship ---
    commands.insert_resource(NeedsRespawn);

    // --- 13. Transition to LoadingBiomes state ---
    next_state.set(AppState::LoadingBiomes);

    info!("Warp to ship complete — loading biomes for ship world");
}
