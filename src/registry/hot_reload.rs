//! Hot-reload systems for registry assets.

use bevy::asset::AssetEvent;
use bevy::ecs::message::MessageReader;
use bevy::prelude::*;

use super::assets::{
    BiomeAsset, ParallaxConfigAsset, PlanetTypeAsset, PlayerDefAsset, TileRegistryAsset,
    WorldConfigAsset,
};
use super::biome::{BiomeDef, BiomeId, BiomeRegistry, LayerConfig, LayerConfigs, PlanetConfig};
use super::player::PlayerConfig;
use super::tile::TileRegistry;
use super::world::WorldConfig;
use super::{BiomeParallaxConfigs, RegistryHandles};

use crate::parallax::config::ParallaxConfig;
use crate::world::biome_map::BiomeMap;
use crate::world::terrain_gen::TerrainNoiseCache;

/// Keeps biome-related asset handles alive for hot-reload detection.
#[derive(Resource)]
pub(crate) struct BiomeHandles {
    pub(crate) planet_type: Handle<PlanetTypeAsset>,
    pub(crate) biomes: Vec<(BiomeId, Handle<BiomeAsset>)>,
    pub(crate) parallax_configs: Vec<(BiomeId, Handle<ParallaxConfigAsset>)>,
}

pub(crate) fn hot_reload_player(
    mut events: MessageReader<AssetEvent<PlayerDefAsset>>,
    handles: Res<RegistryHandles>,
    assets: Res<Assets<PlayerDefAsset>>,
    mut config: ResMut<PlayerConfig>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event
            && *id == handles.player.id()
            && let Some(asset) = assets.get(&handles.player)
        {
            config.speed = asset.speed;
            config.jump_velocity = asset.jump_velocity;
            config.gravity = asset.gravity;
            config.width = asset.width;
            config.height = asset.height;
            info!(
                "Hot-reloaded PlayerConfig: speed={}, jump={}, gravity={}",
                asset.speed, asset.jump_velocity, asset.gravity
            );
        }
    }
}

pub(crate) fn hot_reload_world(
    mut events: MessageReader<AssetEvent<WorldConfigAsset>>,
    handles: Res<RegistryHandles>,
    assets: Res<Assets<WorldConfigAsset>>,
    mut config: ResMut<WorldConfig>,
    mut noise_cache: ResMut<TerrainNoiseCache>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event
            && *id == handles.world_config.id()
            && let Some(asset) = assets.get(&handles.world_config)
        {
            config.width_tiles = asset.width_tiles;
            config.height_tiles = asset.height_tiles;
            config.chunk_size = asset.chunk_size;
            config.tile_size = asset.tile_size;
            config.chunk_load_radius = asset.chunk_load_radius;
            config.seed = asset.seed;
            config.planet_type = asset.planet_type.clone();
            *noise_cache = TerrainNoiseCache::new(asset.seed);
            info!("Hot-reloaded WorldConfig + TerrainNoiseCache");
        }
    }
}

pub(crate) fn hot_reload_tiles(
    mut events: MessageReader<AssetEvent<TileRegistryAsset>>,
    handles: Res<RegistryHandles>,
    assets: Res<Assets<TileRegistryAsset>>,
    mut registry: ResMut<TileRegistry>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event
            && *id == handles.tiles.id()
            && let Some(asset) = assets.get(&handles.tiles)
        {
            *registry = TileRegistry::from_defs(asset.tiles.clone());
            info!("Hot-reloaded TileRegistry ({} tiles)", asset.tiles.len());
        }
    }
}

