# Shared Physics Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace duplicated player/dropped-item physics with a shared `src/physics.rs` module using composable ECS components.

**Architecture:** Single file `src/physics.rs` with components (`Velocity`, `Gravity`, `Grounded`, `TileCollider`, `Friction`, `Bounce`, `BobEffect`), four chained systems (`apply_gravity → tile_collision → apply_friction → apply_bob`), and a `PhysicsPlugin`. Player and dropped items migrate to shared components; old physics code is deleted.

**Tech Stack:** Rust, Bevy ECS, existing `math::Aabb`/`tile_aabb`, `WorldMap::is_solid`

**Design doc:** `docs/plans/2026-03-01-shared-physics-design.md`

---

### Task 1: Create physics.rs with components

**Files:**
- Create: `src/physics.rs`
- Modify: `src/main.rs` (add `pub mod physics;`)

**Step 1: Create physics.rs with all components and empty plugin**

```rust
// src/physics.rs
use bevy::prelude::*;

use crate::math::{tile_aabb, Aabb};
use crate::sets::GameSet;
use crate::world::chunk::WorldMap;
use crate::world::ctx::WorldCtx;

pub const MAX_DELTA_SECS: f32 = 1.0 / 20.0;

const BOUNCE_THRESHOLD: f32 = 5.0;
const BOUNCE_HORIZONTAL_DAMPING: f32 = 0.9;

// ── Components ──────────────────────────────────────────────

#[derive(Component, Default, Debug)]
pub struct Velocity {
    pub x: f32,
    pub y: f32,
}

#[derive(Component, Debug)]
pub struct Gravity(pub f32);

#[derive(Component, Debug)]
pub struct Grounded(pub bool);

#[derive(Component, Debug)]
pub struct TileCollider {
    pub width: f32,
    pub height: f32,
}

#[derive(Component, Debug)]
pub struct Friction(pub f32);

#[derive(Component, Debug)]
pub struct Bounce(pub f32);

#[derive(Component, Debug)]
pub struct BobEffect {
    pub amplitude: f32,
    pub speed: f32,
    pub phase: f32,
    pub rest_y: f32,
}

// ── Plugin ──────────────────────────────────────────────────

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (apply_gravity, tile_collision, apply_friction, apply_bob)
                .chain()
                .in_set(GameSet::Physics),
        );
    }
}

// ── Systems (stubs) ─────────────────────────────────────────

pub fn apply_gravity() {}
pub fn tile_collision() {}
pub fn apply_friction() {}
pub fn apply_bob() {}
```

**Step 2: Add module to main.rs**

Add `pub mod physics;` to `src/main.rs` after `pub mod math;`.
Add `.add_plugins(physics::PhysicsPlugin)` after `.add_plugins(player::PlayerPlugin)`.

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: compiles with no new errors

**Step 4: Commit**

```
feat(physics): add shared physics module with components and plugin stub
```

---

### Task 2: Implement apply_gravity

**Files:**
- Modify: `src/physics.rs`

**Step 1: Write unit test (pure logic)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gravity_reduces_velocity_y() {
        // Pure logic: vel.y should decrease by gravity * dt
        let mut vel_y: f32 = 0.0;
        let gravity = 980.0;
        let dt = 0.016;
        vel_y -= gravity * dt;
        assert!(vel_y < 0.0);
        assert!((vel_y - (-15.68)).abs() < 0.01);
    }
}
```

**Step 2: Write integration test (Bevy system)**

```rust
    #[test]
    fn gravity_system_pulls_entity_down() {
        use crate::test_helpers::fixtures;
        let mut app = fixtures::test_app();
        app.add_systems(Update, apply_gravity);

        app.world_mut().spawn((
            Velocity { x: 0.0, y: 0.0 },
            Gravity(980.0),
        ));

        app.update();
        std::thread::sleep(std::time::Duration::from_millis(50));
        app.update();

        let mut query = app.world_mut().query::<&Velocity>();
        let vel = query.iter(app.world()).next().unwrap();
        assert!(vel.y < 0.0, "gravity should pull down, got {}", vel.y);
    }

    #[test]
    fn gravity_does_not_affect_entity_without_gravity_component() {
        use crate::test_helpers::fixtures;
        let mut app = fixtures::test_app();
        app.add_systems(Update, apply_gravity);

        app.world_mut().spawn(Velocity { x: 10.0, y: 5.0 });

        app.update();
        std::thread::sleep(std::time::Duration::from_millis(50));
        app.update();

        let mut query = app.world_mut().query::<&Velocity>();
        let vel = query.iter(app.world()).next().unwrap();
        assert_eq!(vel.y, 5.0, "no Gravity component = no change");
    }
