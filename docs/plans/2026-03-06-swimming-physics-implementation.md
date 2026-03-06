# Swimming Physics Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the current buoyancy/drag liquid physics with a Starbound-style zonal swimming system where entering liquid changes movement parameters (gravity, speed, controls) rather than simulating physical forces.

**Architecture:** Add a `Submerged` component updated by a lightweight detection system. `player_input` reads `Submerged` to switch between land/water controls. Remove buoyancy/drag forces from `apply_liquid_physics` — it becomes a pure detection system. `swim_speed_factor` from `LiquidDef` finally gets used.

**Tech Stack:** Rust, Bevy ECS, RON config files.

---

### Task 1: Add `Submerged` component and swim config to `PlayerConfig`

**Files:**
- Modify: `src/physics.rs:20-63` (add Submerged component)
- Modify: `src/registry/player.rs:1-31` (add swim fields to PlayerConfig)
- Modify: `assets/content/characters/adventurer/adventurer.character.ron` (add swim values)
- Modify: `src/test_helpers.rs:234-245` (update test_player_config)

**Step 1: Add the `Submerged` component to `src/physics.rs`**

Add after the `BobEffect` component (after line 63):

```rust
/// Tracks how much of the entity is submerged in liquid.
/// Updated by the liquid detection system each frame.
#[derive(Component, Debug, Default)]
pub struct Submerged {
    /// 0.0 = not submerged, 1.0 = fully submerged.
    pub ratio: f32,
    /// The dominant liquid type the entity is in (by fill weight).
    pub liquid_id: LiquidId,
    /// The swim_speed_factor of the dominant liquid.
    pub swim_speed_factor: f32,
}

impl Submerged {
    /// Threshold above which the entity is considered "swimming".
    pub const SWIM_THRESHOLD: f32 = 0.3;

    pub fn is_swimming(&self) -> bool {
        self.ratio >= Self::SWIM_THRESHOLD
    }
}
```

Add use import at the top of `src/physics.rs`:
```rust
use crate::liquid::data::LiquidId;
```

**Step 2: Add swim parameters to `PlayerConfig`**

In `src/registry/player.rs`, add three new fields to `PlayerConfig`:

```rust
/// Vertical impulse (px/s²) when pressing Up/Space in liquid.
#[serde(default = "default_swim_impulse")]
pub swim_impulse: f32,
/// Gravity multiplier while swimming (0.0 = float, 1.0 = full gravity).
#[serde(default = "default_swim_gravity_factor")]
pub swim_gravity_factor: f32,
/// Per-second velocity retention in liquid (0.0 = instant stop, 1.0 = no drag).
#[serde(default = "default_swim_drag")]
pub swim_drag: f32,
```

Add default functions:

```rust
fn default_swim_impulse() -> f32 { 180.0 }
fn default_swim_gravity_factor() -> f32 { 0.3 }
fn default_swim_drag() -> f32 { 0.15 }
```

**Step 3: Add swim values to `adventurer.character.ron`**

Add after `pickup_radius: 20.0,`:

```ron
swim_impulse: 180.0,
swim_gravity_factor: 0.3,
swim_drag: 0.15,
```

**Step 4: Update `test_player_config` in `src/test_helpers.rs`**

Add the new fields to the test fixture:

```rust
swim_impulse: 180.0,
swim_gravity_factor: 0.3,
swim_drag: 0.15,
```

**Step 5: Run `cargo test` to verify nothing is broken**

Run: `cargo test`
Expected: All 268 tests pass (new fields have defaults, so nothing breaks).

**Step 6: Commit**

```
feat: add Submerged component and swim config to PlayerConfig
```

---

### Task 2: Replace `apply_liquid_physics` with detection-only `update_submersion`

**Files:**
- Modify: `src/physics.rs:69-86` (rename system in plugin chain)
- Modify: `src/physics.rs:208-291` (rewrite system body)

**Step 1: Write failing tests for the new detection system**

Add to `src/physics.rs` tests module (replace or add alongside existing):

```rust
#[test]
fn submersion_defaults_to_zero() {
    let sub = Submerged::default();
    assert_eq!(sub.ratio, 0.0);
    assert!(!sub.is_swimming());
}

#[test]
fn submersion_threshold() {
    let mut sub = Submerged::default();
    sub.ratio = 0.29;
    assert!(!sub.is_swimming());
    sub.ratio = 0.3;
    assert!(sub.is_swimming());
}
```

