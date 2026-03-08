use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::inventory::Hotbar;
use crate::player::animation::AnimationState;
use crate::player::parts::{ArmAiming, CharacterPart};
use crate::player::Player;

/// Rotates arm children toward the mouse cursor when an item is in the active hotbar slot.
/// Also overrides facing direction on all children based on cursor position.
pub fn arm_aiming_system(
    window_query: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    mut player_query: Query<
        (&GlobalTransform, &mut AnimationState, &Hotbar, &Children),
        With<Player>,
    >,
    mut arm_query: Query<(&CharacterPart, &mut ArmAiming, &mut Transform)>,
) {
    let Ok(window) = window_query.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok((camera, camera_gt)) = camera_query.single() else {
        return;
    };
    let Ok(world_pos) = camera.viewport_to_world_2d(camera_gt, cursor_pos) else {
        return;
    };

    for (player_gt, mut anim_state, hotbar, children) in &mut player_query {
        let player_pos = player_gt.translation().truncate();

        // Determine if aiming is active (any item in active hotbar slot)
        let slot = hotbar.active_slot();
        let aiming_active = slot.left_hand.is_some() || slot.right_hand.is_some();

        // Always face cursor direction
        anim_state.facing_right = world_pos.x >= player_pos.x;
        anim_state.facing_locked = true;

        // Update arm children
        for child in children.iter() {
            let Ok((_part, mut aim, mut transform)) = arm_query.get_mut(child) else {
                continue;
            };

            aim.active = aiming_active;
            let pivot = aim.pivot;

            if aiming_active {
                let facing_right = anim_state.facing_right;

                // Calculate angle from pivot (shoulder) to cursor in world space
                let pivot_world = Vec2::new(
                    player_pos.x,
                    player_pos.y + pivot.y,
                );
                let delta = world_pos - pivot_world;
                let angle = delta.y.atan2(delta.x);

                // Convert to arm-local angle (0 = pointing forward horizontally).
                // When facing left, scale.x is negative which mirrors rotation visually,
                // so we use (angle - PI) instead of (PI - angle) to keep correct direction.
                let raw = if facing_right {
                    angle
                } else {
                    angle - std::f32::consts::PI
                };
                // Normalize to [-PI, PI]
                let arm_angle = if raw > std::f32::consts::PI {
                    raw - std::f32::consts::TAU
                } else if raw < -std::f32::consts::PI {
                    raw + std::f32::consts::TAU
                } else {
                    raw
                };

                // Clamp to natural shoulder range.
                // When facing left, scale.x flip mirrors rotation visually,
                // so swap clamp bounds to keep consistent visual limits.
                let (min_angle, max_angle) = if facing_right {
                    (-15.0_f32.to_radians(), 60.0_f32.to_radians())
                } else {
                    (-60.0_f32.to_radians(), 15.0_f32.to_radians())
                };
                let clamped = arm_angle.clamp(min_angle, max_angle);

                let rot = Quat::from_rotation_z(clamped);
                transform.rotation = rot;

                // Facing flip via scale.x (animate_player skips aiming arms)
                let abs_sx = transform.scale.x.abs();
                transform.scale.x = if facing_right { abs_sx } else { -abs_sx };

                // Pivot rotation: keep shoulder fixed in parent space.
                // Formula: T = pivot - R * pivot
                let pivot3 = Vec3::new(pivot.x, pivot.y, 0.0);
                let rotated_pivot = rot * pivot3;
                transform.translation.x = pivot3.x - rotated_pivot.x;
                transform.translation.y = pivot3.y - rotated_pivot.y;
            } else {
                // Apply default resting angle
                let default_rot = Quat::from_rotation_z(aim.default_angle);
                transform.rotation = default_rot;

                // Apply pivot offset for default angle
                if aim.default_angle.abs() > 0.001 {
                    let pivot3 = Vec3::new(pivot.x, pivot.y, 0.0);
                    let rotated_pivot = default_rot * pivot3;
                    transform.translation.x = pivot3.x - rotated_pivot.x;
                    transform.translation.y = pivot3.y - rotated_pivot.y;
                } else {
                    transform.translation.x = 0.0;
                    transform.translation.y = 0.0;
                }
            }
        }
    }
}
