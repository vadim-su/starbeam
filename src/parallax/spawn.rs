use bevy::prelude::*;

use super::config::ParallaxConfig;

/// Marker component for a parallax layer entity.
#[derive(Component)]
pub struct ParallaxLayer {
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

/// Spawn parallax layer entities from config.
/// Runs on OnEnter(InGame) and also in Update to respawn after hot-reload despawn.
pub fn spawn_parallax_layers(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    config: Res<ParallaxConfig>,
    existing: Query<Entity, With<ParallaxLayer>>,
) {
    // Don't double-spawn if layers already exist
    if !existing.is_empty() {
        return;
    }

    for layer_def in &config.layers {
        let image_handle: Handle<Image> = asset_server.load(&layer_def.image);

        commands.spawn((
            ParallaxLayer {
                speed_x: layer_def.speed_x,
                speed_y: layer_def.speed_y,
                repeat_x: layer_def.repeat_x,
                repeat_y: layer_def.repeat_y,
                texture_size: Vec2::ZERO,
                initialized: false,
            },
            Sprite::from_image(image_handle),
            Transform::from_xyz(0.0, 0.0, layer_def.z_order),
        ));
    }

    info!("Spawned {} parallax layers", config.layers.len());
}
