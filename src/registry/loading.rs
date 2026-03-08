//! Asset loading pipeline: base assets → biomes → autotiles.

use std::collections::HashSet;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::assets::{
    AnimationDef, AutotileAsset, BiomeAsset, CharacterDefAsset, CharacterPartsDef, ItemDefAsset,
    LiquidRegistryAsset, ObjectDefAsset, ParallaxConfigAsset, PlanetTypeAsset, RecipeListAsset,
    TileRegistryAsset,
};
use super::biome::{
    BiomeDef, BiomeId, BiomeRegistry, LayerBoundaries, LayerConfig, LayerConfigs, PlanetConfig,
};
use super::hot_reload::BiomeHandles;
use super::player::PlayerConfig;
use super::tile::TileRegistry;
use super::world::ActiveWorld;
use super::{AppState, BiomeParallaxConfigs, RegistryHandles};
use crate::cosmos::address::{CelestialAddress, CelestialSeeds};
use crate::cosmos::assets::{GenerationConfigAsset, StarTypeAsset};
use crate::cosmos::current::CurrentSystem;
use crate::cosmos::generation::generate_system;
use crate::cosmos::pressurization::PressureMap;
use crate::cosmos::ship_location::{GlobalBiome, ShipManifest};
use crate::item::definition::ItemDef;
use crate::item::registry::ItemRegistry;
use crate::object::definition::ObjectDef;
use crate::object::registry::ObjectRegistry;
use crate::world::day_night::WorldTime;

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
    objects: Vec<(String, Handle<ObjectDefAsset>)>,
    character: Handle<CharacterDefAsset>,
    generation_config: Handle<GenerationConfigAsset>,
    star_types: Vec<(String, Handle<StarTypeAsset>)>,
    planet_types: Vec<(String, Handle<PlanetTypeAsset>)>,
    items: Vec<(String, Handle<ItemDefAsset>)>,
    recipes: Vec<(String, Handle<RecipeListAsset>)>,
    liquids: Handle<LiquidRegistryAsset>,
    ui_theme: Handle<crate::ui::game_ui::theme::UiTheme>,
}

/// Intermediate resource holding autotile asset handles during loading.
#[derive(Resource)]
pub(crate) struct LoadingAutotileAssets {
    rons: Vec<(String, Handle<AutotileAsset>)>,
    images: Vec<(String, Handle<Image>)>,
}

/// Intermediate resource holding handles during biome loading phase.
#[derive(Resource)]
pub struct LoadingBiomeAssets {
    pub(crate) planet_type: Handle<PlanetTypeAsset>,
    pub(crate) biomes: Vec<(String, Handle<BiomeAsset>)>,
    pub(crate) parallax_configs: Vec<(String, Handle<ParallaxConfigAsset>)>,
}

/// Character animation configuration built from CharacterDefAsset.
/// Stored as a resource so the animation system can load frames data-driven.
#[derive(Resource, Debug, Clone)]
pub struct CharacterAnimConfig {
    pub sprite_size: (u32, u32),
    pub render_scale: f32,
    pub animations: std::collections::HashMap<String, AnimationDef>,
    pub base_path: String,
    pub parts: Option<CharacterPartsDef>,
}

