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
    /// Radius (px) within which dropped items are pulled toward the player.
    #[serde(default = "default_magnet_radius")]
    pub magnet_radius: f32,
    /// Maximum magnetism pull speed (px/s).
    #[serde(default = "default_magnet_strength")]
    pub magnet_strength: f32,
    /// Radius (px) within which items are instantly picked up.
    #[serde(default = "default_pickup_radius")]
    pub pickup_radius: f32,
    /// Vertical impulse (px/s²) when pressing Up/Space in liquid.
    #[serde(default = "default_swim_impulse")]
    pub swim_impulse: f32,
    /// Gravity multiplier while swimming (0.0 = float, 1.0 = full gravity).
    #[serde(default = "default_swim_gravity_factor")]
    pub swim_gravity_factor: f32,
    /// Per-second velocity retention in liquid (0.0 = instant stop, 1.0 = no drag).
    #[serde(default = "default_swim_drag")]
    pub swim_drag: f32,
}

fn default_magnet_radius() -> f32 {
    96.0
}
fn default_magnet_strength() -> f32 {
    400.0
}
fn default_pickup_radius() -> f32 {
    20.0
}
fn default_swim_impulse() -> f32 {
    180.0
}
fn default_swim_gravity_factor() -> f32 {
    0.3
}
fn default_swim_drag() -> f32 {
    0.15
}
