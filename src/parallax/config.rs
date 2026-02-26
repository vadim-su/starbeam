use bevy::prelude::*;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ParallaxLayerDef {
    pub name: String,
    pub image: String,
    pub speed_x: f32,
    pub speed_y: f32,
    pub repeat_x: bool,
    pub repeat_y: bool,
    pub z_order: f32,
}

#[derive(Resource, Debug, Clone)]
pub struct ParallaxConfig {
    pub layers: Vec<ParallaxLayerDef>,
}