pub(crate) fn start_loading(mut commands: Commands, asset_server: Res<AssetServer>) {
    let tiles = asset_server.load::<TileRegistryAsset>("worlds/tiles.registry.ron");
    let character = asset_server
        .load::<CharacterDefAsset>("content/characters/char/char.character.ron");

    // Load cosmos generation assets
    let generation_config = asset_server.load::<GenerationConfigAsset>("worlds/generation.ron");
    let star_types = vec![(
        "yellow_dwarf".to_string(),
        asset_server
            .load::<StarTypeAsset>("worlds/star_types/yellow_dwarf/yellow_dwarf.star.ron"),
    )];
    let planet_types = vec![
        (
            "garden".to_string(),
            asset_server
                .load::<PlanetTypeAsset>("worlds/planet_types/garden/garden.planet.ron"),
        ),
        (
            "barren".to_string(),
            asset_server
                .load::<PlanetTypeAsset>("worlds/planet_types/barren/barren.planet.ron"),
        ),
        (
            "ship".to_string(),
            asset_server
                .load::<PlanetTypeAsset>("worlds/planet_types/ship/ship.planet.ron"),
        ),
    ];

    // Load object definitions from individual *.object.ron files.
    // "none" MUST be first — ObjectId(0) == ObjectId::NONE.
    let objects = vec![
        (
            "content/objects/none/".to_string(),
            asset_server.load::<ObjectDefAsset>("content/objects/none/none.object.ron"),
        ),
        (
            "content/objects/torch/".to_string(),
            asset_server.load::<ObjectDefAsset>("content/objects/torch/torch.object.ron"),
        ),
        (
            "content/objects/wooden_chest/".to_string(),
            asset_server
                .load::<ObjectDefAsset>("content/objects/wooden_chest/wooden_chest.object.ron"),
        ),
        (
            "content/objects/wooden_table/".to_string(),
            asset_server
                .load::<ObjectDefAsset>("content/objects/wooden_table/wooden_table.object.ron"),
        ),
        (
            "content/objects/workbench/".to_string(),
            asset_server
                .load::<ObjectDefAsset>("content/objects/workbench/workbench.object.ron"),
        ),
        (
            "content/objects/tree/".to_string(),
            asset_server.load::<ObjectDefAsset>("content/objects/tree/tree.object.ron"),
        ),
        (
            "content/objects/airlock/".to_string(),
            asset_server.load::<ObjectDefAsset>("content/objects/airlock/airlock.object.ron"),
        ),
        (
            "content/objects/fuel_tank/".to_string(),
            asset_server.load::<ObjectDefAsset>("content/objects/fuel_tank/fuel_tank.object.ron"),
        ),
        (
            "content/objects/autopilot_console/".to_string(),
            asset_server.load::<ObjectDefAsset>(
                "content/objects/autopilot_console/autopilot_console.object.ron",
            ),
        ),
        (
            "content/objects/capsule/".to_string(),
            asset_server.load::<ObjectDefAsset>("content/objects/capsule/capsule.object.ron"),
        ),
    ];

    // Load item definitions from individual *.item.ron files.
    // Objects with `auto_item` config don't need separate .item.ron files —
    // their items are auto-generated during check_loading.
    let items = vec![
        (
            "content/tiles/dirt/".to_string(),
            asset_server.load::<ItemDefAsset>("content/tiles/dirt/dirt.item.ron"),
        ),
        (
            "content/tiles/stone/".to_string(),
            asset_server.load::<ItemDefAsset>("content/tiles/stone/stone.item.ron"),
        ),
        (
            "content/tiles/grass/".to_string(),
            asset_server.load::<ItemDefAsset>("content/tiles/grass/grass.item.ron"),
        ),
        (
            "content/items/wood/".to_string(),
            asset_server.load::<ItemDefAsset>("content/items/wood/wood.item.ron"),
        ),
        (
            "content/items/blueprint_wooden_sword/".to_string(),
            asset_server.load::<ItemDefAsset>("content/items/blueprint_wooden_sword/blueprint_wooden_sword.item.ron"),
        ),
        (
            "content/items/blueprint_stone_pickaxe/".to_string(),
            asset_server.load::<ItemDefAsset>("content/items/blueprint_stone_pickaxe/blueprint_stone_pickaxe.item.ron"),
        ),
    ];

    let recipes = vec![
        (
            "base".to_string(),
            asset_server.load::<RecipeListAsset>("recipes/base.recipes.ron"),
        ),
        (
            "workbench".to_string(),
            asset_server.load::<RecipeListAsset>("recipes/workbench.recipes.ron"),
        ),
    ];

    let liquids =
        asset_server.load::<LiquidRegistryAsset>("worlds/liquids.liquid.ron");
    let ui_theme =
        asset_server.load::<crate::ui::game_ui::theme::UiTheme>("ui.theme.ron");

    commands.insert_resource(LoadingAssets {
        tiles,
        objects,
        character,
        generation_config,
        star_types,
        planet_types,
        items,
        recipes,
        liquids,
        ui_theme,
    });
}

