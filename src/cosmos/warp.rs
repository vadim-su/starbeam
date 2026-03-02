//! Planet warp system — switches the active world to a different body in the
//! current star system.
//!
//! Flow: `WarpToBody` event → clear world state → rebuild `ActiveWorld` +
//! `DayNightConfig` → transition to `LoadingBiomes` to reload biome pipeline.

use bevy::prelude::*;

use crate::cosmos::address::CelestialSeeds;
use crate::cosmos::current::CurrentSystem;
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
    mut rc_config: ResMut<RcLightingConfig>,
    mut rc_input: ResMut<RcInputData>,
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

    // --- 3. Clear world data ---
    world_map.chunks.clear();
    loaded_chunks.map.clear();

    // --- 4. Rebuild ActiveWorld ---
    let seeds = CelestialSeeds::derive(current_system.universe_seed, &body.address);
    let active_world = ActiveWorld {
        address: body.address.clone(),
        seeds: seeds.clone(),
        width_tiles: body.width_tiles,
        height_tiles: body.height_tiles,
        chunk_size: current_system.chunk_size,
        tile_size: current_system.tile_size,
        chunk_load_radius: current_system.chunk_load_radius,
        seed: seeds.terrain_seed_u32(),
        planet_type: body.planet_type_id.clone(),
    };
    commands.insert_resource(TerrainNoiseCache::new(active_world.seed));
    commands.insert_resource(active_world);

    // --- 5. Rebuild DayNightConfig + WorldTime ---
    let day_night_config = body.day_night.clone();
    let wt = WorldTime::from_config(&day_night_config);
    commands.insert_resource(day_night_config);
    commands.insert_resource(wt);

    // --- 6. Kick off biome loading for the new planet ---
    let planet_handle = asset_server.load::<crate::registry::assets::PlanetTypeAsset>(format!(
        "worlds/planet_types/{0}/{0}.planet.ron",
        body.planet_type_id
    ));
    commands.insert_resource(LoadingBiomeAssets {
        planet_type: planet_handle,
        biomes: Vec::new(),
        parallax_configs: Vec::new(),
    });

    // --- 7. Reset RC lighting state ---
    // Zero the input dimensions so the GPU compute node and prepare systems
    // skip during loading (they check input_w/h != 0).  When InGame resumes,
    // extract_lighting_data fills new dimensions and resize_gpu_textures
    // recreates all textures (including fresh white lightmaps), eliminating
    // stale lighting from the previous planet.
    *rc_config = RcLightingConfig::default();
    *rc_input = RcInputData::default();

    // --- 8. Mark player for respawn on new world surface ---
    commands.insert_resource(NeedsRespawn);

    // --- 9. Transition to LoadingBiomes state ---
    next_state.set(AppState::LoadingBiomes);

    info!(
        "Warp complete — loading biomes for {} at orbit {}",
        body.planet_type_id,
        body.address.orbit().unwrap_or(0)
    );
}
