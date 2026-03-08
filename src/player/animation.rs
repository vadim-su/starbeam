use std::collections::HashMap;

use bevy::prelude::*;
use bevy::sprite_render::MeshMaterial2d;

use crate::physics::{Grounded, Submerged, Velocity};
use crate::player::parts::{ArmAiming, CharacterPart, PartType};
use crate::player::Player;
use crate::registry::loading::CharacterAnimConfig;
use crate::registry::player::PlayerConfig;
use crate::world::lit_sprite::LitSpriteMaterial;

const VELOCITY_DEADZONE: f32 = 0.1;

/// Animation frames for a single body part.
#[derive(Debug, Default)]
pub struct PartAnimFrames {
    pub idle: Vec<Handle<Image>>,
    pub running: Vec<Handle<Image>>,
    pub jumping: Vec<Handle<Image>>,
}

/// Loaded animation frame handles for all body parts.
#[derive(Resource)]
pub struct CharacterAnimations {
    pub parts: HashMap<PartType, PartAnimFrames>,
}

impl CharacterAnimations {
    /// Get frames for a specific part and animation kind.
    pub fn frames_for(&self, part: PartType, kind: AnimationKind) -> &[Handle<Image>] {
        self.parts
            .get(&part)
            .map(|p| match kind {
                AnimationKind::Idle => p.idle.as_slice(),
                AnimationKind::Running => p.running.as_slice(),
                AnimationKind::Jumping | AnimationKind::Swimming => p.jumping.as_slice(),
            })
            .unwrap_or(&[])
    }

    /// Max frame count across all parts for a given animation kind.
    /// Used as the master frame count for animation advancement.
    pub fn max_frame_count(&self, kind: AnimationKind) -> usize {
        self.parts
            .values()
            .map(|p| match kind {
                AnimationKind::Idle => p.idle.len(),
                AnimationKind::Running => p.running.len(),
                AnimationKind::Jumping | AnimationKind::Swimming => p.jumping.len(),
            })
            .max()
            .unwrap_or(0)
    }
}

/// Current animation state on the player entity.
#[derive(Component)]
pub struct AnimationState {
    pub kind: AnimationKind,
    pub frame: usize,
    pub timer: Timer,
    pub facing_right: bool,
    /// True when moving in the opposite direction of facing (running backwards).
    pub running_backwards: bool,
    /// When true, the aiming system controls facing direction (cursor-based).
    pub facing_locked: bool,
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
    let mut parts_map = HashMap::new();

    // Discover frame files from a directory on disk (sorted by name).
    let scan_frames = |sprite_dir: &str, anim_name: &str| -> Vec<String> {
        let dir = format!("assets/{base}{sprite_dir}/{anim_name}");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut files: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .is_some_and(|ext| ext == "png")
            })
            .map(|e| {
                format!(
                    "{sprite_dir}/{anim_name}/{}",
                    e.file_name().to_string_lossy()
                )
            })
            .collect();
        files.sort();
        files
    };

    // Load frames for a part. If AnimationDef has explicit frames, use those
    // (replacing body_sprite_dir with this part's sprite_dir). Otherwise,
    // auto-scan the part's directory for frame_*.png files.
    let load_part = |sprite_dir: &str, body_sprite_dir: &str| -> PartAnimFrames {
        let load_anim = |anim_name: &str| -> Vec<Handle<Image>> {
            let frames: Vec<String> = anim_config
                .animations
                .get(anim_name)
                .filter(|def| !def.frames.is_empty())
                .map(|def| {
                    def.frames
                        .iter()
                        .map(|frame| frame.replacen(body_sprite_dir, sprite_dir, 1))
                        .collect()
                })
                .unwrap_or_else(|| scan_frames(sprite_dir, anim_name));

            frames
                .iter()
                .map(|frame| asset_server.load(format!("{base}{frame}")))
                .collect()
        };
        PartAnimFrames {
            idle: load_anim("staying"),
            running: load_anim("running"),
            jumping: load_anim("jumping"),
        }
    };

    if let Some(ref parts_def) = anim_config.parts {
        let body_dir = &parts_def.body.sprite_dir;
        parts_map.insert(PartType::Body, load_part(body_dir, body_dir));
        if let Some(ref head) = parts_def.head {
            parts_map.insert(PartType::Head, load_part(&head.sprite_dir, body_dir));
        }
        if let Some(ref legs) = parts_def.legs {
            parts_map.insert(PartType::Legs, load_part(&legs.sprite_dir, body_dir));
        }
        if let Some(ref hand_right) = parts_def.hand_right {
            parts_map.insert(PartType::FrontArm, load_part(&hand_right.sprite_dir, body_dir));
        }
        if let Some(ref hand_left) = parts_def.hand_left {
            parts_map.insert(PartType::BackArm, load_part(&hand_left.sprite_dir, body_dir));
        }
    } else {
        // Legacy mode: load all frames under Body
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
        parts_map.insert(
            PartType::Body,
            PartAnimFrames {
                idle: load_frames("staying"),
                running: load_frames("running"),
                jumping: load_frames("jumping"),
            },
        );
    }

    commands.insert_resource(CharacterAnimations { parts: parts_map });
}

