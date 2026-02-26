---
source: Context7 API
library: Bevy
package: bevy
topic: Child entity spawning APIs (with_children, with_child, children! macro, ChildOf)
fetched: 2026-02-26T12:00:00Z
official_docs: https://docs.rs/bevy/latest/bevy/ecs/prelude/struct.EntityCommands.html
---

# Child Entity Spawning in Bevy 0.18

## Three Main Approaches

### 1. `children!` macro (preferred for inline spawning)

The `children!` macro spawns children inline as part of the parent's bundle tuple.
Each argument is a bundle. Nesting is supported.

```rust
commands.spawn((
    Mesh3d(cube_handle.clone()),
    MeshMaterial3d(material.clone()),
    Transform::from_xyz(0.0, 0.0, 1.0),
    Rotator,
    children![(
        // child entity
        Mesh3d(cube_handle),
        MeshMaterial3d(material),
        Transform::from_xyz(0.0, 0.0, 3.0),
    )],
));
```

Nested children:
```rust
world.spawn((
    Name::new("Root"),
    children![
        Name::new("Child1"),
        (
            Name::new("Child2"),
            children![Name::new("Grandchild")]
        )
    ]
));
```

### 2. `.with_children()` (closure-based, for multiple children)

Takes a closure with `RelatedSpawnerCommands<'_, ChildOf>`. Good when you need
entity IDs back or complex spawning logic.

```rust
pub fn with_children(
    &mut self,
    func: impl FnOnce(&mut RelatedSpawnerCommands<'_, ChildOf>),
) -> &mut EntityCommands<'a>
```

Usage:
```rust
commands.entity(parent).with_children(|parent| {
    parent.spawn(SomeBundle { .. });
    parent.spawn(AnotherBundle { .. });
});

// Also works on spawn:
commands.spawn_empty().with_children(|p| {
    child1 = p.spawn_empty().id();
    child2 = p.spawn_empty().id();
});
```

### 3. `.with_child()` (single child convenience)

Spawns a single child from a bundle. For multiple children, use `with_children`.

```rust
pub fn with_child(&mut self, bundle: impl Bundle) -> &mut EntityCommands<'a>
```

Usage:
```rust
commands.entity(parent).with_child((
    Component1,
    Component2,
    Transform::default(),
));
```

### 4. Direct `ChildOf` insertion

The most explicit approach — insert `ChildOf(parent_entity)` on the child:

```rust
let root = world.spawn_empty().id();
let child1 = world.spawn(ChildOf(root)).id();
let child2 = world.spawn(ChildOf(root)).id();
```

## Key Notes

- `ChildOf` is the **source of truth** relationship component
- `Children` is the auto-populated `RelationshipTarget` — do NOT manually modify it
- When a parent is despawned, all children (and descendants) are also despawned
- The closure parameter in `with_children` is `RelatedSpawnerCommands<'_, ChildOf>` (not `ChildBuilder` as in older Bevy versions)