```

**Step 3: Run tests to verify they fail**

Run: `cargo test physics`
Expected: FAIL (apply_gravity is empty stub)

**Step 4: Implement apply_gravity**

```rust
pub fn apply_gravity(
    time: Res<Time>,
    mut query: Query<(&mut Velocity, &Gravity)>,
) {
    let dt = time.delta_secs().min(MAX_DELTA_SECS);
    for (mut vel, gravity) in &mut query {
        vel.y -= gravity.0 * dt;
    }
}
```

**Step 5: Run tests to verify they pass**

Run: `cargo test physics`
Expected: PASS

**Step 6: Commit**

```
feat(physics): implement apply_gravity system
```

---

### Task 3: Implement tile_collision

**Files:**
- Modify: `src/physics.rs`

This is the most complex system. It replicates the logic from `player/collision.rs` but generalized for any entity with `TileCollider`.

**Key design decision:** BobEffect entities need their visual Y offset removed before physics, and rest_y stored after resolution. This prevents the bob animation from interfering with collision detection.

**Step 1: Write integration tests**

```rust
    #[test]
    fn collision_does_not_crash_on_empty_world() {
        use crate::test_helpers::fixtures;
        let mut app = fixtures::test_app();
        app.add_systems(Update, tile_collision);

        app.world_mut().spawn((
            Transform::from_xyz(500.0, 30000.0, 0.0),
            Velocity { x: 0.0, y: -100.0 },
            Grounded(false),
            TileCollider { width: 24.0, height: 40.0 },
        ));

        app.update();

        let mut query = app.world_mut().query::<&Grounded>();
        let grounded = query.iter(app.world()).next().unwrap();
        assert!(!grounded.0, "should not be grounded in empty world");
    }

    #[test]
    fn collision_grounds_entity_on_solid_surface() {
        use crate::test_helpers::fixtures;
        use crate::world::terrain_gen;

        let mut app = fixtures::test_app();
        app.add_systems(Update, tile_collision);

        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let surface_y = terrain_gen::surface_height(
            &nc, 0, &wc,
            pc.layers.surface.terrain_frequency,
            pc.layers.surface.terrain_amplitude,
        );
        let chunk_size = wc.chunk_size as i32;
        let surface_chunk_y = surface_y.div_euclid(chunk_size);

        let mut world_map = WorldMap::default();
        for cy in (surface_chunk_y - 1)..=(surface_chunk_y + 1) {
            world_map.get_or_generate_chunk(0, cy, &ctx);
        }
        *app.world_mut().resource_mut::<WorldMap>() = world_map;

        let tile_size = wc.tile_size;
        let height = 40.0;
        let spawn_y = (surface_y + 1) as f32 * tile_size + height / 2.0 - 2.0;

        app.world_mut().spawn((
            Transform::from_xyz(tile_size / 2.0, spawn_y, 0.0),
            Velocity { x: 0.0, y: -200.0 },
            Grounded(false),
            TileCollider { width: 24.0, height: 40.0 },
        ));

        app.update();

        let mut query = app.world_mut().query::<&Grounded>();
        let grounded = query.iter(app.world()).next().unwrap();
        assert!(grounded.0, "entity should be grounded on solid surface");
    }

    #[test]
    fn collision_works_without_grounded_component() {
        use crate::test_helpers::fixtures;
        let mut app = fixtures::test_app();
        app.add_systems(Update, tile_collision);

        // Entity with collider but no Grounded (e.g. projectile that still collides)
        app.world_mut().spawn((
            Transform::from_xyz(500.0, 30000.0, 0.0),
            Velocity { x: 0.0, y: -100.0 },
            TileCollider { width: 4.0, height: 4.0 },
        ));

        // Should not panic
        app.update();
    }