/// Advance animation frames and switch states based on velocity.
///
/// Iterates all child `CharacterPart` entities to update their sprite textures
/// and facing direction via `Transform.scale.x` (negative = flip horizontally).
pub fn animate_player(
    time: Res<Time>,
    animations: Res<CharacterAnimations>,
    player_config: Res<PlayerConfig>,
    mut materials: ResMut<Assets<LitSpriteMaterial>>,
    mut player_query: Query<
        (
            &mut AnimationState,
            &Velocity,
            &Grounded,
            Option<&Submerged>,
            &Children,
        ),
        With<Player>,
    >,
    mut part_query: Query<(
        &CharacterPart,
        &MeshMaterial2d<LitSpriteMaterial>,
        &mut Transform,
        Option<&ArmAiming>,
    )>,
) {
    for (mut anim, velocity, grounded, submerged, children) in &mut player_query {
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

        // Reset frame on state change
        let kind_changed = new_kind != anim.kind;
        if kind_changed {
            anim.kind = new_kind;
            anim.frame = 0;
            anim.timer.reset();
        }

        // Detect running backwards: moving opposite to facing direction.
        // Facing is set by aiming system (cursor position); if not aiming,
        // fall back to velocity-based facing.
        if !anim.facing_locked {
            if velocity.x > VELOCITY_DEADZONE {
                anim.facing_right = true;
            }
            if velocity.x < -VELOCITY_DEADZONE {
                anim.facing_right = false;
            }
        }
        anim.running_backwards = if anim.kind == AnimationKind::Running {
            (anim.facing_right && velocity.x < -VELOCITY_DEADZONE)
                || (!anim.facing_right && velocity.x > VELOCITY_DEADZONE)
        } else {
            false
        };

        // Frame advancement depends on animation kind
        let mut new_frame = anim.frame;
        let total_frames = animations.max_frame_count(anim.kind);
        match anim.kind {
            AnimationKind::Jumping | AnimationKind::Swimming => {
                if total_frames > 0 {
                    let half = total_frames / 2;
                    let jump_vel = player_config.jump_velocity;
                    new_frame = if velocity.y > 0.0 {
                        let t = 1.0 - (velocity.y / jump_vel).clamp(0.0, 1.0);
                        (t * half as f32) as usize
                    } else {
                        let t = (-velocity.y / jump_vel).clamp(0.0, 1.0);
                        half + (t * (total_frames - 1 - half) as f32) as usize
                    };
                    new_frame = new_frame.min(total_frames - 1);
                }
            }
            _ => {
                anim.timer.tick(time.delta());
                if anim.timer.just_finished() && total_frames > 0 {
                    if anim.running_backwards {
                        new_frame = if anim.frame == 0 {
                            total_frames - 1
                        } else {
                            anim.frame - 1
                        };
                    } else {
                        new_frame = (anim.frame + 1) % total_frames;
                    }
                }
            }
        }

        let frame_changed = new_frame != anim.frame || kind_changed;
        anim.frame = new_frame;

        // Update all child part sprites
        if frame_changed {
            for child in children.iter() {
                let Ok((part, mat_handle, _, aim)) = part_query.get(child) else {
                    continue;
                };
                // Skip frame update if this arm is actively aiming (use idle frame 0)
                if aim.is_some_and(|a| a.active) {
                    let frames = animations.frames_for(part.0, AnimationKind::Idle);
                    if !frames.is_empty() {
                        if let Some(mat) = materials.get_mut(&mat_handle.0) {
                            mat.sprite = frames[0].clone();
                        }
                    }
                    continue;
                }
                let frames = animations.frames_for(part.0, anim.kind);
                if !frames.is_empty() {
                    let idx = anim.frame.min(frames.len() - 1);
                    if let Some(mat) = materials.get_mut(&mat_handle.0) {
                        mat.sprite = frames[idx].clone();
                    }
                }
            }
        }

        // Update facing on all children
        for child in children.iter() {
            let Ok((_, _, mut transform, aim)) = part_query.get_mut(child) else {
                continue;
            };
            // Skip facing override for aiming arms (aiming system handles their transform)
            if aim.is_some_and(|a| a.active) {
                continue;
            }
            let abs_scale_x = transform.scale.x.abs();
            transform.scale.x = if anim.facing_right {
                abs_scale_x
            } else {
                -abs_scale_x
            };
        }
    }
}
