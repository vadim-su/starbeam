---
source: Context7 API + docs.rs/bevy/0.18.0
library: Bevy
package: bevy
version: "0.18.0"
topic: Camera2d, Transform, GlobalTransform
fetched: 2025-02-25T00:00:00Z
official_docs: https://docs.rs/bevy/0.18.0/bevy/prelude/struct.Transform.html
---

# Camera2d & Transforms (Bevy 0.18)

## Camera2d

```rust
pub struct Camera2d;  // Unit struct — just a marker component
```

Spawning a 2D camera:

```rust
fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}
```

With custom position:

```rust
fn setup(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Transform::from_xyz(100.0, 200.0, 0.0),
    ));
}
```

With additional camera configuration:

```rust
commands.spawn((
    Camera2d,
    Camera {
        clear_color: ClearColorConfig::Custom(Color::BLACK),
        ..default()
    },
));
```

### Camera2d Required Components (auto-inserted)

When you spawn `Camera2d`, Bevy automatically adds:
- `Camera` (the actual camera config)
- `OrthographicProjection` (2D projection)
- `Transform` + `GlobalTransform`
- `Visibility` components

## Transform

```rust
pub struct Transform {
    pub translation: Vec3,  // Position. In 2D, z is used for z-ordering (higher = in front)
    pub rotation: Quat,     // Rotation
    pub scale: Vec3,        // Scale
}
```

### Required Components (auto-inserted with Transform)

- `GlobalTransform`
- `TransformTreeChanged`

### Constants

```rust
Transform::IDENTITY  // translation=0, rotation=identity, scale=1
```

### Constructors

```rust
// From position
pub const fn from_xyz(x: f32, y: f32, z: f32) -> Transform
pub const fn from_translation(translation: Vec3) -> Transform

// From rotation only (translation=0, scale=1)
pub const fn from_rotation(rotation: Quat) -> Transform

// From scale only (translation=0, rotation=identity)
pub const fn from_scale(scale: Vec3) -> Transform

// From matrix
pub fn from_matrix(world_from_local: Mat4) -> Transform

// From isometry
pub fn from_isometry(iso: Isometry3d) -> Transform
```

### Builder Methods (return Self)

```rust
pub const fn with_translation(self, translation: Vec3) -> Transform
pub const fn with_rotation(self, rotation: Quat) -> Transform
pub const fn with_scale(self, scale: Vec3) -> Transform
```

### Chaining Example

```rust
Transform::from_xyz(-2.0, 2.5, 5.0).looking_at(Vec3::ZERO, Vec3::Y)
Transform::from_xyz(0.0, 0.5, 0.0).with_scale(Vec3::splat(2.0))
Transform::default().with_scale(Vec3::splat(128.))
```

### Mutation Methods

```rust
// Rotation
pub fn rotate(&mut self, rotation: Quat)
pub fn rotate_axis(&mut self, axis: Dir3, angle: f32)
pub fn rotate_x(&mut self, angle: f32)
pub fn rotate_y(&mut self, angle: f32)
pub fn rotate_z(&mut self, angle: f32)
pub fn rotate_local(&mut self, rotation: Quat)
pub fn rotate_local_x(&mut self, angle: f32)
pub fn rotate_local_y(&mut self, angle: f32)
pub fn rotate_local_z(&mut self, angle: f32)
pub fn rotate_around(&mut self, point: Vec3, rotation: Quat)

// Look-at
pub fn look_at(&mut self, target: Vec3, up: impl TryInto<Dir3>)
pub fn look_to(&mut self, direction: impl TryInto<Dir3>, up: impl TryInto<Dir3>)
pub fn looking_at(self, target: Vec3, up: impl TryInto<Dir3>) -> Transform  // builder
pub fn looking_to(self, direction: impl TryInto<Dir3>, up: impl TryInto<Dir3>) -> Transform

// Direction helpers
pub fn local_x(&self) -> Dir3   // right
pub fn local_y(&self) -> Dir3   // up
pub fn local_z(&self) -> Dir3   // backward
pub fn forward(&self) -> Dir3   // -local_z
pub fn back(&self) -> Dir3      // local_z
pub fn up(&self) -> Dir3        // local_y
pub fn down(&self) -> Dir3      // -local_y
pub fn left(&self) -> Dir3      // -local_x
pub fn right(&self) -> Dir3     // local_x

// Transform a point
pub fn transform_point(&self, point: Vec3) -> Vec3

// Utility
pub fn is_finite(&self) -> bool
pub fn to_matrix(&self) -> Mat4
pub fn compute_affine(&self) -> Affine3A
pub fn mul_transform(&self, transform: Transform) -> Transform
```

## GlobalTransform

`GlobalTransform` is the **world-space** transform. It is computed automatically
from the entity's `Transform` and its parent hierarchy during `PostUpdate`.

```rust
// Decompose
pub fn to_scale_rotation_translation(&self) -> (Vec3, Quat, Vec3)

// Get translation
pub fn translation(&self) -> Vec3

// Transform a point from local to world space
pub fn transform_point(&self, point: Vec3) -> Vec3
```

### Important: Transform vs GlobalTransform

- **Transform** = position relative to parent (or world if no parent)
- **GlobalTransform** = absolute world position (read-only, computed by engine)
- **Set Transform** to move entities
- **Read GlobalTransform** to get world position
- GlobalTransform updates happen in `PostUpdate` — there's a 1-frame lag if you
  modify Transform during or after PostUpdate

## 2D Z-Ordering

In 2D, `translation.z` controls draw order:
- Higher z = drawn in front
- Lower z = drawn behind

```rust
// Background layer
Transform::from_xyz(0.0, 0.0, 0.0)
// Foreground layer
Transform::from_xyz(0.0, 0.0, 1.0)
// UI layer
Transform::from_xyz(0.0, 0.0, 10.0)
```
