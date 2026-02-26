---
source: Context7 API (docs.rs/bevy/latest)
library: Bevy
package: bevy
topic: Sprite animation with Timer, TimerMode, sprite sheet animation
fetched: 2026-02-26T00:00:00Z
official_docs: https://docs.rs/bevy/latest/bevy/prelude/struct.Timer.html
---

# Sprite Animation & Timer API (Bevy 0.18)

## Timer::from_seconds

**Confirmed**: `Timer::from_seconds(duration, TimerMode::Repeating)` exists in Bevy 0.18.

```rust
use std::time::Duration;

// Create a repeating timer (fires every 0.1 seconds)
let timer = Timer::from_seconds(0.1, TimerMode::Repeating);

// Create a one-shot timer
let timer = Timer::from_seconds(1.0, TimerMode::Once);
```

### Timer::tick()

Advances the timer by a duration. For repeating timers, elapsed time wraps around.

```rust
use std::time::Duration;
let mut timer = Timer::from_seconds(1.0, TimerMode::Once);
let mut repeating = Timer::from_seconds(1.0, TimerMode::Repeating);
timer.tick(Duration::from_secs_f32(1.5));
repeating.tick(Duration::from_secs_f32(1.5));
assert_eq!(timer.elapsed_secs(), 1.0);    // clamped at duration
assert_eq!(repeating.elapsed_secs(), 0.5); // wraps around
```

### Timer::just_finished()

Returns `true` if the timer finished during the last `tick()` call.

## TimerMode

```rust
pub enum TimerMode {
    Once,       // Timer stops after finishing
    Repeating,  // Timer resets and repeats
}
```

## Complete Sprite Sheet Animation Example (Official Bevy Example)

```rust
use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(ImagePlugin::default_nearest())) // prevents blurry sprites
        .add_systems(Startup, setup)
        .add_systems(Update, animate_sprite)
        .run();
}

#[derive(Component)]
struct AnimationIndices {
    first: usize,
    last: usize,
}

#[derive(Component, Deref, DerefMut)]
struct AnimationTimer(Timer);

fn animate_sprite(
    time: Res<Time>,
    mut query: Query<(&AnimationIndices, &mut AnimationTimer, &mut Sprite)>,
) {
    for (indices, mut timer, mut sprite) in &mut query {
        timer.tick(time.delta());

        if timer.just_finished()
            && let Some(atlas) = &mut sprite.texture_atlas
        {
            atlas.index = if atlas.index == indices.last {
                indices.first
            } else {
                atlas.index + 1
            };
        }
    }
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let texture = asset_server.load("textures/rpg/chars/gabe/gabe-idle-run.png");
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(24), 7, 1, None, None);
    let texture_atlas_layout = texture_atlas_layouts.add(layout);
    let animation_indices = AnimationIndices { first: 1, last: 6 };

    commands.spawn(Camera2d);

    commands.spawn((
        Sprite::from_atlas_image(
            texture,
            TextureAtlas {
                layout: texture_atlas_layout,
                index: animation_indices.first,
            },
        ),
        Transform::from_scale(Vec3::splat(6.0)),
        animation_indices,
        AnimationTimer(Timer::from_seconds(0.1, TimerMode::Repeating)),
    ));
}
```

## Individual Image Animation (No Sprite Sheet)

For swapping between separate image files instead of a sprite sheet atlas:

```rust
#[derive(Component)]
struct AnimationFrames {
    frames: Vec<Handle<Image>>,
    current: usize,
}

#[derive(Component, Deref, DerefMut)]
struct AnimationTimer(Timer);

fn animate_individual_sprites(
    time: Res<Time>,
    mut query: Query<(&mut Sprite, &mut AnimationTimer, &mut AnimationFrames)>,
) {
    for (mut sprite, mut timer, mut frames) in &mut query {
        timer.0.tick(time.delta());
        if timer.0.just_finished() {
            frames.current = (frames.current + 1) % frames.frames.len();
            sprite.image = frames.frames[frames.current].clone();
        }
    }
}
```

### Key Notes

- `time.delta()` returns `Duration` — pass directly to `timer.tick()`
- `timer.just_finished()` checks if the timer completed in the last tick
- For sprite sheets: mutate `sprite.texture_atlas.as_mut().unwrap().index`
- For individual images: mutate `sprite.image` directly with a new `Handle<Image>`
