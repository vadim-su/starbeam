//! Asset loading pipeline: base assets → biomes → autotiles.

use std::collections::HashSet;

use bevy::prelude::*;

use super::assets::{
    AutotileAsset, BiomeAsset, ParallaxConfigAsset, PlanetTypeAsset, PlayerDefAsset,
    TileRegistryAsset, WorldConfigAsset,
};
use super::biome::{
    BiomeDef, BiomeId, BiomeRegistry, LayerBoundaries, LayerConfig, LayerConfigs, PlanetConfig,
};
use super::hot_reload::BiomeHandles;
use super::player::PlayerConfig;
use super::tile::TileRegistry;
use super::world::WorldConfig;
use super::{AppState, BiomeParallaxConfigs, RegistryHandles};

use crate::parallax::config::ParallaxConfig;
use crate::world::atlas::{build_combined_atlas, AtlasParams, TileAtlas};
use crate::world::autotile::{AutotileEntry, AutotileRegistry};
use crate::world::biome_map::BiomeMap;
use crate::world::terrain_gen::TerrainNoiseCache;
use crate::world::tile_renderer::{SharedTileMaterial, TileMaterial};

/// Handles for assets being loaded.
#[derive(Resource)]
pub(crate) struct LoadingAssets {
    tiles: Handle<TileRegistryAsset>,
    player: Handle<PlayerDefAsset>,
    world_config: Handle<WorldConfigAsset>,
}

/// Intermediate resource holding autotile asset handles during loading.
#[derive(Resource)]
pub(crate) struct LoadingAutotileAssets {
    rons: Vec<(String, Handle<AutotileAsset>)>,
    images: Vec<(String, Handle<Image>)>,
}

/// Intermediate resource holding handles during biome loading phase.
#[derive(Resource)]
pub(crate) struct LoadingBiomeAssets {
    planet_type: Handle<PlanetTypeAsset>,
    biomes: Vec<(String, Handle<BiomeAsset>)>,
    parallax_configs: Vec<(String, Handle<ParallaxConfigAsset>)>,
}

pub(crate) fn start_loading(mut commands: Commands, asset_server: Res<AssetServer>) {
    let tiles = asset_server.load::<TileRegistryAsset>("world/tiles.registry.ron");
    let player = asset_server.load::<PlayerDefAsset>("characters/adventurer/adventurer.def.ron");
    let world_config = asset_server.load::<WorldConfigAsset>("world/world.config.ron");
    commands.insert_resource(LoadingAssets {
        tiles,
        player,
        world_config,
    });
}

pub(crate) fn check_loading(
    mut commands: Commands,
    loading: Res<LoadingAssets>,
    asset_server: Res<AssetServer>,
    tile_assets: Res<Assets<TileRegistryAsset>>,
    player_assets: Res<Assets<PlayerDefAsset>>,
    world_assets: Res<Assets<WorldConfigAsset>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let (Some(tiles), Some(player), Some(world_cfg)) = (
        tile_assets.get(&loading.tiles),
        player_assets.get(&loading.player),
        world_assets.get(&loading.world_config),
    ) else {
        return; // not loaded yet
    };

    // Build resources from loaded assets
    let registry_ref = TileRegistry::from_defs(tiles.tiles.clone());
    commands.insert_resource(registry_ref);
    commands.insert_resource(PlayerConfig {
        speed: player.speed,
        jump_velocity: player.jump_velocity,
        gravity: player.gravity,
        width: player.width,
        height: player.height,
    });
    commands.insert_resource(WorldConfig {
        width_tiles: world_cfg.width_tiles,
        height_tiles: world_cfg.height_tiles,
        chunk_size: world_cfg.chunk_size,
        tile_size: world_cfg.tile_size,
        chunk_load_radius: world_cfg.chunk_load_radius,
        seed: world_cfg.seed,
        planet_type: world_cfg.planet_type.clone(),
    });
    commands.insert_resource(TerrainNoiseCache::new(world_cfg.seed));

    // Keep handles alive for hot-reload
    commands.insert_resource(RegistryHandles {
        tiles: loading.tiles.clone(),
        player: loading.player.clone(),
        world_config: loading.world_config.clone(),
    });

    // Start loading the planet type asset for the biome pipeline
    let planet_handle = asset_server.load::<PlanetTypeAsset>(format!(
        "world/planet_types/{}.planet.ron",
        world_cfg.planet_type
    ));
    commands.insert_resource(LoadingBiomeAssets {
        planet_type: planet_handle,
        biomes: Vec::new(),
        parallax_configs: Vec::new(),
    });

    commands.remove_resource::<LoadingAssets>();
    next_state.set(AppState::LoadingBiomes);
    info!("Base registry assets loaded, loading biome assets...");
}