pub(crate) fn hot_reload_biomes(
    mut events: MessageReader<AssetEvent<BiomeAsset>>,
    handles: Res<BiomeHandles>,
    biome_assets: Res<Assets<BiomeAsset>>,
    tile_registry: Res<TileRegistry>,
    mut biome_registry: ResMut<BiomeRegistry>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event {
            for (biome_id, handle) in &handles.biomes {
                if *id == handle.id()
                    && let Some(asset) = biome_assets.get(handle)
                {
                    let name = biome_registry.name_of(*biome_id).to_string();
                    biome_registry.insert(
                        &name,
                        BiomeDef {
                            id: asset.id.clone(),
                            surface_block: tile_registry.by_name(&asset.surface_block),
                            subsurface_block: tile_registry.by_name(&asset.subsurface_block),
                            subsurface_depth: asset.subsurface_depth,
                            fill_block: tile_registry.by_name(&asset.fill_block),
                            cave_threshold: asset.cave_threshold,
                            parallax_path: asset.parallax.clone(),
                        },
                    );
                    info!("Hot-reloaded biome: {name}");
                    break;
                }
            }
        }
    }
}

pub(crate) fn hot_reload_planet_type(
    mut events: MessageReader<AssetEvent<PlanetTypeAsset>>,
    handles: Res<BiomeHandles>,
    planet_assets: Res<Assets<PlanetTypeAsset>>,
    world_config: Res<WorldConfig>,
    biome_registry: Res<BiomeRegistry>,
    mut planet_config: ResMut<PlanetConfig>,
    mut biome_map: ResMut<BiomeMap>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event
            && *id == handles.planet_type.id()
            && let Some(asset) = planet_assets.get(&handles.planet_type)
        {
            planet_config.id = asset.id.clone();
            planet_config.primary_biome = asset.primary_biome.clone();
            planet_config.secondary_biomes = asset.secondary_biomes.clone();
            planet_config.layers = LayerConfigs {
                surface: LayerConfig {
                    primary_biome: asset.layers.surface.primary_biome.clone(),
                    terrain_frequency: asset.layers.surface.terrain_frequency,
                    terrain_amplitude: asset.layers.surface.terrain_amplitude,
                },
                underground: LayerConfig {
                    primary_biome: asset.layers.underground.primary_biome.clone(),
                    terrain_frequency: asset.layers.underground.terrain_frequency,
                    terrain_amplitude: asset.layers.underground.terrain_amplitude,
                },
                deep_underground: LayerConfig {
                    primary_biome: asset.layers.deep_underground.primary_biome.clone(),
                    terrain_frequency: asset.layers.deep_underground.terrain_frequency,
                    terrain_amplitude: asset.layers.deep_underground.terrain_amplitude,
                },
                core: LayerConfig {
                    primary_biome: asset.layers.core.primary_biome.clone(),
                    terrain_frequency: asset.layers.core.terrain_frequency,
                    terrain_amplitude: asset.layers.core.terrain_amplitude,
                },
            };
            planet_config.region_width_min = asset.region_width_min;
            planet_config.region_width_max = asset.region_width_max;
            planet_config.primary_region_ratio = asset.primary_region_ratio;

            // Rebuild BiomeMap with updated planet config
            let secondaries: Vec<&str> = planet_config
                .secondary_biomes
                .iter()
                .map(|s| s.as_str())
                .collect();
            *biome_map = BiomeMap::generate(
                &planet_config.primary_biome,
                &secondaries,
                world_config.seed as u64,
                world_config.width_tiles as u32,
                planet_config.region_width_min,
                planet_config.region_width_max,
                planet_config.primary_region_ratio,
                &biome_registry,
            );
            info!(
                "Hot-reloaded PlanetConfig + BiomeMap ({} regions)",
                biome_map.regions.len()
            );
        }
    }
}

pub(crate) fn hot_reload_biome_parallax(
    mut events: MessageReader<AssetEvent<ParallaxConfigAsset>>,
    handles: Res<BiomeHandles>,
    parallax_assets: Res<Assets<ParallaxConfigAsset>>,
    mut biome_parallax: ResMut<BiomeParallaxConfigs>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event {
            for (biome_id, handle) in &handles.parallax_configs {
                if *id == handle.id()
                    && let Some(asset) = parallax_assets.get(handle)
                {
                    biome_parallax.configs.insert(
                        *biome_id,
                        ParallaxConfig {
                            layers: asset.layers.clone(),
                        },
                    );
                    info!("Hot-reloaded parallax config for biome: {biome_id}");
                    break;
                }
            }
        }
    }
}
