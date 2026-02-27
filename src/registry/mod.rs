pub mod assets;
pub mod biome;
pub mod hot_reload;
pub mod loader;
pub mod loading;
pub mod player;
pub mod tile;
pub mod world;

use std::collections::HashMap;

use bevy::prelude::*;

use assets::{
    AutotileAsset, BiomeAsset, ParallaxConfigAsset, PlanetTypeAsset, PlayerDefAsset,
    TileRegistryAsset, WorldConfigAsset,
};
use hot_reload::{
    hot_reload_biome_parallax, hot_reload_biomes, hot_reload_planet_type, hot_reload_player,
    hot_reload_tiles, hot_reload_world,
};
use loader::RonLoader;
use loading::{
    check_autotile_loading, check_biomes_loaded, check_loading, start_autotile_loading,
    start_loading,
};

use crate::parallax::config::ParallaxConfig;

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
    LoadingBiomes,
    LoadingAutotile,
    InGame,
}

/// Per-biome parallax configs, keyed by biome ID.
#[derive(Resource, Debug, Default, Clone)]
pub struct BiomeParallaxConfigs {
    pub configs: HashMap<String, ParallaxConfig>,
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
            .init_asset::<PlanetTypeAsset>()
            .init_asset::<BiomeAsset>()
            .register_asset_loader(RonLoader::<PlanetTypeAsset>::new(&["planet.ron"]))
            .register_asset_loader(RonLoader::<BiomeAsset>::new(&["biome.ron"]))
            .add_systems(Startup, start_loading)
            .add_systems(Update, check_loading.run_if(in_state(AppState::Loading)))
            .add_systems(
                Update,
                check_biomes_loaded.run_if(in_state(AppState::LoadingBiomes)),
            )
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
                    hot_reload_biomes,
                    hot_reload_planet_type,
                    hot_reload_biome_parallax,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
