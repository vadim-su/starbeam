use bevy::prelude::*;

use crate::player::{Grounded, Player, Velocity};
use crate::registry::player::PlayerConfig;

const VELOCITY_DEADZONE: f32 = 0.1;

/// Loaded animation frame handles.
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
}

/// Load character animation frames (runs once on InGame enter, before spawn_player).
pub fn load_character_animations(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(CharacterAnimations {
        idle: vec![asset_server.load("characters/advanturer/animations/staying/frame_000.png")],
        running: vec![
            asset_server.load("characters/advanturer/animations/running/frame_000.png"),
            asset_server.load("characters/advanturer/animations/running/frame_001.png"),
            asset_server.load("characters/advanturer/animations/running/frame_002.png"),
            asset_server.load("characters/advanturer/animations/running/frame_003.png"),
        ],
        jumping: vec![
            asset_server.load("characters/advanturer/animations/jumping/frame_000.png"),
            asset_server.load("characters/advanturer/animations/jumping/frame_001.png"),
            asset_server.load("characters/advanturer/animations/jumping/frame_002.png"),
            asset_server.load("characters/advanturer/animations/jumping/frame_003.png"),
            asset_server.load("characters/advanturer/animations/jumping/frame_004.png"),
            asset_server.load("characters/advanturer/animations/jumping/frame_005.png"),
            asset_server.load("characters/advanturer/animations/jumping/frame_006.png"),
        ],
    });
}

/// Advance animation frames and switch states based on velocity.
pub fn animate_player(
    time: Res<Time>,
    animations: Res<CharacterAnimations>,
    player_config: Res<PlayerConfig>,
    mut query: Query<(&mut AnimationState, &mut Sprite, &Velocity, &Grounded), With<Player>>,
) {
    for (mut anim, mut sprite, velocity, grounded) in &mut query {
        // Determine animation kind
        let new_kind = if !grounded.0 {
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
                sprite.image = frames[0].clone();
            }
        }

        // Update facing direction
        if velocity.x > VELOCITY_DEADZONE {
            anim.facing_right = true;
        }
        if velocity.x < -VELOCITY_DEADZONE {
            anim.facing_right = false;
        }
        sprite.flip_x = !anim.facing_right;

        // Frame advancement depends on animation kind
        match anim.kind {
            AnimationKind::Jumping => {
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
                        sprite.image = frames[new_frame].clone();
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
                        sprite.image = frames[anim.frame].clone();
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
    }
}
