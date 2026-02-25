pub mod assets;
pub mod loader;
pub mod player;
pub mod tile;
pub mod world;

use bevy::asset::AssetEvent;
use bevy::ecs::message::MessageReader;
use bevy::prelude::*;

use assets::{PlayerDefAsset, TileRegistryAsset, WorldConfigAsset};
use loader::RonLoader;
use player::PlayerConfig;
use tile::TileRegistry;
use world::WorldConfig;

/// Keeps asset handles alive for hot-reload detection.
#[derive(Resource)]
pub struct RegistryHandles {
    pub tiles: Handle<TileRegistryAsset>,
    pub player: Handle<PlayerDefAsset>,
    pub world_config: Handle<WorldConfigAsset>,
}

/// Application state: Loading waits for assets, InGame runs gameplay.
#[derive(States, Default, Debug, Clone, Eq, PartialEq, Hash)]
pub enum AppState {
    #[default]
    Loading,
    InGame,
}

/// Handles for assets being loaded.
#[derive(Resource)]
struct LoadingAssets {
    tiles: Handle<TileRegistryAsset>,
    player: Handle<PlayerDefAsset>,
    world_config: Handle<WorldConfigAsset>,
}

pub struct RegistryPlugin;

impl Plugin for RegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .init_asset::<TileRegistryAsset>()
            .init_asset::<PlayerDefAsset>()
            .init_asset::<WorldConfigAsset>()
            .register_asset_loader(RonLoader::<TileRegistryAsset>::new(&["registry.ron"]))
            .register_asset_loader(RonLoader::<PlayerDefAsset>::new(&["def.ron"]))
            .register_asset_loader(RonLoader::<WorldConfigAsset>::new(&["config.ron"]))
            .add_systems(Startup, start_loading)
            .add_systems(Update, check_loading.run_if(in_state(AppState::Loading)))
            .add_systems(
                Update,
                (
                    hot_reload_player,
                    hot_reload_world,
                    hot_reload_tiles,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn start_loading(mut commands: Commands, asset_server: Res<AssetServer>) {
    let tiles = asset_server.load::<TileRegistryAsset>("data/tiles.registry.ron");
    let player = asset_server.load::<PlayerDefAsset>("data/player.def.ron");
    let world_config = asset_server.load::<WorldConfigAsset>("data/world.config.ron");
    commands.insert_resource(LoadingAssets {
        tiles,
        player,
        world_config,
    });
}

fn check_loading(
    mut commands: Commands,
    loading: Res<LoadingAssets>,
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

    // Keep handles alive for hot-reload
    commands.insert_resource(RegistryHandles {
        tiles: loading.tiles.clone(),
        player: loading.player.clone(),
        world_config: loading.world_config.clone(),
    });
    commands.remove_resource::<LoadingAssets>();
    next_state.set(AppState::InGame);
    info!("All registry assets loaded, entering InGame state");
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
