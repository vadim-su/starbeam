use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::inventory::Hotbar;
use crate::player::animation::AnimationState;
use crate::player::parts::{ArmAiming, CharacterPart};
use crate::player::Player;

/// Pivot offset: shoulder position relative to sprite center (pixels).
/// 5px above center on a 48x48 canvas.
const SHOULDER_PIVOT_Y: f32 = 5.0;

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

        // Override facing direction when aiming
        if aiming_active {
            anim_state.facing_right = world_pos.x >= player_pos.x;
        }

        // Calculate angle from shoulder to cursor
        let shoulder_world = Vec2::new(player_pos.x, player_pos.y + SHOULDER_PIVOT_Y);
        let delta = world_pos - shoulder_world;
        let angle = delta.y.atan2(delta.x);

        // Update arm children
        for child in children.iter() {
            let Ok((_part, mut aim, mut transform)) = arm_query.get_mut(child) else {
                continue;
            };

            aim.active = aiming_active;

            if aiming_active {
                // Flip angle when facing left: mirror across Y axis
                let facing_right = anim_state.facing_right;
                let arm_angle = if facing_right {
                    angle
                } else {
                    std::f32::consts::PI - angle
                };
                transform.rotation = Quat::from_rotation_z(arm_angle);
            } else {
                // Reset rotation when not aiming
                transform.rotation = Quat::IDENTITY;
            }
        }
    }
}