```

**Step 2: Run tests to verify they fail**

Run: `cargo test physics`
Expected: FAIL

**Step 3: Implement tile_collision**

```rust
pub fn tile_collision(
    time: Res<Time>,
    ctx: WorldCtx,
    world_map: Res<WorldMap>,
    mut query: Query<(
        &mut Transform,
        &mut Velocity,
        &TileCollider,
        Option<&mut Grounded>,
        Option<&Bounce>,
        Option<&mut BobEffect>,
    )>,
) {
    let dt = time.delta_secs().min(MAX_DELTA_SECS);
    let ts = ctx.config.tile_size;
    let ctx_ref = ctx.as_ref();

    for (mut tf, mut vel, collider, mut grounded, bounce, mut bob) in &mut query {
        let pos = &mut tf.translation;
        let w = collider.width;
        let h = collider.height;

        // Remove bob offset before physics (restore to physics position)
        if let Some(ref bob) = bob {
            if grounded.as_ref().is_some_and(|g| g.0) {
                pos.y = bob.rest_y;
            }
        }

        // ── Resolve X axis ──
        pos.x += vel.x * dt;
        let aabb = Aabb::from_center(pos.x, pos.y, w, h);
        for (tx, ty) in aabb.overlapping_tiles(ts) {
            if world_map.is_solid(tx, ty, &ctx_ref) {
                let tile = tile_aabb(tx, ty, ts);
                let entity_aabb = Aabb::from_center(pos.x, pos.y, w, h);
                if entity_aabb.overlaps(&tile) {
                    if vel.x > 0.0 {
                        pos.x = tile.min_x - w / 2.0;
                    } else if vel.x < 0.0 {
                        pos.x = tile.max_x + w / 2.0;
                    }
                    vel.x = 0.0;
                }
            }
        }

        // ── Resolve Y axis ──
        pos.y += vel.y * dt;
        if let Some(ref mut g) = grounded {
            g.0 = false;
        }
        let aabb = Aabb::from_center(pos.x, pos.y, w, h);
        for (tx, ty) in aabb.overlapping_tiles(ts) {
            if world_map.is_solid(tx, ty, &ctx_ref) {
                let tile = tile_aabb(tx, ty, ts);
                let entity_aabb = Aabb::from_center(pos.x, pos.y, w, h);
                if entity_aabb.overlaps(&tile) {
                    if vel.y < 0.0 {
                        pos.y = tile.max_y + h / 2.0;

                        // Bounce or land
                        let bounce_factor = bounce.map(|b| b.0).unwrap_or(0.0);
                        let bounced = -vel.y * bounce_factor;
                        if bounced > BOUNCE_THRESHOLD {
                            vel.y = bounced;
                            vel.x *= BOUNCE_HORIZONTAL_DAMPING;
                        } else {
                            vel.y = 0.0;
                            if let Some(ref mut g) = grounded {
                                g.0 = true;
                            }
                        }
                    } else if vel.y > 0.0 {
                        pos.y = tile.min_y - h / 2.0;
                        vel.y = 0.0;
                    }
                }
            }
        }

        // Store physics-resolved Y for bob effect
        if let Some(ref mut bob) = bob {
            if grounded.as_ref().is_some_and(|g| g.0) {
                bob.rest_y = pos.y;
            }
        }
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test physics`
Expected: PASS

**Step 5: Commit**

```
feat(physics): implement tile_collision system with bounce support
```

---

### Task 4: Implement apply_friction and apply_bob

**Files:**
- Modify: `src/physics.rs`

**Step 1: Write tests**

```rust
    #[test]
    fn friction_slows_grounded_entity() {
        use crate::test_helpers::fixtures;
        let mut app = fixtures::test_app();
        app.add_systems(Update, apply_friction);

        app.world_mut().spawn((
            Velocity { x: 100.0, y: 0.0 },
            Grounded(true),
            Friction(0.9),
        ));

        app.update();

        let mut query = app.world_mut().query::<&Velocity>();
        let vel = query.iter(app.world()).next().unwrap();
        assert!((vel.x - 90.0).abs() < 0.01, "friction should slow x, got {}", vel.x);
    }

    #[test]
    fn friction_does_not_affect_airborne_entity() {
        use crate::test_helpers::fixtures;
        let mut app = fixtures::test_app();
        app.add_systems(Update, apply_friction);

        app.world_mut().spawn((
            Velocity { x: 100.0, y: -50.0 },
            Grounded(false),
            Friction(0.9),
        ));

        app.update();

        let mut query = app.world_mut().query::<&Velocity>();
        let vel = query.iter(app.world()).next().unwrap();
        assert_eq!(vel.x, 100.0, "airborne = no friction");
    }

    #[test]
    fn bob_oscillates_grounded_entity() {
        use crate::test_helpers::fixtures;
        let mut app = fixtures::test_app();
        app.add_systems(Update, apply_bob);

        let rest = 500.0;
        app.world_mut().spawn((
            Transform::from_xyz(0.0, rest, 0.0),
            Grounded(true),
            BobEffect {
                amplitude: 3.0,
                speed: 2.0,
                phase: 0.0,
                rest_y: rest,
            },
        ));

        app.update();
        std::thread::sleep(std::time::Duration::from_millis(50));
        app.update();

        let mut query = app.world_mut().query::<(&Transform, &BobEffect)>();
        let (tf, bob) = query.iter(app.world()).next().unwrap();
        assert!(bob.phase > 0.0, "phase should advance");
        // Y should differ from rest_y by bob offset
        let expected_y = rest + 3.0 * bob.phase.sin();
        assert!((tf.translation.y - expected_y).abs() < 0.1);
    }

    #[test]
    fn bob_does_not_affect_airborne_entity() {
        use crate::test_helpers::fixtures;
        let mut app = fixtures::test_app();
        app.add_systems(Update, apply_bob);

        app.world_mut().spawn((
            Transform::from_xyz(0.0, 500.0, 0.0),
            Grounded(false),
            BobEffect {
                amplitude: 3.0,
                speed: 2.0,
                phase: 0.0,
                rest_y: 0.0,
            },
        ));

        app.update();
        std::thread::sleep(std::time::Duration::from_millis(50));
        app.update();

        let mut query = app.world_mut().query::<&Transform>();
        let tf = query.iter(app.world()).next().unwrap();
        assert_eq!(tf.translation.y, 500.0, "airborne = no bob");
    }
```

**Step 2: Run tests, verify fail**

Run: `cargo test physics`

**Step 3: Implement**

```rust
pub fn apply_friction(
    mut query: Query<(&mut Velocity, &Grounded, &Friction)>,
) {
    for (mut vel, grounded, friction) in &mut query {
        if grounded.0 {
            vel.x *= friction.0;
        }
    }
}

pub fn apply_bob(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut BobEffect, &Grounded)>,
) {
    let dt = time.delta_secs();
    for (mut tf, mut bob, grounded) in &mut query {
        if grounded.0 {
            bob.phase += bob.speed * dt;
            tf.translation.y = bob.rest_y + bob.amplitude * bob.phase.sin();
        }
    }
}
```

**Step 4: Run tests, verify pass**

Run: `cargo test physics`

**Step 5: Commit**

```
feat(physics): implement apply_friction and apply_bob systems
```

---

### Task 5: Migrate player to shared components

**Files:**
- Modify: `src/player/mod.rs` — remove `Velocity`, `Grounded`, `MAX_DELTA_SECS`; update spawn; update plugin systems
- Modify: `src/player/movement.rs` — change imports, remove `apply_gravity`
- Modify: `src/player/collision.rs` — delete `collision_system`, keep file for tests or remove
- Modify: `src/player/animation.rs` — change imports

**Step 1: Update player/mod.rs**

Remove definitions of `Velocity`, `Grounded`, `MAX_DELTA_SECS`.

Update imports to re-export from physics:
```rust
pub use crate::physics::{Grounded, Velocity};
```

Update `PlayerPlugin::build` — remove `apply_gravity` and `collision_system` from the system chain:
```rust
// Was:
(player_input, apply_gravity, collision_system, wrap, animate).chain().in_set(GameSet::Physics)

// Becomes:
(player_input, wrap, animate).chain().in_set(GameSet::Physics)
```

Update `spawn_player` — add `Gravity`, `TileCollider`:
```rust
use crate::physics::{Gravity, Grounded, TileCollider, Velocity};

commands.spawn((
    Player,
    /* inventory, hotbar... same as before */
    Velocity::default(),
    Gravity(player_config.gravity),
    Grounded(false),
    TileCollider { width: player_config.width, height: player_config.height },
    /* animation, sprite, transform... same as before */
));
```

**Step 2: Update player/movement.rs**

Change import from `crate::player::{Grounded, Player, Velocity, MAX_DELTA_SECS}` to:
```rust
use crate::physics::{Grounded, Velocity};
use crate::player::Player;
```

Delete `apply_gravity` function and its test `gravity_decreases_velocity_y`.

`player_input` stays, only imports change.

**Step 3: Update player/animation.rs**

Change import:
```rust
use crate::physics::{Grounded, Velocity};
use crate::player::Player;
```

**Step 4: Delete player/collision.rs contents**

Remove `collision_system`. The tests for grounded-on-surface behavior are now in `physics.rs`. Delete or empty the file (keep `pub mod collision;` in mod.rs to avoid breakage, or remove both).

**Step 5: Run all tests**

Run: `cargo test`
Expected: all 160 tests pass (some moved to physics, some deleted from player)

**Step 6: Commit**

```
refactor(player): migrate to shared physics components
```

---

### Task 6: Migrate dropped items to shared components

**Files:**
- Modify: `src/item/dropped_item.rs` — remove `velocity` from `DroppedItem`, remove `DroppedItemPhysics`, remove `dropped_item_physics_system` and related functions/constants, update tests
- Modify: `src/item/plugin.rs` — remove `dropped_item_physics_system` registration
- Modify: `src/interaction/block_action.rs` — update `spawn_tile_drops` to use physics components

**Step 1: Update DroppedItem struct**

Remove `velocity: Vec2` field. Remove `DroppedItemPhysics` struct entirely.

```rust
#[derive(Component, Debug)]
pub struct DroppedItem {
    pub item_id: String,
    pub count: u16,
    pub lifetime: Timer,
    pub magnetized: bool,
}
```

**Step 2: Remove old physics code from dropped_item.rs**

Delete:
- `const BOUNCE_HORIZONTAL_DAMPING`
- `const FRICTION_THRESHOLD`
- `pub fn apply_gravity`
- `pub fn apply_friction`
- `pub fn apply_bounce`
- `pub fn dropped_item_physics_system`

Keep:
- `DroppedItem` (modified)
- `PickupConfig`
- `SpawnParams`
- `calculate_drops`

**Step 3: Update tests in dropped_item.rs**

Remove `velocity` from `dropped_item_has_required_fields` test:
```rust
    #[test]
    fn dropped_item_has_required_fields() {
        let item = DroppedItem {
            item_id: "dirt".into(),
            count: 5,
            lifetime: Timer::from_seconds(300.0, TimerMode::Once),
            magnetized: false,
        };
        assert_eq!(item.item_id, "dirt");
        assert_eq!(item.count, 5);
        assert!(!item.magnetized);
    }
```

Delete these tests (logic moved to physics.rs):
- `dropped_item_physics_defaults`
- `physics_system_applies_gravity`
- `physics_system_applies_friction_when_grounded`
- `physics_system_applies_bounce_on_ground_hit`
- `physics_system_no_bounce_when_rising`

Keep:
- `dropped_item_has_required_fields` (updated)
- `spawn_params_calculates_velocity`

**Step 4: Update item/plugin.rs**

Remove: `use super::dropped_item::dropped_item_physics_system;`
Remove: `.add_systems(Update, dropped_item_physics_system);`

The lifetime/despawn system needs to stay. Extract it from the old `dropped_item_physics_system` into a new small system:

```rust
// In dropped_item.rs:
pub fn despawn_expired_drops(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut DroppedItem)>,
) {
    for (entity, mut item) in &mut query {
        item.lifetime.tick(time.delta());
        if item.lifetime.just_finished() {
            commands.entity(entity).despawn();
        }
    }
}
```

Register in plugin.rs:
```rust
.add_systems(Update, dropped_item::despawn_expired_drops);
```

**Step 5: Update block_action.rs spawn**

```rust
use crate::physics::{BobEffect, Bounce, Friction, Gravity, Grounded, TileCollider, Velocity};

