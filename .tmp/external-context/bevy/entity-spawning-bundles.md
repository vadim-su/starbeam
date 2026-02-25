---
source: Context7 API + docs.rs/bevy/0.18.0
library: Bevy
package: bevy
version: "0.18.0"
topic: Entity spawning, Commands, Bundles, Components
fetched: 2025-02-25T00:00:00Z
official_docs: https://docs.rs/bevy/0.18.0/bevy/ecs/prelude/struct.Commands.html
---

# Entity Spawning & Bundles (Bevy 0.18)

## Defining Components

```rust
#[derive(Component)]
struct Position { x: f32, y: f32 }

#[derive(Component)]
struct Velocity { x: f32, y: f32 }

#[derive(Component)]
struct Player;

#[derive(Component)]
struct Health(f32);
```

## Defining Bundles

```rust
#[derive(Bundle)]
struct PhysicsBundle {
    position: Position,
    velocity: Velocity,
}
```

## Spawning Entities with Commands

```rust
fn example_system(mut commands: Commands) {
    // Spawn with a single component
    commands.spawn(Position { x: 0.0, y: 0.0 });

    // Spawn with a tuple of components ("tuple bundle")
    commands.spawn((
        Position { x: 0.0, y: 0.0 },
        Velocity { x: 1.0, y: 1.0 },
    ));

    // Spawn with a named Bundle
    commands.spawn(PhysicsBundle {
        position: Position { x: 2.0, y: 2.0 },
        velocity: Velocity { x: 0.0, y: 4.0 },
    });

    // Mix bundles and components in a tuple
    commands.spawn((
        PhysicsBundle {
            position: Position { x: 2.0, y: 2.0 },
            velocity: Velocity { x: 0.0, y: 4.0 },
        },
        Player,
        Health(100.0),
    ));
}
```

## Getting Entity ID After Spawn

```rust
fn setup(mut commands: Commands) {
    let entity = commands.spawn((
        Position { x: 0.0, y: 0.0 },
        Player,
    )).id();
    // entity is of type Entity
}
```

## Spawning 2D Entities (Camera + Sprite)

```rust
fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    // Spawn 2D camera
    commands.spawn(Camera2d);

    // Spawn sprite from image
    commands.spawn((
        Sprite::from_image(asset_server.load("branding/icon.png")),
        Transform::from_xyz(0., 0., 0.),
    ));

    // Spawn colored rectangle (no image needed)
    commands.spawn((
        Sprite::from_color(Color::srgb(0.2, 0.8, 0.3), Vec2::new(64.0, 64.0)),
        Transform::from_xyz(100.0, 50.0, 0.0),
    ));
}
```

## Required Components (Auto-inserted)

When you spawn a `Sprite`, Bevy 0.18 automatically inserts:
- `Transform`
- `Visibility`
- `VisibilityClass`
- `Anchor`

When you spawn a `Transform`, Bevy automatically inserts:
- `GlobalTransform`
- `TransformTreeChanged`

## Despawning Entities

```rust
fn cleanup(mut commands: Commands, query: Query<Entity, With<Enemy>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}
```

## World::spawn (Direct World Access)

```rust
let mut world = World::new();

// Same API as Commands::spawn
world.spawn(Position { x: 0.0, y: 0.0 });

world.spawn((
    Position { x: 0.0, y: 0.0 },
    Velocity { x: 1.0, y: 1.0 },
));

let entity = world.spawn((
    PhysicsBundle { position: Position { x: 2.0, y: 2.0 }, velocity: Velocity { x: 0.0, y: 4.0 } },
    Name("Elaina Proctor"),
)).id();
```

## NOTE: SpriteBundle is GONE in 0.18

In Bevy 0.18, `SpriteBundle` no longer exists. Instead, spawn `Sprite` directly
as a component. Required components (Transform, Visibility, etc.) are auto-inserted.

```rust
// OLD (pre-0.15):
// commands.spawn(SpriteBundle { sprite: Sprite { ... }, transform: ..., ..default() });

// NEW (0.18):
commands.spawn((
    Sprite::from_color(Color::WHITE, Vec2::new(32.0, 32.0)),
    Transform::from_xyz(0.0, 0.0, 0.0),
));
```