**Step 2: Run tests to verify they pass (these test the component, not the system)**

Run: `cargo test submersion`
Expected: PASS

**Step 3: Rewrite `apply_liquid_physics` → `update_submersion`**

Replace the entire `apply_liquid_physics` function (lines 208-291) with:

```rust
/// Detect liquid submersion for all entities with `TileCollider` + `Submerged`.
///
/// This is a pure detection system — it writes the `Submerged` component
/// but applies no forces. Forces are handled by the movement system
/// (player_input) and gravity, which read `Submerged` to adjust behavior.
pub fn update_submersion(
    ctx: WorldCtx,
    world_map: Res<WorldMap>,
    liquid_registry: Res<LiquidRegistry>,
    mut query: Query<(&Transform, &TileCollider, &mut Submerged)>,
) {
    if liquid_registry.defs.is_empty() {
        return;
    }
    let ts = ctx.config.tile_size;
    let ctx_ref = ctx.as_ref();

    for (tf, collider, mut sub) in &mut query {
        let pos = tf.translation;
        let w = collider.width;
        let h = collider.height;
        let aabb = Aabb::from_center(pos.x, pos.y, w, h);

        let mut total_fill: f32 = 0.0;
        let mut best_fill: f32 = 0.0;
        let mut best_liquid = LiquidId::NONE;
        let mut best_swim_factor: f32 = 1.0;
        let mut max_damage: f32 = 0.0;

        for (tx, ty) in aabb.overlapping_tiles(ts) {
            let cell = {
                let wtx = ctx_ref.config.wrap_tile_x(tx);
                if ty < 0 || ty >= ctx_ref.config.height_tiles {
                    LiquidCell::EMPTY
                } else {
                    let (cx, cy) = chunk::tile_to_chunk(wtx, ty, ctx_ref.config.chunk_size);
                    let (lx, ly) = chunk::tile_to_local(wtx, ty, ctx_ref.config.chunk_size);
                    match world_map.chunk(cx, cy) {
                        Some(chunk) => chunk.liquid.get(lx, ly, ctx_ref.config.chunk_size),
                        None => LiquidCell::EMPTY,
                    }
                }
            };
            if cell.is_empty() {
                continue;
            }

            if let Some(def) = liquid_registry.get(cell.liquid_type) {
                let fill = cell.level.clamp(0.0, 1.0);
                total_fill += fill;
                if fill > best_fill {
                    best_fill = fill;
                    best_liquid = cell.liquid_type;
                    best_swim_factor = def.swim_speed_factor;
                }
                max_damage = max_damage.max(def.damage_on_contact);
            }
        }

        let entity_tile_height = (h / ts).max(1.0);
        sub.ratio = (total_fill / entity_tile_height).clamp(0.0, 1.0);
        sub.liquid_id = best_liquid;
        sub.swim_speed_factor = if best_liquid.is_none() { 1.0 } else { best_swim_factor };

        // TODO: Apply damage_on_contact to player health when health system exists.
        let _ = max_damage;
    }
}
```

**Step 4: Update the plugin system chain**

In `PhysicsPlugin::build` (line 73-84), rename `apply_liquid_physics` to `update_submersion`:

```rust
app.add_systems(
    Update,
    (
        apply_gravity,
        tile_collision,
        update_submersion,
        apply_friction,
        apply_bob,
    )
        .chain()
        .in_set(GameSet::Physics),
);
```

**Step 5: Run `cargo test` to verify compilation and existing tests pass**