fn spawn_tile_drops(/* ... */) {
    let drops = calculate_drops(tile_drops);
    for (item_id, count) in drops {
        let params = SpawnParams::random(tile_center);
        let vel = params.velocity();

        let sprite = /* ... same icon resolution as before ... */;

        commands.spawn((
            DroppedItem {
                item_id,
                count,
                lifetime: Timer::from_seconds(300.0, TimerMode::Once),
                magnetized: false,
            },
            Velocity { x: vel.x, y: vel.y },
            Gravity(400.0),
            Grounded(false),
            TileCollider { width: 4.0, height: 4.0 },
            Friction(0.9),
            Bounce(0.3),
            BobEffect {
                amplitude: 3.0,
                speed: 2.0,
                phase: 0.0,
                rest_y: 0.0,
            },
            sprite,
            Transform::from_translation(tile_center.extend(1.0)),
        ));
    }
}
```

**Step 6: Run all tests**

Run: `cargo test`
Expected: all tests pass

**Step 7: Commit**

```
refactor(items): migrate dropped items to shared physics components
```

---

### Task 7: Cleanup and final verification

**Files:**
- Possibly: `src/player/mod.rs` — remove `pub mod collision;` if file emptied
- Verify: no remaining references to old types

**Step 1: Search for leftover references**

```bash
grep -rn "DroppedItemPhysics\|player::Velocity\|player::Grounded\|BOUNCE_HORIZONTAL_DAMPING\|FRICTION_THRESHOLD" src/
```

Expected: no matches (or only in physics.rs for constants)

**Step 2: Remove empty collision.rs if applicable**

If `player/collision.rs` is empty or only has dead code, delete it and remove `pub mod collision;` from `player/mod.rs`.

**Step 3: Run full test suite**

Run: `cargo test`
Expected: all tests pass

**Step 4: Run build with warnings check**

Run: `cargo build 2>&1 | grep warning`
Expected: no new warnings from physics changes

**Step 5: Commit**

```
chore: remove dead physics code after migration
```

---

## Execution order summary

| Task | Description | Risk | Dependencies |
|------|-------------|------|-------------|
| 1 | Create physics.rs with components + stub plugin | None (additive) | — |
| 2 | Implement apply_gravity | Low | Task 1 |
| 3 | Implement tile_collision | Medium (core logic) | Task 2 |
| 4 | Implement friction + bob | Low | Task 3 |
| 5 | Migrate player | Medium (breaking change) | Task 4 |
| 6 | Migrate dropped items | Medium (breaking change) | Task 5 |
| 7 | Cleanup | Low | Task 6 |

Tasks 1-4 are additive (nothing breaks). Tasks 5-6 are swaps (old code removed, new code takes over). Task 7 is cleanup.
