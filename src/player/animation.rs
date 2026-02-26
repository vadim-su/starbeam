use bevy::prelude::*;

use crate::player::{Player, Velocity};

const VELOCITY_DEADZONE: f32 = 0.1;

/// Loaded animation frame handles.
#[derive(Resource)]
pub struct CharacterAnimations {
    pub idle: Vec<Handle<Image>>,
    pub running: Vec<Handle<Image>>,
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
    });
}

/// Advance animation frames and switch states based on velocity.
pub fn animate_player(
    time: Res<Time>,
    animations: Res<CharacterAnimations>,
    mut query: Query<(&mut AnimationState, &mut Sprite, &Velocity), With<Player>>,
) {
    for (mut anim, mut sprite, velocity) in &mut query {
        // Determine animation kind from movement
        let new_kind = if velocity.x.abs() > VELOCITY_DEADZONE {
            AnimationKind::Running
        } else {
            AnimationKind::Idle
        };

        // Reset frame on state change and immediately show first frame
        if new_kind != anim.kind {
            anim.kind = new_kind;
            anim.frame = 0;
            anim.timer.reset();
            let frames = match anim.kind {
                AnimationKind::Idle => &animations.idle,
                AnimationKind::Running => &animations.running,
            };
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

        // Advance frame timer
        anim.timer.tick(time.delta());
        if anim.timer.just_finished() {
            let frames = match anim.kind {
                AnimationKind::Idle => &animations.idle,
                AnimationKind::Running => &animations.running,
            };
            if !frames.is_empty() {
                anim.frame = (anim.frame + 1) % frames.len();
                sprite.image = frames[anim.frame].clone();
            }
        }
    }
}