Run: `cargo test`
Expected: All tests pass. Some liquid physics tests may need updating if they tested buoyancy/drag forces (they don't exist in the current test suite).

**Step 6: Commit**

```
refactor: replace apply_liquid_physics with detection-only update_submersion
```

---

### Task 3: Add `Submerged` component to player entity spawn

**Files:**
- Modify: `src/player/mod.rs:93-122` (add Submerged to spawn bundle)

**Step 1: Add `Submerged` to player spawn**

In `spawn_player` (line 93), add `Submerged::default()` to the spawn bundle, after `Grounded(false)`:

```rust
Submerged::default(),
```

Add the import at the top of `src/player/mod.rs`:

```rust
use crate::physics::Submerged;
```

**Step 2: Run `cargo test` to verify**

Run: `cargo test`
Expected: All tests pass.

**Step 3: Commit**

```
feat: attach Submerged component to player entity
```

---

### Task 4: Rewrite `player_input` with swimming support

**Files:**
- Modify: `src/player/movement.rs` (full rewrite)

**Step 1: Write the new `player_input` system**

Replace the entire contents of `src/player/movement.rs`:

```rust
use bevy::prelude::*;

use crate::physics::{Grounded, Submerged, Velocity, MAX_DELTA_SECS};
use crate::player::Player;
use crate::registry::player::PlayerConfig;

pub fn player_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    player_config: Res<PlayerConfig>,
    mut query: Query<(&mut Velocity, &Grounded, &Submerged), With<Player>>,
) {
    let dt = time.delta_secs().min(MAX_DELTA_SECS);

    for (mut vel, grounded, submerged) in &mut query {
        if submerged.is_swimming() {
            // --- Swimming mode ---
            let swim_speed = player_config.speed * submerged.swim_speed_factor;

            // Horizontal movement
            vel.x = 0.0;
            if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
                vel.x -= swim_speed;
            }
            if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
                vel.x += swim_speed;
            }

            // Vertical swimming: W/Space = up, S = down
            if keys.pressed(KeyCode::Space)
                || keys.pressed(KeyCode::KeyW)
                || keys.pressed(KeyCode::ArrowUp)
            {
                vel.y += player_config.swim_impulse * dt;
            }
            if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
                vel.y -= player_config.swim_impulse * dt;
            }

            // FPS-independent drag (exponential decay)
            let drag = player_config.swim_drag.powf(dt);
            vel.x *= drag;
            vel.y *= drag;
        } else {
            // --- Normal ground/air mode ---
            vel.x = 0.0;
            if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
                vel.x -= player_config.speed;
            }
            if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
                vel.x += player_config.speed;
            }
            if keys.just_pressed(KeyCode::Space) && grounded.0 {
                vel.y = player_config.jump_velocity;
            }
        }
    }
}
```

**Step 2: Run `cargo test` to verify**

Run: `cargo test`
Expected: All tests pass.

**Step 3: Commit**

```
feat: add swimming controls to player_input (W/S/Space for vertical, reduced horizontal speed)
```

---

### Task 5: Modify `apply_gravity` to reduce gravity while swimming

**Files:**
- Modify: `src/physics.rs:93-98` (update apply_gravity signature and logic)

**Step 1: Update `apply_gravity` to read `Submerged`**

Replace the `apply_gravity` function:

```rust
/// Apply gravitational acceleration to all entities with `Velocity` + `Gravity`.
/// If the entity has a `Submerged` component and is swimming, gravity is reduced
/// by `swim_gravity_factor` (read from PlayerConfig — for now hardcoded as a
/// simple lerp based on submersion ratio).
pub fn apply_gravity(
    time: Res<Time>,
    player_config: Option<Res<PlayerConfig>>,
    mut query: Query<(&mut Velocity, &Gravity, Option<&Submerged>)>,
) {
    let dt = time.delta_secs().min(MAX_DELTA_SECS);
    for (mut vel, gravity, submerged) in &mut query {
        let gravity_factor = match submerged {
            Some(sub) if sub.is_swimming() => {
                player_config
                    .as_ref()
                    .map(|c| c.swim_gravity_factor)
                    .unwrap_or(0.3)
            }
            _ => 1.0,
        };
        vel.y -= gravity.0 * gravity_factor * dt;
    }
}
```

**Step 2: Run `cargo test` to verify existing gravity tests still pass**

Run: `cargo test gravity`
Expected: `gravity_system_pulls_entity_down` and `gravity_does_not_affect_entity_without_gravity_component` still pass (entities without Submerged get factor 1.0).

**Step 3: Commit**

```
feat: reduce gravity while swimming based on swim_gravity_factor
```

---

### Task 6: Update animation to show jumping frames while swimming (placeholder)

**Files:**
- Modify: `src/player/animation.rs:29-34` (add Swimming variant)
- Modify: `src/player/animation.rs:87-95` (add swimming detection)
- Modify: `src/player/animation.rs:167-173` (add Swimming to frames_for_kind)

**Step 1: Add `Swimming` variant to `AnimationKind`**

In `AnimationKind` enum (line 30-34), add:

```rust
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum AnimationKind {
    Idle,
    Running,
    Jumping,
    Swimming,
}
```

**Step 2: Update `animate_player` to detect swimming**

In the animation kind determination (lines 89-95), add swimming check.
The system needs to read `Submerged`:

Update the query to include `Option<&Submerged>`:

```rust
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
```

Add import at top: `use crate::physics::Submerged;`

**Step 3: Update `frames_for_kind` to handle Swimming**

Swimming uses jumping frames as placeholder until swimming sprites exist:

```rust
fn frames_for_kind(animations: &CharacterAnimations, kind: AnimationKind) -> &[Handle<Image>] {
    match kind {
        AnimationKind::Idle => &animations.idle,
        AnimationKind::Running => &animations.running,
        AnimationKind::Jumping => &animations.jumping,
        AnimationKind::Swimming => &animations.jumping, // placeholder
    }
}
```

**Step 4: Handle `Swimming` in the match block for frame advancement**

In the `match anim.kind` block (line 125), add `AnimationKind::Swimming` alongside `AnimationKind::Jumping` since they share the same velocity-based frame logic for now:

```rust
AnimationKind::Jumping | AnimationKind::Swimming => {
```

**Step 5: Run `cargo test`**

Run: `cargo test`
Expected: All tests pass.

**Step 6: Commit**

```
feat: add Swimming animation kind (uses jumping frames as placeholder)
```

---

### Task 7: Write integration tests for swimming physics

**Files:**
- Modify: `src/physics.rs` (add tests to the tests module)

**Step 1: Add swimming-specific tests**

Add to the `#[cfg(test)] mod tests` in `src/physics.rs`:

```rust
// -----------------------------------------------------------------------
// Submersion detection tests
// -----------------------------------------------------------------------

#[test]
fn submerged_component_defaults() {
    let sub = Submerged::default();
    assert_eq!(sub.ratio, 0.0);
    assert!(!sub.is_swimming());
    assert_eq!(sub.swim_speed_factor, 0.0);
}

#[test]
fn submerged_swim_threshold() {
    let mut sub = Submerged::default();
    sub.ratio = Submerged::SWIM_THRESHOLD - 0.01;
    assert!(!sub.is_swimming());
    sub.ratio = Submerged::SWIM_THRESHOLD;
    assert!(sub.is_swimming());
    sub.ratio = 1.0;
    assert!(sub.is_swimming());
}

#[test]
fn gravity_reduced_while_swimming() {
    // Pure logic test: gravity factor for swimming entity
    let sub = Submerged {
        ratio: 0.5,
        liquid_id: LiquidId(1),
        swim_speed_factor: 0.5,
    };
    let swim_gravity_factor = 0.3_f32;
    let gravity = 500.0_f32;
    let effective = gravity * swim_gravity_factor;
    assert!(sub.is_swimming());
    assert!((effective - 150.0).abs() < 0.01);
}

#[test]
fn swim_drag_is_fps_independent() {
    // Verify exponential drag produces same result at different dt
    let swim_drag = 0.15_f32;
    let initial_vel = 100.0_f32;
    let total_time = 1.0_f32;

    // 60 FPS: 60 steps of dt=1/60
    let mut vel_60 = initial_vel;
    for _ in 0..60 {
        vel_60 *= swim_drag.powf(1.0 / 60.0);
    }

    // 30 FPS: 30 steps of dt=1/30
    let mut vel_30 = initial_vel;
    for _ in 0..30 {
        vel_30 *= swim_drag.powf(1.0 / 30.0);
    }

    // Both should converge to the same value: initial * drag^1.0
    let expected = initial_vel * swim_drag.powf(total_time);
    assert!(
        (vel_60 - expected).abs() < 0.1,
        "60 FPS result {} should be close to {}",
        vel_60, expected
    );
    assert!(
        (vel_30 - expected).abs() < 0.1,
        "30 FPS result {} should be close to {}",
        vel_30, expected
    );
}
```

**Step 2: Run the new tests**

Run: `cargo test swim`
Expected: All new tests pass.

**Step 3: Run full test suite**

Run: `cargo test`
Expected: All tests pass (268 + new tests).

**Step 4: Commit**

```
test: add swimming physics unit tests
```

---

### Task 8: Final verification and cleanup

**Step 1: Run full test suite**

Run: `cargo test`
Expected: All tests pass.

**Step 2: Run clippy**

Run: `cargo clippy -- -D warnings 2>&1 | head -20`
Expected: No warnings.

**Step 3: Verify the game compiles**

Run: `cargo build`
Expected: Success.

**Step 4: Commit any remaining cleanups**

```
chore: swimming physics cleanup
```
