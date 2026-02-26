use bevy::prelude::*;
use serde::Deserialize;

/// Definition of a single parallax layer from RON.
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

/// Runtime resource holding the parallax configuration.
#[derive(Resource, Debug, Clone, Deserialize)]
pub struct ParallaxConfig {
    pub layers: Vec<ParallaxLayerDef>,
}