pub(crate) fn check_loading(
    mut commands: Commands,
    loading: Res<LoadingAssets>,
    tile_assets: Res<Assets<TileRegistryAsset>>,
    object_assets: Res<Assets<ObjectDefAsset>>,
    character_assets: Res<Assets<CharacterDefAsset>>,
    gen_config_assets: Res<Assets<GenerationConfigAsset>>,
    star_type_assets: Res<Assets<StarTypeAsset>>,
    planet_type_assets: Res<Assets<PlanetTypeAsset>>,
    item_assets: Res<Assets<ItemDefAsset>>,
    recipe_assets: Res<Assets<RecipeListAsset>>,
    liquid_assets: Res<Assets<LiquidRegistryAsset>>,
    ui_theme_assets: Res<Assets<crate::ui::game_ui::theme::UiTheme>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let (Some(tiles), Some(character)) = (
        tile_assets.get(&loading.tiles),
        character_assets.get(&loading.character),
    ) else {
        return; // not loaded yet
    };

    // Wait for generation config
    let Some(gen_config) = gen_config_assets.get(&loading.generation_config) else {
        return;
    };

    // Wait for all star/planet types
    let all_stars = loading
        .star_types
        .iter()
        .all(|(_, h)| star_type_assets.contains(h));
    let all_planets = loading
        .planet_types
        .iter()
        .all(|(_, h)| planet_type_assets.contains(h));
    if !all_stars || !all_planets {
        return;
    }

    // Wait for all object assets to load
    let all_objects_loaded = loading
        .objects
        .iter()
        .all(|(_, h)| object_assets.contains(h));
    if !all_objects_loaded {
        return;
    }

    // Wait for all item assets to load
    let all_items_loaded = loading
        .items
        .iter()
        .all(|(_, h)| item_assets.contains(h));
    if !all_items_loaded {
        return;
    }

    // Wait for all recipe assets
    let all_recipes_loaded = loading
        .recipes
        .iter()
        .all(|(_, h)| recipe_assets.contains(h));
    if !all_recipes_loaded {
        return;
    }

    // Wait for liquid registry
    if !liquid_assets.contains(&loading.liquids) {
        return;
    }

    // Wait for UI theme
    if !ui_theme_assets.contains(&loading.ui_theme) {
        return;
    }

    // Build ObjectRegistry from loaded object.ron files (order preserved from start_loading)
    let object_defs: Vec<ObjectDef> = loading
        .objects
        .iter()
        .filter_map(|(base_path, handle)| {
            object_assets
                .get(handle)
                .map(|asset| asset.to_object_def(base_path))
        })
        .collect();

    // Build ItemRegistry from loaded item.ron files + auto-generated items from objects
    let mut item_defs: Vec<ItemDef> = loading
        .items
        .iter()
        .filter_map(|(base_path, handle)| {
            item_assets
                .get(handle)
                .map(|asset| asset.to_item_def(base_path))
        })
        .collect();

    // Generate ItemDefs from objects with auto_item config
    for (base_path, handle) in &loading.objects {
        if let Some(asset) = object_assets.get(handle) {
            let def = asset.to_object_def(base_path);
            if let Some(item_def) = def.generate_item_def(base_path) {
                info!("Auto-generated item '{}' from object '{}'", item_def.id, def.id);
                item_defs.push(item_def);
            }
        }
    }

    commands.insert_resource(ItemRegistry::from_defs(item_defs));

    // Build RecipeRegistry from loaded recipe.ron files
    let mut recipe_registry = crate::crafting::RecipeRegistry::new();
    for (_name, handle) in &loading.recipes {
        if let Some(asset) = recipe_assets.get(handle) {
            for recipe in &asset.0 {
                recipe_registry.add(recipe.clone());
            }
        }
    }
    info!("Recipe registry loaded: {} recipes", recipe_registry.len());
    commands.insert_resource(recipe_registry);

    // Build resources from loaded assets
    let registry_ref = TileRegistry::from_defs(tiles.tiles.clone());

    // Build liquid registry from loaded asset
    let liquid_asset = liquid_assets.get(&loading.liquids).unwrap();
    let liquid_registry =
        crate::liquid::registry::LiquidRegistry::from_defs(liquid_asset.0.clone());
    bevy::log::info!("Liquid registry loaded: {} defs", liquid_registry.defs.len());
    commands.insert_resource(liquid_registry);

    // Insert UI theme from loaded asset
    let ui_theme = ui_theme_assets.get(&loading.ui_theme).unwrap().clone();
    commands.insert_resource(ui_theme);

    commands.insert_resource(registry_ref);
    commands.insert_resource(ObjectRegistry::from_defs(object_defs));
    commands.insert_resource(PlayerConfig {
        speed: character.speed,
        jump_velocity: character.jump_velocity,
        gravity: character.gravity,
        width: character.width,
        height: character.height,
        magnet_radius: character.magnet_radius,
        magnet_strength: character.magnet_strength,
        pickup_radius: character.pickup_radius,
        swim_impulse: character.swim_impulse,
        swim_gravity_factor: character.swim_gravity_factor,
        swim_drag: character.swim_drag,
    });

    // Store character animation data for the animation system
    commands.insert_resource(CharacterAnimConfig {
        sprite_size: character.sprite_size,
        render_scale: character.render_scale,
        animations: character.animations.clone(),
        base_path: "content/characters/char/".to_string(),
        parts: character.parts.clone(),
    });

    // --- Procedural system generation ---

    // Collect star templates
    let star_templates: Vec<&StarTypeAsset> = loading
        .star_types
        .iter()
        .filter_map(|(_, h)| star_type_assets.get(h))
        .collect();

    // Collect planet templates
    let planet_templates: std::collections::HashMap<String, &PlanetTypeAsset> = loading
        .planet_types
        .iter()
        .filter_map(|(name, h)| planet_type_assets.get(h).map(|a| (name.clone(), a)))
        .collect();

    // Generate system (hardcoded universe_seed=42, galaxy=(0,0), system=(0,0) for now)
    let system = generate_system(
        42, // universe_seed — hardcoded for now
        IVec2::ZERO,
        IVec2::ZERO,
        &star_templates,
        &planet_templates,
        gen_config,
    );

    // Find first garden planet for ship orbit reference
    let garden_body = system
        .bodies
        .iter()
        .find(|b| b.planet_type_id == "garden")
        .or_else(|| system.bodies.first())
        .expect("system must have at least one body");
    let orbit_address = garden_body.address.clone();

    // Build ActiveWorld for the player's ship instead of a planet
    let ship_address = CelestialAddress::Ship { ship_id: 0 };
    let ship_planet_type = "ship".to_string();
    let ship_width: i32 = 128;
    let ship_height: i32 = 64;

    let seeds = CelestialSeeds::derive(42, &ship_address);
    let active_world = ActiveWorld {
        address: ship_address,
        seeds: seeds.clone(),
        width_tiles: ship_width,
        height_tiles: ship_height,
        chunk_size: gen_config.chunk_size,
        tile_size: gen_config.tile_size,
        chunk_load_radius: gen_config.chunk_load_radius,
        seed: seeds.terrain_seed_u32(),
        planet_type: ship_planet_type.clone(),
        wrap_x: false,
    };
    commands.insert_resource(TerrainNoiseCache::new(active_world.seed));
    commands.insert_resource(active_world);

    // Ship manifest with starter ship orbiting the first garden planet
    commands.insert_resource(ShipManifest::with_starter_ship(orbit_address.clone()));
    commands.insert_resource(PressureMap::new_dirty());

    // DayNightConfig for ship (permanent "day" lighting)
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

    // Keep handles alive for hot-reload
    commands.insert_resource(RegistryHandles {
        tiles: loading.tiles.clone(),
        objects: loading.objects.clone(),
        character: loading.character.clone(),
        items: loading.items.clone(),
        recipes: loading.recipes.clone(),
        liquids: loading.liquids.clone(),
        ui_theme: loading.ui_theme.clone(),
    });

    // Load the "ship" planet type for the biome pipeline
    let planet_handle = loading
        .planet_types
        .iter()
        .find(|(name, _)| name == &ship_planet_type)
        .map(|(_, h)| h.clone())
        .expect("ship planet type must have been loaded");
    commands.insert_resource(LoadingBiomeAssets {
        planet_type: planet_handle,
        biomes: Vec::new(),
        parallax_configs: Vec::new(),
    });

    // Store system for star-map UI and planet warping
    commands.insert_resource(CurrentSystem {
        system: system.clone(),
        universe_seed: 42,
        chunk_size: gen_config.chunk_size,
        tile_size: gen_config.tile_size,
        chunk_load_radius: gen_config.chunk_load_radius,
    });

    info!(
        "Generated system: star={}, {} bodies, spawning on ship (orbiting {})",
        system.star.type_id,
        system.bodies.len(),
        orbit_address.orbit().unwrap_or(0),
    );

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
    world_config: Res<ActiveWorld>,
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
                asset_server.load::<BiomeAsset>(format!("content/biomes/{id}/{id}.biome.ron"));
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

    // For ship worlds, insert a GlobalBiome override so the entire world
    // uses a single biome instead of position-based biome detection.
    if matches!(world_config.address, CelestialAddress::Ship { .. }) {
        let primary_id = biome_registry.id_by_name(&planet_config.primary_biome);
        commands.insert_resource(GlobalBiome {
            biome_id: primary_id,
        });
        info!(
            "Ship world detected — inserted GlobalBiome override: {}",
            planet_config.primary_biome
        );
    }

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
            let ron_handle = asset_server
                .load::<AutotileAsset>(format!("content/tiles/{name}/{name}.autotile.ron"));
            let img_handle =
                asset_server.load::<Image>(format!("content/tiles/{name}/{name}.png"));
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

    // Create 1×1 white fallback lightmap (replaced by RC pipeline each frame).
    // Uses Rgba16Float to match the RC pipeline's lightmap format exactly.
    let white_lightmap = image_assets.add(Image::new_fill(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        // f16 1.0 = 0x3C00 → little-endian [0x00, 0x3C] per channel
        &[0x00u8, 0x3C, 0x00, 0x3C, 0x00, 0x3C, 0x00, 0x3C],
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    ));

    // Create shared tile materials: full brightness for foreground, dimmed for background
    let fg_material = tile_materials.add(TileMaterial {
        atlas: atlas_handle.clone(),
        dim: 1.0,
        lightmap: white_lightmap.clone(),
        lightmap_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0), // No scaling/offset
    });
    let bg_material = tile_materials.add(TileMaterial {
        atlas: atlas_handle.clone(),
        dim: 0.6,
        lightmap: white_lightmap,
        lightmap_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0), // No scaling/offset
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
