# Shared Physics System

**Date:** 2026-03-01
**Status:** Approved

## Goal

Replace duplicated physics code (player and dropped items each have their own gravity/collision) with a single shared system in `src/physics.rs`. Support player, dropped items, NPCs, and projectiles via composable ECS components.

## Decisions

- **Collision scope:** Tile collision only. Entity-to-entity collision handled separately (hitboxes, overlap checks, game logic).
- **Component granularity:** Fine-grained. Each physics behavior is a separate component. Entities opt in by having the component.
- **Structure:** Single file `src/physics.rs` (matches `math.rs`, `sets.rs` pattern).
- **Player migration:** Immediate. Player moves to shared components in the same PR.

## Components

```rust
#[derive(Component, Default)]
pub struct Velocity { pub x: f32, pub y: f32 }

#[derive(Component)]
pub struct Gravity(pub f32);

#[derive(Component)]
pub struct Grounded(pub bool);

#[derive(Component)]
pub struct TileCollider { pub width: f32, pub height: f32 }

#[derive(Component)]
pub struct Friction(pub f32);

#[derive(Component)]
pub struct Bounce(pub f32);

#[derive(Component)]
pub struct BobEffect {
    pub amplitude: f32,
    pub speed: f32,
    pub phase: f32,
    pub rest_y: f32,
}
```

### Entity composition

| Entity       | Velocity | Gravity | Grounded | TileCollider | Friction | Bounce | BobEffect |
|--------------|----------|---------|----------|--------------|----------|--------|-----------|
| Player       | yes      | yes     | yes      | yes (24x48)  | no       | no     | no        |
| Dropped item | yes      | yes(400)| yes      | yes (4x4)    | yes(0.9) | yes(0.3)| yes      |
| NPC (future) | yes      | yes     | yes      | yes          | no       | no     | no        |
| Projectile   | yes      | yes(low)| no       | no           | no       | no     | no        |

## Systems

Execution order (chained in `GameSet::Physics`):

1. **`apply_gravity`** — `vel.y -= gravity * dt` for non-grounded entities with `Velocity` + `Gravity`
2. **`tile_collision`** — move by velocity, resolve X then Y axis against solid tiles using `WorldMap::is_solid` + `Aabb`. Set `Grounded` on landing. Apply `Bounce` on ground hit if present.
3. **`apply_friction`** — `vel.x *= friction` for grounded entities with `Friction`
4. **`apply_bob`** — sine-wave Y offset for grounded entities with `BobEffect`

## Plugin

```rust
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update,
            (apply_gravity, tile_collision, apply_friction, apply_bob)
                .chain()
                .in_set(GameSet::Physics),
        );
    }
}
```

`player_input` stays in `GameSet::Input` (runs before Physics). `wrap`/`animate` run after physics systems within `GameSet::Physics`.

## Deletions

| File | Removed |
|------|---------|
| `player/mod.rs` | `Velocity`, `Grounded`, `MAX_DELTA_SECS` |
| `player/movement.rs` | `apply_gravity` fn + test |
| `player/collision.rs` | `collision_system` fn (logic moves to `physics::tile_collision`) |
| `item/dropped_item.rs` | `DroppedItemPhysics`, `apply_gravity`, `apply_friction`, `apply_bounce`, `dropped_item_physics_system`, constants |
| `item/plugin.rs` | `.add_systems(Update, dropped_item_physics_system)` |

## Import migrations

`player::Velocity`/`player::Grounded` → `physics::Velocity`/`physics::Grounded` in:
- `player/movement.rs` (player_input)
- `player/animation.rs` (animate_player)

## Test migrations

- `gravity_decreases_velocity_y` → `physics.rs` (generic, no `With<Player>`)
- `collision_no_crash_on_empty_world` → `physics.rs`
- `collision_grounds_player_on_solid_surface` → `physics.rs`
- `apply_bounce`/`apply_friction` unit tests → `physics.rs`
- `dropped_item_has_required_fields` → stays, loses `velocity` field
- `dropped_item_physics_defaults` → deleted

## Not changed

- `player_input`, `player_wrap_system`, `animate_player` — logic unchanged, only imports
- `DroppedItem` struct — stays, loses `velocity` field
- `calculate_drops`, `SpawnParams`, `PickupConfig` — untouched
- `math.rs` (`Aabb`, `tile_aabb`) — used by physics, unchanged
