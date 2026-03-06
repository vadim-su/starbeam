use bevy::prelude::*;
use bevy::sprite_render::MeshMaterial2d;

use crate::physics::{Grounded, Submerged, Velocity};
use crate::player::{Player, PLAYER_SPRITE_SIZE};
use crate::registry::loading::CharacterAnimConfig;
use crate::registry::player::PlayerConfig;
use crate::world::lit_sprite::LitSpriteMaterial;

const VELOCITY_DEADZONE: f32 = 0.1;

/// Loaded animation frame handles, built from CharacterAnimConfig data.
#[derive(Resource)]
pub struct CharacterAnimations {
    pub idle: Vec<Handle<Image>>,
    pub running: Vec<Handle<Image>>,
    pub jumping: Vec<Handle<Image>>,
}

/// Current animation state on the player entity.
#[derive(Component)]
pub struct AnimationState {
    pub kind: AnimationKind,
    pub frame: usize,
    pub timer: Timer,
    pub facing_right: bool,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum AnimationKind {
    Idle,
    Running,
    Jumping,
    Swimming,
}

/// Load character animation frames from CharacterAnimConfig (data-driven).
/// Runs once on InGame enter, before spawn_player.
pub fn load_character_animations(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    anim_config: Res<CharacterAnimConfig>,
) {
    let base = &anim_config.base_path;

    // Helper: resolve frame paths relative to the character's base directory
    let load_frames = |anim_name: &str| -> Vec<Handle<Image>> {
        anim_config
            .animations
            .get(anim_name)
            .map(|def| {
                def.frames
                    .iter()
                    .map(|frame| asset_server.load(format!("{base}{frame}")))
                    .collect()
            })
            .unwrap_or_default()
    };

    commands.insert_resource(CharacterAnimations {
        idle: load_frames("staying"),
        running: load_frames("running"),
        jumping: load_frames("jumping"),
    });
}

/// Advance animation frames and switch states based on velocity.
///
/// Uses `MeshMaterial2d<LitSpriteMaterial>` instead of `Sprite`: the animation
/// swaps the sprite texture inside the material, and facing direction is
/// controlled via `Transform.scale.x` (negative = flip horizontally).
pub fn animate_player(
    time: Res<Time>,
    animations: Res<CharacterAnimations>,
    player_config: Res<PlayerConfig>,
    mut materials: ResMut<Assets<LitSpriteMaterial>>,
    mut query: Query<
        (
            &mut AnimationState,
            &MeshMaterial2d<LitSpriteMaterial>,
            &mut Transform,
            &Velocity,
            &Grounded,
            Option<&Submerged>,
        ),
        With<Player>,
    >,
) {
    for (mut anim, mat_handle, mut transform, velocity, grounded, submerged) in &mut query {
        // Determine animation kind
        let is_swimming = submerged.is_some_and(|s| s.is_swimming());

        let new_kind = if is_swimming {
            AnimationKind::Swimming
        } else if !grounded.0 {
            AnimationKind::Jumping
        } else if velocity.x.abs() > VELOCITY_DEADZONE {
            AnimationKind::Running
        } else {
            AnimationKind::Idle
        };

        // Reset frame on state change and immediately show first frame
        if new_kind != anim.kind {
            anim.kind = new_kind;
            anim.frame = 0;
            anim.timer.reset();
            let frames = frames_for_kind(&animations, anim.kind);
            if !frames.is_empty() {
                if let Some(mat) = materials.get_mut(&mat_handle.0) {
                    mat.sprite = frames[0].clone();
                }
            }
        }

        // Update facing direction
        if velocity.x > VELOCITY_DEADZONE {
            anim.facing_right = true;
        }
        if velocity.x < -VELOCITY_DEADZONE {
            anim.facing_right = false;
        }
        // Flip via Transform.scale.x: positive = right, negative = left
        transform.scale.x = if anim.facing_right {
            PLAYER_SPRITE_SIZE
        } else {
            -PLAYER_SPRITE_SIZE
        };

        // Frame advancement depends on animation kind
        match anim.kind {
            AnimationKind::Jumping | AnimationKind::Swimming => {
                // Velocity-based frame selection (not timer-based).
                // Rising (vel.y > 0): frames 0..half (first half)
                // Falling (vel.y <= 0): frames half..end (second half)
                let frames = &animations.jumping;
                if !frames.is_empty() {
                    let half = frames.len() / 2; // 3 for 7 frames
                    let jump_vel = player_config.jump_velocity;
                    let new_frame = if velocity.y > 0.0 {
                        let t = 1.0 - (velocity.y / jump_vel).clamp(0.0, 1.0);
                        (t * half as f32) as usize
                    } else {
                        let t = (-velocity.y / jump_vel).clamp(0.0, 1.0);
                        half + (t * (frames.len() - 1 - half) as f32) as usize
                    };
                    let new_frame = new_frame.min(frames.len() - 1);
                    if anim.frame != new_frame {
                        anim.frame = new_frame;
                        if let Some(mat) = materials.get_mut(&mat_handle.0) {
                            mat.sprite = frames[new_frame].clone();
                        }
                    }
                }
            }
            _ => {
                // Timer-based cycling for Idle and Running
                anim.timer.tick(time.delta());
                if anim.timer.just_finished() {
                    let frames = frames_for_kind(&animations, anim.kind);
                    if !frames.is_empty() {
                        anim.frame = (anim.frame + 1) % frames.len();
                        if let Some(mat) = materials.get_mut(&mat_handle.0) {
                            mat.sprite = frames[anim.frame].clone();
                        }
                    }
                }
            }
        }
    }
}

fn frames_for_kind(animations: &CharacterAnimations, kind: AnimationKind) -> &[Handle<Image>] {
    match kind {
        AnimationKind::Idle => &animations.idle,
        AnimationKind::Running => &animations.running,
        AnimationKind::Jumping => &animations.jumping,
        AnimationKind::Swimming => &animations.jumping, // placeholder until swimming sprites exist
    }
}