/// Multi-phase system that loads planet type → biome assets → parallax configs,
/// then builds BiomeRegistry, BiomeMap, PlanetConfig, and BiomeParallaxConfigs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_biomes_loaded(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut loading: ResMut<LoadingBiomeAssets>,
    planet_assets: Res<Assets<PlanetTypeAsset>>,
    biome_assets: Res<Assets<BiomeAsset>>,
    parallax_assets: Res<Assets<ParallaxConfigAsset>>,
    tile_registry: Res<TileRegistry>,
    world_config: Res<WorldConfig>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    // Check for planet type load failure
    if let bevy::asset::LoadState::Failed(_) = asset_server.load_state(&loading.planet_type) {
        error!(
            "Failed to load planet type asset — check file exists and is valid"
        );
        return;
    }

    // Phase 1: Wait for planet type to load, then kick off biome loading
    let Some(planet_asset) = planet_assets.get(&loading.planet_type) else {
        return; // planet type not loaded yet
    };

    if loading.biomes.is_empty() {
        // Collect all unique biome IDs from the planet type
        let mut biome_ids = HashSet::new();
        biome_ids.insert(planet_asset.primary_biome.clone());
        for id in &planet_asset.secondary_biomes {
            biome_ids.insert(id.clone());
        }
        // Also collect biomes referenced in layer configs
        if let Some(ref b) = planet_asset.layers.surface.primary_biome {
            biome_ids.insert(b.clone());
        }
        if let Some(ref b) = planet_asset.layers.underground.primary_biome {
            biome_ids.insert(b.clone());
        }
        if let Some(ref b) = planet_asset.layers.deep_underground.primary_biome {
            biome_ids.insert(b.clone());
        }
        if let Some(ref b) = planet_asset.layers.core.primary_biome {
            biome_ids.insert(b.clone());
        }

        // Load each biome asset
        for id in &biome_ids {
            let handle =
                asset_server.load::<BiomeAsset>(format!("world/biomes/{id}/{id}.biome.ron"));
            loading.biomes.push((id.clone(), handle));
        }

        info!("Loading {} biome assets...", loading.biomes.len());
        return; // wait for next frame
    }

    // Check for biome load failures
    for (name, handle) in &loading.biomes {
        if let bevy::asset::LoadState::Failed(_) = asset_server.load_state(handle) {
            error!("Failed to load biome asset: {name} — check file exists and is valid");
        }
    }

    // Phase 2: Wait for all biomes to load, then load parallax configs
    let all_biomes_loaded = loading
        .biomes
        .iter()
        .all(|(_, h)| biome_assets.contains(h));
    if !all_biomes_loaded {
        return;
    }

    if loading.parallax_configs.is_empty() {
        // Collect parallax paths from loaded biome assets (separate pass to avoid borrow conflict)
        let parallax_to_load: Vec<(String, String)> = loading
            .biomes
            .iter()
            .filter_map(|(biome_id, handle)| {
                biome_assets
                    .get(handle)
                    .and_then(|asset| asset.parallax.as_ref().map(|p| (biome_id.clone(), p.clone())))
            })
            .collect();

        if !parallax_to_load.is_empty() {
            for (biome_id, parallax_path) in &parallax_to_load {
                let parallax_handle =
                    asset_server.load::<ParallaxConfigAsset>(parallax_path.clone());
                loading
                    .parallax_configs
                    .push((biome_id.clone(), parallax_handle));
            }

            info!(
                "Loading {} biome parallax configs...",
                loading.parallax_configs.len()
            );
            return; // wait for next frame
        }
        // If no biome has parallax, fall through to Phase 3
    }

    // Check for parallax load failures
    for (name, handle) in &loading.parallax_configs {
        if let bevy::asset::LoadState::Failed(_) = asset_server.load_state(handle) {
            error!("Failed to load biome parallax config: {name} — check file exists");
        }
    }

    // Phase 3: Wait for all parallax configs, then build resources
    if !loading.parallax_configs.is_empty() {
        let all_parallax_loaded = loading
            .parallax_configs
            .iter()
            .all(|(_, h)| parallax_assets.contains(h));
        if !all_parallax_loaded {
            return;
        }
    }

    // --- Build PlanetConfig ---
    let layers = LayerConfigs {
        surface: LayerConfig {
            primary_biome: planet_asset.layers.surface.primary_biome.clone(),
            terrain_frequency: planet_asset.layers.surface.terrain_frequency,
            terrain_amplitude: planet_asset.layers.surface.terrain_amplitude,
            depth_ratio: planet_asset.layers.surface.depth_ratio,
        },
        underground: LayerConfig {
            primary_biome: planet_asset.layers.underground.primary_biome.clone(),
            terrain_frequency: planet_asset.layers.underground.terrain_frequency,
            terrain_amplitude: planet_asset.layers.underground.terrain_amplitude,
            depth_ratio: planet_asset.layers.underground.depth_ratio,
        },
        deep_underground: LayerConfig {
            primary_biome: planet_asset.layers.deep_underground.primary_biome.clone(),
            terrain_frequency: planet_asset.layers.deep_underground.terrain_frequency,
            terrain_amplitude: planet_asset.layers.deep_underground.terrain_amplitude,
            depth_ratio: planet_asset.layers.deep_underground.depth_ratio,
        },
        core: LayerConfig {
            primary_biome: planet_asset.layers.core.primary_biome.clone(),
            terrain_frequency: planet_asset.layers.core.terrain_frequency,
            terrain_amplitude: planet_asset.layers.core.terrain_amplitude,
            depth_ratio: planet_asset.layers.core.depth_ratio,
        },
    };
    let layer_boundaries = LayerBoundaries::from_layers(&layers, world_config.height_tiles);
    let planet_config = PlanetConfig {
        id: planet_asset.id.clone(),
        primary_biome: planet_asset.primary_biome.clone(),
        secondary_biomes: planet_asset.secondary_biomes.clone(),
        layers,
        layer_boundaries,
        region_width_min: planet_asset.region_width_min,
        region_width_max: planet_asset.region_width_max,
        primary_region_ratio: planet_asset.primary_region_ratio,
    };

    // --- Build BiomeRegistry ---
    let mut biome_registry = BiomeRegistry::default();
    for (name, handle) in &loading.biomes {
        let asset = biome_assets.get(handle).unwrap();
        biome_registry.insert(
            name,
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
    }

    // --- Build BiomeMap ---
    let secondaries: Vec<&str> = planet_config
        .secondary_biomes
        .iter()
        .map(|s| s.as_str())
        .collect();
    let biome_map = BiomeMap::generate(
        &planet_config.primary_biome,
        &secondaries,
        world_config.seed as u64,
        world_config.width_tiles as u32,
        planet_config.region_width_min,
        planet_config.region_width_max,
        planet_config.primary_region_ratio,
        &biome_registry,
    );
    let region_count = biome_map.regions.len();

    // --- Build BiomeParallaxConfigs ---
    let mut biome_parallax = BiomeParallaxConfigs::default();
    for (biome_name, handle) in &loading.parallax_configs {
        let asset = parallax_assets.get(handle).unwrap();
        let id = biome_registry.id_by_name(biome_name);
        biome_parallax.configs.insert(
            id,
            ParallaxConfig {
                layers: asset.layers.clone(),
            },
        );
    }

    // Build biome handles before moving biome_registry (resolve names to BiomeId)
    let biome_handles: Vec<(BiomeId, Handle<BiomeAsset>)> = loading
        .biomes
        .iter()
        .map(|(name, handle)| (biome_registry.id_by_name(name), handle.clone()))
        .collect();
    let parallax_handles: Vec<(BiomeId, Handle<ParallaxConfigAsset>)> = loading
        .parallax_configs
        .iter()
        .map(|(name, handle)| (biome_registry.id_by_name(name), handle.clone()))
        .collect();

    // Insert all resources
    commands.insert_resource(planet_config);
    commands.insert_resource(biome_registry);
    commands.insert_resource(biome_map);
    commands.insert_resource(biome_parallax);
    commands.insert_resource(BiomeHandles {
        planet_type: loading.planet_type.clone(),
        biomes: biome_handles,
        parallax_configs: parallax_handles,
    });

    commands.remove_resource::<LoadingBiomeAssets>();
    next_state.set(AppState::LoadingAutotile);
    info!(
        "Biomes loaded, BiomeMap generated with {} regions",
        region_count
    );
}

