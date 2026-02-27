use bevy::prelude::*;

use crate::registry::biome::BiomeId;

/// Immutable config for a parallax layer entity.
#[derive(Component)]
pub struct ParallaxLayerConfig {
    pub biome_id: BiomeId,
    pub speed_x: f32,
    pub speed_y: f32,
    pub repeat_x: bool,
    pub repeat_y: bool,
}

/// Mutable runtime state for a parallax layer entity.
#[derive(Component, Default)]
pub struct ParallaxLayerState {
    pub texture_size: Vec2,
    pub initialized: bool,
}

/// Marker for individual tile sprites within a repeating parallax layer.
/// These are spawned as children of the `ParallaxLayerConfig` entity.
#[derive(Component)]
pub struct ParallaxTile;
