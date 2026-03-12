//! Hot-reload systems for registry assets.

use bevy::asset::AssetEvent;
use bevy::ecs::message::MessageReader;
use bevy::prelude::*;

use super::assets::{
    BiomeAsset, CharacterDefAsset, ItemDefAsset, LiquidRegistryAsset, ObjectDefAsset,
    ParallaxConfigAsset, PlanetTypeAsset, RecipeListAsset, TileRegistryAsset,
};
use super::biome::{
    BiomeDef, BiomeId, BiomeRegistry, LayerBoundaries, LayerConfig, LayerConfigs, PlanetConfig,
};
use super::player::PlayerConfig;
use super::tile::TileRegistry;
use super::world::ActiveWorld;
use super::{BiomeParallaxConfigs, RegistryHandles};
use crate::object::registry::ObjectRegistry;

use crate::parallax::config::ParallaxConfig;
use crate::world::biome_map::BiomeMap;

/// Keeps biome-related asset handles alive for hot-reload detection.
#[derive(Resource)]
pub(crate) struct BiomeHandles {
    pub(crate) planet_type: Handle<PlanetTypeAsset>,
    pub(crate) biomes: Vec<(BiomeId, Handle<BiomeAsset>)>,
    pub(crate) parallax_configs: Vec<(BiomeId, Handle<ParallaxConfigAsset>)>,
}

