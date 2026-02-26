# Scale, Tile Size & Player Sprite Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Switch to 1:1 camera scale, 8x8 pixel tiles (Starbound-style), and replace the colored rectangle player with an animated sprite.

**Architecture:** Config-only changes for scale/tile-size, regenerate tile atlas via existing Python script, add a sprite animation system with idle/running states loaded from individual frame PNGs.

**Tech Stack:** Bevy 0.18, PIL (tile atlas gen), RON configs with hot-reload

---

## Context: Dimension math

| Parameter | Old | New |
|-----------|-----|-----|
| Camera scale | 2.0 | 1.0 |
| Tile size | 32px | 8px |
| Viewport (world px) | 640×360 | 1280×720 |
| Tiles visible | ~20×11 | ~160×90 |
| Player hitbox | 64×128px (2×4 tiles) | 16×32px (2×4 tiles) |
| Player sprite | colored rect | 44×44px frames |
| Atlas size | 96×32 (3×32px) | 24×8 (3×8px) |

Physics retuned for 8px tiles (all hot-reloadable via `player.def.ron`):
- speed: 150 → ~19 tiles/s
- jump_velocity: 200 → ~5 tile jump height
- gravity: 500

---

### Task 1: Config changes

**Files:**
- Modify: `src/main.rs` (line 43: scale)
- Modify: `assets/data/world.config.ron` (tile_size)
- Modify: `assets/data/player.def.ron` (all values)

**Step 1: Change camera scale**

`src/main.rs` line 43: `scale: 2.0` → `scale: 1.0`

**Step 2: Change tile size**

`assets/data/world.config.ron`:
```ron
(
  width_tiles: 2048,
  height_tiles: 1024,
  chunk_size: 32,
  tile_size: 8.0,
  chunk_load_radius: 3,
  seed: 42,
)
```

**Step 3: Retune player physics for 8px tiles**

`assets/data/player.def.ron`:
```ron
(
  speed: 150.0,
  jump_velocity: 200.0,
  gravity: 500.0,
  width: 16.0,
  height: 32.0,
)
```

**Step 4: Commit**
```
feat: switch to scale 1.0 and 8px tiles
```

---

### Task 2: Regenerate tile atlas for 8px tiles

**Files:**
- Modify: `scripts/gen_tiles.py` (TILE=8, adjust art for smaller canvas)
- Regenerate: `assets/terrain/tiles.png` (output: 24×8)

**Step 1: Update gen_tiles.py**

Key changes:
- `TILE = 8` (was 32)
- Grass: 2px green top, 6px dirt body, minimal variation
- Dirt: brown base + 3-4 spot variations, no clusters
- Stone: gray base + 2-3 light specks + 1 short crack (3-4px)
- Atlas output: 24×8

**Step 2: Run script**
```bash
python scripts/gen_tiles.py
```
Expected: `Saved .../assets/terrain/tiles.png  (24x8)`

**Step 3: Commit**
```
art: regenerate tile atlas for 8px tiles
```

---

### Task 3: Convert metadata.ron from JSON to RON

**Files:**
- Modify: `assets/characters/advanturer/metadata.ron`

**Step 1: Rewrite as valid RON**

```ron
(
  name: "adventurer",
  size: (width: 44, height: 44),
  animations: {
    "staying": [
      "characters/advanturer/animations/staying/frame_000.png",
    ],
    "running": [
      "characters/advanturer/animations/running/frame_000.png",
      "characters/advanturer/animations/running/frame_001.png",
      "characters/advanturer/animations/running/frame_002.png",
      "characters/advanturer/animations/running/frame_003.png",
    ],
  },
)
```

Note: paths changed to full asset paths (from `assets/` root) for easy `asset_server.load()`.

**Step 2: Commit**
```
chore: convert character metadata from JSON to RON
```

---

### Task 4: Player sprite & animation system

**Files:**
- Create: `src/player/animation.rs` (~80 lines)
- Modify: `src/player/mod.rs` (add module, update spawn_player, register systems)

**Step 1: Create animation module**

`src/player/animation.rs`:

```rust
use bevy::prelude::*;
use crate::player::{Player, Velocity};

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

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum AnimationKind {
    Idle,
    Running,
}

/// Load character animation frames (runs once on InGame enter, before spawn_player).
pub fn load_character_animations(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(CharacterAnimations {
        idle: vec![
            asset_server.load("characters/advanturer/animations/staying/frame_000.png"),
        ],
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
        let new_kind = if velocity.x.abs() > 0.1 {
            AnimationKind::Running
        } else {
            AnimationKind::Idle
        };

        // Reset frame on state change
        if new_kind != anim.kind {
            anim.kind = new_kind;
            anim.frame = 0;
            anim.timer.reset();
        }

        // Update facing direction
        if velocity.x > 0.1 { anim.facing_right = true; }
        if velocity.x < -0.1 { anim.facing_right = false; }
        sprite.flip_x = !anim.facing_right;

        // Advance frame timer
        anim.timer.tick(time.delta());
        if anim.timer.just_finished() {
            let frames = match anim.kind {
                AnimationKind::Idle => &animations.idle,
                AnimationKind::Running => &animations.running,
            };
            anim.frame = (anim.frame + 1) % frames.len();
            sprite.image = frames[anim.frame].clone();
        }
    }
}
```

**Step 2: Update player/mod.rs**

1. Add `pub mod animation;`
2. Import `animation::{AnimationState, AnimationKind, CharacterAnimations}`
3. Update `spawn_player` to accept `Res<CharacterAnimations>`, replace `Sprite::from_color(...)` with:
   ```rust
   Sprite {
       image: animations.idle[0].clone(),
       ..default()
   }
   ```
   And add `AnimationState` component.
4. Register systems in `PlayerPlugin::build`:
   ```rust
   app.add_systems(OnEnter(AppState::InGame),
       (animation::load_character_animations, spawn_player).chain()
   )
   .add_systems(Update,
       (
           movement::player_input,
           movement::apply_gravity,
           collision::collision_system,
           wrap::player_wrap_system,
           animation::animate_player,  // NEW: after movement
       ).chain().run_if(in_state(AppState::InGame)),
   );
   ```

**Step 3: Run tests**
```bash
cargo test
```
Existing tests should pass (they don't touch animation).

**Step 4: Commit**
```
feat: add player sprite with idle/running animation
```

---

### Task 5: Verify everything works

**Step 1: Build**
```bash
cargo build
```

**Step 2: Run tests**
```bash
cargo test
```

**Step 3: Manual test**
```bash
cargo run
```
- Player appears as 44×44 sprite (not colored rectangle)
- Walking left/right cycles running frames
- Idle shows idle frame
- Sprite flips when changing direction
- F3 debug panel still works
- Tiles are 8×8, world looks correct
- Camera is 1:1 scale

---

## Execution order

Tasks 1, 2, 3 are independent and can be done in parallel.
Task 4 depends on Task 3 (needs animation frame paths).
Task 5 depends on all previous tasks.
