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
    AutotileAsset, BiomeAsset, CharacterDefAsset, ItemDefAsset, ObjectDefAsset,
    ParallaxConfigAsset, PlanetTypeAsset, TileRegistryAsset, WorldConfigAsset,
};
use crate::cosmos::assets::{GenerationConfigAsset, StarTypeAsset};
use biome::BiomeId;
use hot_reload::{
    hot_reload_biome_parallax, hot_reload_biomes, hot_reload_character, hot_reload_objects,
    hot_reload_planet_type, hot_reload_tiles, hot_reload_world,
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
    /// (base_path, handle) pairs for per-object assets; order matters (index 0 = ObjectId::NONE).
    pub objects: Vec<(String, Handle<ObjectDefAsset>)>,
    pub character: Handle<CharacterDefAsset>,
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

/// Per-biome parallax configs, keyed by BiomeId.
#[derive(Resource, Debug, Default, Clone)]
pub struct BiomeParallaxConfigs {
    pub configs: HashMap<BiomeId, ParallaxConfig>,
}

pub struct RegistryPlugin;

impl Plugin for RegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .init_asset::<TileRegistryAsset>()
            .init_asset::<ObjectDefAsset>()
            .init_asset::<CharacterDefAsset>()
            .init_asset::<ItemDefAsset>()
            .init_asset::<WorldConfigAsset>()
            .init_asset::<ParallaxConfigAsset>()
            .init_asset::<AutotileAsset>()
            .register_asset_loader(RonLoader::<TileRegistryAsset>::new(&["registry.ron"]))
            .register_asset_loader(RonLoader::<ObjectDefAsset>::new(&["object.ron"]))
            .register_asset_loader(RonLoader::<CharacterDefAsset>::new(&["character.ron"]))
            .register_asset_loader(RonLoader::<ItemDefAsset>::new(&["item.ron"]))
            .register_asset_loader(RonLoader::<WorldConfigAsset>::new(&["config.ron"]))
            .register_asset_loader(RonLoader::<ParallaxConfigAsset>::new(&["parallax.ron"]))
            .register_asset_loader(RonLoader::<AutotileAsset>::new(&["autotile.ron"]))
            .init_asset::<PlanetTypeAsset>()
            .init_asset::<BiomeAsset>()
            .init_asset::<GenerationConfigAsset>()
            .init_asset::<StarTypeAsset>()
            .register_asset_loader(RonLoader::<PlanetTypeAsset>::new(&["planet.ron"]))
            .register_asset_loader(RonLoader::<BiomeAsset>::new(&["biome.ron"]))
            .register_asset_loader(RonLoader::<GenerationConfigAsset>::new(&["generation.ron"]))
            .register_asset_loader(RonLoader::<StarTypeAsset>::new(&["star.ron"]))
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
                    hot_reload_character,
                    hot_reload_world,
                    hot_reload_tiles,
                    hot_reload_objects,
                    hot_reload_biomes,
                    hot_reload_planet_type,
                    hot_reload_biome_parallax,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
