use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::spawn::{ParallaxLayer, ParallaxTile};

/// Scroll parallax layers based on camera position.
///
/// Each layer's position is computed as:
///   `cam_pos * (1.0 - speed)`
///
/// - speed=0.0 → layer follows camera (static on screen, e.g. sky)
/// - speed=0.5 → layer moves at half camera speed (mid-depth)
/// - speed=1.0 → layer is fixed in world (moves with tiles)
///
/// For layers with `repeat_x` or `repeat_y`, child tile sprites are spawned
/// on first initialization and repositioned each frame with wrapping to create
/// seamless tiling across the visible area.
pub fn parallax_scroll(
    mut commands: Commands,
    camera_query: Query<(&Transform, &Projection), With<Camera2d>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    images: Res<Assets<Image>>,
    mut layer_query: Query<
        (
            Entity,
            &mut ParallaxLayer,
            &mut Transform,
            &mut Visibility,
            &Sprite,
        ),
        (Without<Camera2d>, Without<ParallaxTile>),
    >,
    mut tile_query: Query<
        &mut Transform,
        (
            With<ParallaxTile>,
            Without<Camera2d>,
            Without<ParallaxLayer>,
        ),
    >,
    children_query: Query<&Children>,
) {
    let Ok((camera_tf, projection)) = camera_query.single() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

    let proj_scale = match projection {
        Projection::Orthographic(ortho) => ortho.scale,
        _ => 1.0,
    };

    let cam_x = camera_tf.translation.x;
    let cam_y = camera_tf.translation.y;
    let visible_w = window.width() * proj_scale;
    let visible_h = window.height() * proj_scale;

    for (entity, mut layer, mut transform, mut visibility, sprite) in &mut layer_query {
        // Resolve texture size on first frame the image is available
        if layer.texture_size == Vec2::ZERO {
            if let Some(image) = images.get(&sprite.image) {
                layer.texture_size = image.size_f32();
            } else {
                continue; // image not loaded yet, skip this layer
            }
        }

        let tex_w = layer.texture_size.x;
        let tex_h = layer.texture_size.y;

        // Initialize repeat tiling: hide parent sprite, spawn child tiles
        if (layer.repeat_x || layer.repeat_y) && !layer.initialized {
            *visibility = Visibility::Hidden;

            let copies_x = if layer.repeat_x {
                (visible_w / tex_w).ceil() as i32 + 2
            } else {
                1
            };
            let copies_y = if layer.repeat_y {
                (visible_h / tex_h).ceil() as i32 + 2
            } else {
                1
            };

            let image_handle = sprite.image.clone();

            commands.entity(entity).with_children(|parent| {
                for iy in 0..copies_y {
                    for ix in 0..copies_x {
                        parent.spawn((
                            ParallaxTile,
                            Sprite::from_image(image_handle.clone()),
                            Transform::from_xyz(ix as f32 * tex_w, iy as f32 * tex_h, 0.0),
                        ));
                    }
                }
            });

            layer.initialized = true;
            info!(
                "Initialized parallax tiling: {}x{} copies for {}x{} texture",
                copies_x, copies_y, tex_w, tex_h
            );
        }

        // Parallax positioning — preserve z-order set at spawn
        let z = transform.translation.z;

        if layer.initialized {
            // Repeat layer: position parent at parallax offset, reposition children with wrapping
            let base_x = cam_x * (1.0 - layer.speed_x);
            let base_y = cam_y * (1.0 - layer.speed_y);

            transform.translation.x = base_x;
            transform.translation.y = base_y;
            transform.translation.z = z;

            // In local space, the camera center is at (cam_x - base_x, cam_y - base_y).
            // We need to tile around that point.
            let local_cam_x = cam_x - base_x; // = cam_x * speed_x
            let local_cam_y = cam_y - base_y; // = cam_y * speed_y

            // Wrapping offset: the fractional position within one texture period.
            // This determines how the tile grid shifts as the camera moves.
            let wrap_x = if layer.repeat_x && tex_w > 0.0 {
                local_cam_x.rem_euclid(tex_w)
            } else {
                0.0
            };
            let wrap_y = if layer.repeat_y && tex_h > 0.0 {
                local_cam_y.rem_euclid(tex_h)
            } else {
                0.0
            };

            // Reposition child tiles in local space (relative to parent).
            // Grid is anchored so that tiles seamlessly cover the visible area
            // centered on the camera's local-space position.
            let copies_x = if layer.repeat_x {
                (visible_w / tex_w).ceil() as i32 + 2
            } else {
                1
            };

            if let Ok(children) = children_query.get(entity) {
                let mut idx = 0;
                for child in children.iter() {
                    if let Ok(mut child_tf) = tile_query.get_mut(child) {
                        let ix = idx % copies_x;
                        let iy = idx / copies_x;

                        // Anchor the grid at the camera's local position.
                        // Start one tile before the left edge of the visible area,
                        // offset by the wrap amount for seamless scrolling.
                        child_tf.translation.x = if layer.repeat_x {
                            local_cam_x - wrap_x + (ix as f32 - 1.0) * tex_w - visible_w / 2.0
                                + tex_w / 2.0
                        } else {
                            0.0
                        };

                        child_tf.translation.y = if layer.repeat_y {
                            local_cam_y - wrap_y + (iy as f32 - 1.0) * tex_h - visible_h / 2.0
                                + tex_h / 2.0
                        } else {
                            0.0
                        };

                        idx += 1;
                    }
                }
            }
        } else {
            // Non-repeat layer: simple parallax position
            transform.translation.x = cam_x * (1.0 - layer.speed_x);
            transform.translation.y = cam_y * (1.0 - layer.speed_y);
            transform.translation.z = z;
        }
    }
}
