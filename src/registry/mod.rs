pub mod assets;
pub mod loader;
pub mod player;
pub mod tile;
pub mod world;

use bevy::asset::AssetEvent;
use bevy::ecs::message::MessageReader;
use bevy::prelude::*;

use assets::{AutotileAsset, ParallaxConfigAsset, PlayerDefAsset, TileRegistryAsset, WorldConfigAsset};
use loader::RonLoader;
use player::PlayerConfig;
use tile::TileRegistry;
use world::WorldConfig;

use crate::parallax::config::ParallaxConfig;
use crate::world::atlas::{build_combined_atlas, AtlasParams, TileAtlas};
use crate::world::autotile::{AutotileEntry, AutotileRegistry};
use crate::world::tile_renderer::{SharedTileMaterial, TileMaterial};

/// Keeps asset handles alive for hot-reload detection.
#[derive(Resource)]
pub struct RegistryHandles {
    pub tiles: Handle<TileRegistryAsset>,
    pub player: Handle<PlayerDefAsset>,
    pub world_config: Handle<WorldConfigAsset>,
    pub parallax: Handle<ParallaxConfigAsset>,
}

/// Application state: Loading waits for assets, InGame runs gameplay.
#[derive(States, Default, Debug, Clone, Eq, PartialEq, Hash)]
pub enum AppState {
    #[default]
    Loading,
    LoadingAutotile,
    InGame,
}

/// Handles for assets being loaded.
#[derive(Resource)]
struct LoadingAssets {
    tiles: Handle<TileRegistryAsset>,
    player: Handle<PlayerDefAsset>,
    world_config: Handle<WorldConfigAsset>,
    parallax: Handle<ParallaxConfigAsset>,
}

/// Intermediate resource holding autotile asset handles during loading.
#[derive(Resource)]
struct LoadingAutotileAssets {
    rons: Vec<(String, Handle<AutotileAsset>)>,
    images: Vec<(String, Handle<Image>)>,
}

pub struct RegistryPlugin;

impl Plugin for RegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .init_asset::<TileRegistryAsset>()
            .init_asset::<PlayerDefAsset>()
            .init_asset::<WorldConfigAsset>()
            .init_asset::<ParallaxConfigAsset>()
            .init_asset::<AutotileAsset>()
            .register_asset_loader(RonLoader::<TileRegistryAsset>::new(&["registry.ron"]))
            .register_asset_loader(RonLoader::<PlayerDefAsset>::new(&["def.ron"]))
            .register_asset_loader(RonLoader::<WorldConfigAsset>::new(&["config.ron"]))
            .register_asset_loader(RonLoader::<ParallaxConfigAsset>::new(&["parallax.ron"]))
            .register_asset_loader(RonLoader::<AutotileAsset>::new(&["autotile.ron"]))
            .add_systems(Startup, start_loading)
            .add_systems(Update, check_loading.run_if(in_state(AppState::Loading)))
            .add_systems(OnEnter(AppState::LoadingAutotile), start_autotile_loading)
            .add_systems(
                Update,
                check_autotile_loading.run_if(in_state(AppState::LoadingAutotile)),
            )
            .add_systems(
                Update,
                (
                    hot_reload_player,
                    hot_reload_world,
                    hot_reload_tiles,
                    hot_reload_parallax,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn start_loading(mut commands: Commands, asset_server: Res<AssetServer>) {
    let tiles = asset_server.load::<TileRegistryAsset>("world/tiles.registry.ron");
    let player = asset_server.load::<PlayerDefAsset>("characters/adventurer/adventurer.def.ron");
    let world_config = asset_server.load::<WorldConfigAsset>("world/world.config.ron");
    let parallax = asset_server.load::<ParallaxConfigAsset>("world/parallax.ron");
    commands.insert_resource(LoadingAssets {
        tiles,
        player,
        world_config,
        parallax,
    });
}

fn check_loading(
    mut commands: Commands,
    loading: Res<LoadingAssets>,
    tile_assets: Res<Assets<TileRegistryAsset>>,
    player_assets: Res<Assets<PlayerDefAsset>>,
    world_assets: Res<Assets<WorldConfigAsset>>,
    parallax_assets: Res<Assets<ParallaxConfigAsset>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let (Some(tiles), Some(player), Some(world_cfg), Some(parallax)) = (
        tile_assets.get(&loading.tiles),
        player_assets.get(&loading.player),
        world_assets.get(&loading.world_config),
        parallax_assets.get(&loading.parallax),
    ) else {
        return; // not loaded yet
    };

    // Build resources from loaded assets
    let registry_ref = TileRegistry::from_defs(tiles.tiles.clone());
    commands.insert_resource(tile::TerrainTiles {
        air: registry_ref.by_name("air"),
        grass: registry_ref.by_name("grass"),
        dirt: registry_ref.by_name("dirt"),
        stone: registry_ref.by_name("stone"),
    });
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
    });
    commands.insert_resource(ParallaxConfig {
        layers: parallax.layers.clone(),
    });

    // Keep handles alive for hot-reload
    commands.insert_resource(RegistryHandles {
        tiles: loading.tiles.clone(),
        player: loading.player.clone(),
        world_config: loading.world_config.clone(),
        parallax: loading.parallax.clone(),
    });
    commands.remove_resource::<LoadingAssets>();
    next_state.set(AppState::LoadingAutotile);
    info!("Base registry assets loaded, loading autotile assets...");
}