pub(crate) fn hot_reload_character(
    mut events: MessageReader<AssetEvent<CharacterDefAsset>>,
    handles: Res<RegistryHandles>,
    assets: Res<Assets<CharacterDefAsset>>,
    mut config: ResMut<PlayerConfig>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event
            && *id == handles.character.id()
            && let Some(asset) = assets.get(&handles.character)
        {
            config.speed = asset.speed;
            config.jump_velocity = asset.jump_velocity;
            config.gravity = asset.gravity;
            config.width = asset.width;
            config.height = asset.height;
            config.magnet_radius = asset.magnet_radius;
            config.magnet_strength = asset.magnet_strength;
            config.pickup_radius = asset.pickup_radius;
            config.swim_impulse = asset.swim_impulse;
            config.swim_gravity_factor = asset.swim_gravity_factor;
            config.swim_drag = asset.swim_drag;
            info!(
                "Hot-reloaded PlayerConfig: speed={}, jump={}, gravity={}, magnet_r={}, magnet_s={}",
                asset.speed, asset.jump_velocity, asset.gravity,
                config.magnet_radius, config.magnet_strength
            );
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

pub(crate) fn hot_reload_objects(
    mut events: MessageReader<AssetEvent<ObjectDefAsset>>,
    handles: Res<RegistryHandles>,
    assets: Res<Assets<ObjectDefAsset>>,
    item_assets: Res<Assets<ItemDefAsset>>,
    mut registry: ResMut<ObjectRegistry>,
    mut item_registry: ResMut<crate::item::registry::ItemRegistry>,
) {
    let mut changed = false;
    for event in events.read() {
        if let AssetEvent::Modified { id } = event
            && handles.objects.iter().any(|(_, h)| *id == h.id())
        {
            changed = true;
        }
    }
    if !changed {
        return;
    }
    // Rebuild entire registry from all individual object assets (preserves ordering)
    let defs: Vec<_> = handles
        .objects
        .iter()
        .filter_map(|(base_path, handle)| {
            assets.get(handle).map(|a| a.to_object_def(base_path))
        })
        .collect();
    if defs.len() == handles.objects.len() {
        *registry = ObjectRegistry::from_defs(defs);
        info!(
            "Hot-reloaded ObjectRegistry ({} objects)",
            registry.len()
        );

        // Rebuild ItemRegistry: explicit .item.ron items + auto-generated from objects
        let mut item_defs: Vec<_> = handles
            .items
            .iter()
            .filter_map(|(base_path, handle)| {
                item_assets.get(handle).map(|a| a.to_item_def(base_path))
            })
            .collect();
        for (base_path, handle) in &handles.objects {
            if let Some(asset) = assets.get(handle) {
                let def = asset.to_object_def(base_path);
                if let Some(item_def) = def.generate_item_def(base_path) {
                    item_defs.push(item_def);
                }
            }
        }
        *item_registry = crate::item::registry::ItemRegistry::from_defs(item_defs);
        info!(
            "Hot-reloaded ItemRegistry from objects ({} items)",
            item_registry.len()
        );
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
                            snow_base_chance: asset.snow_base_chance,
                            snow_permanent: asset.snow_permanent,
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
    world_config: Res<ActiveWorld>,
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
                    depth_ratio: asset.layers.surface.depth_ratio,
                },
                underground: LayerConfig {
                    primary_biome: asset.layers.underground.primary_biome.clone(),
                    terrain_frequency: asset.layers.underground.terrain_frequency,
                    terrain_amplitude: asset.layers.underground.terrain_amplitude,
                    depth_ratio: asset.layers.underground.depth_ratio,
                },
                deep_underground: LayerConfig {
                    primary_biome: asset.layers.deep_underground.primary_biome.clone(),
                    terrain_frequency: asset.layers.deep_underground.terrain_frequency,
                    terrain_amplitude: asset.layers.deep_underground.terrain_amplitude,
                    depth_ratio: asset.layers.deep_underground.depth_ratio,
                },
                core: LayerConfig {
                    primary_biome: asset.layers.core.primary_biome.clone(),
                    terrain_frequency: asset.layers.core.terrain_frequency,
                    terrain_amplitude: asset.layers.core.terrain_amplitude,
                    depth_ratio: asset.layers.core.depth_ratio,
                },
            };
            planet_config.layer_boundaries = LayerBoundaries::from_layers(
                &planet_config.layers,
                world_config.height_tiles,
            );
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

pub(crate) fn hot_reload_items(
    mut events: MessageReader<AssetEvent<ItemDefAsset>>,
    handles: Res<RegistryHandles>,
    assets: Res<Assets<ItemDefAsset>>,
    mut registry: ResMut<crate::item::registry::ItemRegistry>,
) {
    let mut changed = false;
    for event in events.read() {
        if let AssetEvent::Modified { id } = event
            && handles.items.iter().any(|(_, h)| *id == h.id())
        {
            changed = true;
        }
    }
    if !changed {
        return;
    }
    // Rebuild entire item registry from all individual item assets
    let defs: Vec<_> = handles
        .items
        .iter()
        .filter_map(|(base_path, handle)| {
            assets.get(handle).map(|a| a.to_item_def(base_path))
        })
        .collect();
    if defs.len() == handles.items.len() {
        *registry = crate::item::registry::ItemRegistry::from_defs(defs);
        info!("Hot-reloaded ItemRegistry ({} items)", registry.len());
    }
}

pub(crate) fn hot_reload_recipes(
    mut events: MessageReader<AssetEvent<RecipeListAsset>>,
    handles: Res<RegistryHandles>,
    assets: Res<Assets<RecipeListAsset>>,
    mut registry: ResMut<crate::crafting::RecipeRegistry>,
) {
    let mut changed = false;
    for event in events.read() {
        if let AssetEvent::Modified { id } = event
            && handles.recipes.iter().any(|(_, h)| *id == h.id())
        {
            changed = true;
        }
    }
    if !changed {
        return;
    }
    // Rebuild entire recipe registry from all recipe list assets
    let mut new_registry = crate::crafting::RecipeRegistry::new();
    for (_name, handle) in &handles.recipes {
        if let Some(asset) = assets.get(handle) {
            for recipe in &asset.0 {
                new_registry.add(recipe.clone());
            }
        }
    }
    info!(
        "Hot-reloaded RecipeRegistry ({} recipes)",
        new_registry.len()
    );
    *registry = new_registry;
}

pub(crate) fn hot_reload_liquids(
    mut events: MessageReader<AssetEvent<LiquidRegistryAsset>>,
    handles: Res<RegistryHandles>,
    assets: Res<Assets<LiquidRegistryAsset>>,
    mut registry: ResMut<crate::liquid::registry::LiquidRegistry>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event
            && *id == handles.liquids.id()
            && let Some(asset) = assets.get(&handles.liquids)
        {
            *registry =
                crate::liquid::registry::LiquidRegistry::from_defs(asset.0.clone());
            info!(
                "Hot-reloaded LiquidRegistry ({} defs)",
                registry.defs.len()
            );
        }
    }
}

pub(crate) fn hot_reload_ui_theme(
    mut events: MessageReader<AssetEvent<crate::ui::game_ui::theme::UiTheme>>,
    handles: Res<RegistryHandles>,
    assets: Res<Assets<crate::ui::game_ui::theme::UiTheme>>,
    mut theme: ResMut<crate::ui::game_ui::theme::UiTheme>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event
            && *id == handles.ui_theme.id()
            && let Some(asset) = assets.get(&handles.ui_theme)
        {
            *theme = asset.clone();
            info!("Hot-reloaded UiTheme");
        }
    }
}