pub(crate) fn start_autotile_loading(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    registry: Res<TileRegistry>,
) {
    let mut rons = Vec::new();
    let mut imgs = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for def in &registry.defs {
        if let Some(ref name) = def.autotile
            && seen.insert(name.clone())
        {
            let ron_handle =
                asset_server.load::<AutotileAsset>(format!("world/terrain/{name}.autotile.ron"));
            let img_handle =
                asset_server.load::<Image>(format!("world/terrain/{name}.png"));
            rons.push((name.clone(), ron_handle));
            imgs.push((name.clone(), img_handle));
        }
    }

    info!("Loading {} autotile assets...", rons.len());
    commands.insert_resource(LoadingAutotileAssets { rons, images: imgs });
}

pub(crate) fn check_autotile_loading(
    mut commands: Commands,
    loading: Res<LoadingAutotileAssets>,
    autotile_assets: Res<Assets<AutotileAsset>>,
    mut image_assets: ResMut<Assets<Image>>,
    mut tile_materials: ResMut<Assets<TileMaterial>>,
    asset_server: Res<AssetServer>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    // Check for load failures before waiting
    for (name, handle) in &loading.rons {
        if let bevy::asset::LoadState::Failed(_) = asset_server.load_state(handle) {
            error!("Failed to load autotile RON: {name} — check file exists and is valid");
        }
    }
    for (name, handle) in &loading.images {
        if let bevy::asset::LoadState::Failed(_) = asset_server.load_state(handle) {
            error!("Failed to load autotile image: {name} — check file exists");
        }
    }

    // Wait until all .autotile.ron and .png assets are loaded
    let all_rons = loading.rons.iter().all(|(_, h)| autotile_assets.contains(h));
    let all_imgs = loading.images.iter().all(|(_, h)| image_assets.contains(h));
    if !all_rons || !all_imgs {
        return;
    }

    // Read tile_size and rows from first loaded AutotileAsset for consistency
    let first_ron = autotile_assets
        .get(&loading.rons[0].1)
        .expect("first autotile RON must be loaded");
    let tile_size = first_ron.tile_size;
    let rows = first_ron.atlas_rows;

    // Build combined atlas from per-type spritesheet images
    let sources: Vec<(&str, &Image)> = loading
        .images
        .iter()
        .filter_map(|(name, handle)| {
            image_assets.get(handle).map(|img| (name.as_str(), img)).or_else(|| {
                error!("Failed to load autotile image: {name}");
                None
            })
        })
        .collect();

    if sources.len() != loading.images.len() {
        error!("Some autotile images failed to load, aborting atlas build");
        return;
    }

    let (atlas_image, column_map) = build_combined_atlas(&sources, tile_size, rows);
    let num_types = sources.len() as u32;
    let params = AtlasParams {
        tile_size,
        rows,
        atlas_width: num_types * tile_size,
        atlas_height: rows * tile_size,
    };
    let atlas_handle = image_assets.add(atlas_image);

    // Build AutotileRegistry from loaded .autotile.ron assets
    let mut autotile_reg = AutotileRegistry::default();
    for (name, handle) in &loading.rons {
        let Some(asset) = autotile_assets.get(handle) else {
            error!("Failed to get autotile RON asset: {name}");
            continue;
        };
        let col_idx = column_map[name.as_str()];
        autotile_reg.insert(name.clone(), AutotileEntry::from_asset(asset, col_idx));
    }

    // Create shared tile materials: full brightness for foreground, dimmed for background
    let fg_material = tile_materials.add(TileMaterial {
        atlas: atlas_handle.clone(),
        dim: 1.0,
    });
    let bg_material = tile_materials.add(TileMaterial {
        atlas: atlas_handle.clone(),
        dim: 0.6,
    });

    // Insert all autotile resources
    commands.insert_resource(TileAtlas {
        image: atlas_handle,
        params,
    });
    commands.insert_resource(autotile_reg);
    commands.insert_resource(SharedTileMaterial {
        fg: fg_material,
        bg: bg_material,
    });

    commands.remove_resource::<LoadingAutotileAssets>();
    next_state.set(AppState::InGame);
    info!(
        "Autotile atlas built ({} types), entering InGame",
        num_types
    );
}
