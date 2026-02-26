use bevy::prelude::*;

use super::spawn::ParallaxLayer;

/// Scroll parallax layers based on camera position.
///
/// Each layer's position is computed as:
///   `cam_pos * (1.0 - speed)`
///
/// - speed=0.0 → layer follows camera (static on screen, e.g. sky)
/// - speed=0.5 → layer moves at half camera speed (mid-depth)
/// - speed=1.0 → layer is fixed in world (moves with tiles)
pub fn parallax_scroll(
    camera_query: Query<&Transform, With<Camera2d>>,
    images: Res<Assets<Image>>,
    mut layer_query: Query<(&mut ParallaxLayer, &mut Transform, &Sprite), Without<Camera2d>>,
) {
    let Ok(camera_tf) = camera_query.single() else {
        return;
    };

    let cam_x = camera_tf.translation.x;
    let cam_y = camera_tf.translation.y;

    for (mut layer, mut transform, sprite) in &mut layer_query {
        // Resolve texture size on first frame the image is available
        if layer.texture_size == Vec2::ZERO {
            if let Some(image) = images.get(&sprite.image) {
                layer.texture_size = image.size_f32();
            } else {
                continue; // image not loaded yet, skip this layer
            }
        }

        // Parallax positioning — preserve z-order set at spawn
        let z = transform.translation.z;
        transform.translation.x = cam_x * (1.0 - layer.speed_x);
        transform.translation.y = cam_y * (1.0 - layer.speed_y);
        transform.translation.z = z;
    }
}
