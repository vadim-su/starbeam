---
source: Context7 API
library: Bevy
package: bevy
topic: Iterating children entities (Children component, Query, indexing)
fetched: 2026-02-26T12:00:00Z
official_docs: https://docs.rs/bevy/latest/bevy/ecs/prelude/struct.Children.html
---

# Iterating Children in Bevy 0.18

## Children Component

`Children` is a `RelationshipTarget` that stores entities targeting the current entity
with the `ChildOf` relationship. It implements `IntoIterator`.

```rust
pub struct Children(/* private fields */);
```

## Querying Children

```rust
fn my_system(
    children_query: Query<&Children>,
) {
    // ...
}
```

## Iteration Patterns

### Iterate with for loop (IntoIterator)
```rust
for entity in children.iter() {
    // entity: &Entity
}
```

### Indexing (direct access)
```rust
let first_child = children[0];
let second_child = children[1];
```

Real example from Bevy source:
```rust
fn joint_animation(
    time: Res<Time>,
    children: Query<&ChildOf, With<SkinnedMesh>>,
    parents: Query<&Children>,
    mut transform_query: Query<&mut Transform>,
) {
    for child_of in &children {
        let mesh_node_entity = child_of.parent();
        let mesh_node_parent = parents.get(mesh_node_entity).unwrap();

        // Indexing into Children:
        let first_joint_entity = mesh_node_parent[1];
        let first_joint_children = parents.get(first_joint_entity).unwrap();
        let second_joint_entity = first_joint_children[0];

        let mut transform = transform_query.get_mut(second_joint_entity).unwrap();
        transform.rotation = Quat::from_rotation_z(FRAC_PI_2 * ops::sin(time.elapsed_secs()));
    }
}
```

### Iterate all descendants (recursive)
```rust
fn move_scene_entities(
    time: Res<Time>,
    moved_scene: Query<Entity, With<MovedScene>>,
    children: Query<&Children>,
    mut transforms: Query<&mut Transform>,
) {
    for moved_scene_entity in &moved_scene {
        for entity in children.iter_descendants(moved_scene_entity) {
            if let Ok(mut transform) = transforms.get_mut(entity) {
                transform.translation = Vec3::new(
                    offset * ops::sin(time.elapsed_secs()) / 20.,
                    0.,
                    ops::cos(time.elapsed_secs()) / 20.,
                );
            }
        }
    }
}
```

## Deref to slice

`Children` derefs to `&[Entity]`, so you can use:
- `children.len()`
- `children[index]`
- `children.iter()`
- Slice patterns and methods

Confirmed by assertion pattern in docs:
```rust
assert_eq!(&**world.entity(root).get::<Children>().unwrap(), &[child1, child2]);
```

## Key Notes

- `Query<&Children>` is correct for querying children
- Do NOT manually modify `Children` — modify `ChildOf` on child entities instead
- `children.iter_descendants(entity)` traverses the full hierarchy recursively
- `ChildOf.parent()` returns the parent entity from a child's perspective
