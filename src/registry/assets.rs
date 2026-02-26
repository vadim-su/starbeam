use std::collections::HashMap;

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

/// A single sprite variant within a bitmask mapping.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // col, index: present for RON compatibility with autotile47.py output
pub struct SpriteVariant {
    pub row: u32,
    pub weight: f32,
    #[serde(default)]
    pub col: u32,
    #[serde(default)]
    pub index: u32,
}

/// Mapping for a single bitmask value: description + weighted variants.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // description: present for RON readability, not used at runtime
pub struct BitmaskMapping {
    #[serde(default)]
    pub description: String,
    pub variants: Vec<SpriteVariant>,
}

/// Asset loaded from *.autotile.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
#[allow(dead_code)] // atlas_columns: reserved, not yet used at runtime
pub struct AutotileAsset {
    pub tile_size: u32,
    pub atlas_columns: u32,
    pub atlas_rows: u32,
    pub tiles: HashMap<u8, BitmaskMapping>,
}
