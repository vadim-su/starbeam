use bevy::prelude::*;

/// Marker component for a parallax layer entity.
#[derive(Component)]
pub struct ParallaxLayer {
    pub biome_id: String,
    pub speed_x: f32,
    pub speed_y: f32,
    pub repeat_x: bool,
    pub repeat_y: bool,
    pub texture_size: Vec2, // filled once image loads, starts as Vec2::ZERO
    pub initialized: bool,  // for repeat tiling (Task 4)
}

/// Marker for individual tile sprites within a repeating parallax layer.
/// These are spawned as children of the `ParallaxLayer` entity.
#[derive(Component)]
pub struct ParallaxTile;
