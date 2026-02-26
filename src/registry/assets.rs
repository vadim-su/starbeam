use bevy::prelude::*;
use bevy::reflect::TypePath;
use serde::Deserialize;

use super::tile::TileDef;
use crate::parallax::config::ParallaxLayerDef;

/// Asset loaded from tiles.registry.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct TileRegistryAsset {
    pub tiles: Vec<TileDef>,
}

/// Asset loaded from player.def.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct PlayerDefAsset {
    pub speed: f32,
    pub jump_velocity: f32,
    pub gravity: f32,
    pub width: f32,
    pub height: f32,
}

/// Asset loaded from world.config.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct WorldConfigAsset {
    pub width_tiles: i32,
    pub height_tiles: i32,
    pub chunk_size: u32,
    pub tile_size: f32,
    pub chunk_load_radius: i32,
    pub seed: u32,
}

/// Asset loaded from bg.parallax.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct ParallaxConfigAsset {
    pub layers: Vec<ParallaxLayerDef>,
}
