use bevy::prelude::*;
use serde::Deserialize;

/// Player parameters loaded from RON.
#[derive(Resource, Debug, Clone, Deserialize)]
pub struct PlayerConfig {
    pub speed: f32,
    pub jump_velocity: f32,
    pub gravity: f32,
    pub width: f32,
    pub height: f32,
}