fn start_autotile_loading(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    registry: Res<TileRegistry>,
) {
    let mut rons = Vec::new();
    let mut imgs = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for def in &registry.defs {
        if let Some(ref name) = def.autotile {
            if seen.insert(name.clone()) {
                let ron_handle =
                    asset_server.load::<AutotileAsset>(format!("world/terrain/{name}.autotile.ron"));
                let img_handle =
                    asset_server.load::<Image>(format!("world/terrain/{name}.png"));
                rons.push((name.clone(), ron_handle));
                imgs.push((name.clone(), img_handle));
            }
        }
    }

    info!("Loading {} autotile assets...", rons.len());
    commands.insert_resource(LoadingAutotileAssets { rons, images: imgs });
}

fn check_autotile_loading(
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
        autotile_reg
            .entries
            .insert(name.clone(), AutotileEntry::from_asset(asset, col_idx));
    }

    // Create shared tile material with the combined atlas
    let material_handle = tile_materials.add(TileMaterial {
        atlas: atlas_handle.clone(),
    });

    // Insert all autotile resources
    commands.insert_resource(TileAtlas {
        image: atlas_handle,
        params,
    });
    commands.insert_resource(autotile_reg);
    commands.insert_resource(SharedTileMaterial {
        handle: material_handle,
    });

    commands.remove_resource::<LoadingAutotileAssets>();
    next_state.set(AppState::InGame);
    info!(
        "Autotile atlas built ({} types), entering InGame",
        num_types
    );
}

fn hot_reload_player(
    mut events: MessageReader<AssetEvent<PlayerDefAsset>>,
    handles: Res<RegistryHandles>,
    assets: Res<Assets<PlayerDefAsset>>,
    mut config: ResMut<PlayerConfig>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event {
            if *id == handles.player.id() {
                if let Some(asset) = assets.get(&handles.player) {
                    config.speed = asset.speed;
                    config.jump_velocity = asset.jump_velocity;
                    config.gravity = asset.gravity;
                    config.width = asset.width;
                    config.height = asset.height;
                    info!("Hot-reloaded PlayerConfig: speed={}, jump={}, gravity={}", asset.speed, asset.jump_velocity, asset.gravity);
                }
            }
        }
    }
}

fn hot_reload_world(
    mut events: MessageReader<AssetEvent<WorldConfigAsset>>,
    handles: Res<RegistryHandles>,
    assets: Res<Assets<WorldConfigAsset>>,
    mut config: ResMut<WorldConfig>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event {
            if *id == handles.world_config.id() {
                if let Some(asset) = assets.get(&handles.world_config) {
                    config.width_tiles = asset.width_tiles;
                    config.height_tiles = asset.height_tiles;
                    config.chunk_size = asset.chunk_size;
                    config.tile_size = asset.tile_size;
                    config.chunk_load_radius = asset.chunk_load_radius;
                    config.seed = asset.seed;
                    info!("Hot-reloaded WorldConfig");
                }
            }
        }
    }
}

fn hot_reload_tiles(
    mut events: MessageReader<AssetEvent<TileRegistryAsset>>,
    handles: Res<RegistryHandles>,
    assets: Res<Assets<TileRegistryAsset>>,
    mut registry: ResMut<TileRegistry>,
    mut terrain_tiles: ResMut<tile::TerrainTiles>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event {
            if *id == handles.tiles.id() {
                if let Some(asset) = assets.get(&handles.tiles) {
                    let new_reg = TileRegistry::from_defs(asset.tiles.clone());
                    *terrain_tiles = tile::TerrainTiles {
                        air: new_reg.by_name("air"),
                        grass: new_reg.by_name("grass"),
                        dirt: new_reg.by_name("dirt"),
                        stone: new_reg.by_name("stone"),
                    };
                    *registry = new_reg;
                    info!("Hot-reloaded TileRegistry ({} tiles)", asset.tiles.len());
                }
            }
        }
    }
}

fn hot_reload_parallax(
    mut commands: Commands,
    mut events: MessageReader<AssetEvent<ParallaxConfigAsset>>,
    handles: Res<RegistryHandles>,
    assets: Res<Assets<ParallaxConfigAsset>>,
    mut config: ResMut<ParallaxConfig>,
    layer_query: Query<Entity, With<crate::parallax::spawn::ParallaxLayer>>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event {
            if *id == handles.parallax.id() {
                if let Some(asset) = assets.get(&handles.parallax) {
                    config.layers = asset.layers.clone();
                    // Despawn existing layers so spawn system recreates them next frame
                    for entity in &layer_query {
                        commands.entity(entity).despawn();
                    }
                    info!(
                        "Hot-reloaded ParallaxConfig ({} layers), despawned old entities",
                        asset.layers.len()
                    );
                }
            }
        }
    }
}
