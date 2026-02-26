---
source: Context7 API
library: Bevy
package: bevy
topic: Visibility enum, Projection/OrthographicProjection, Window/PrimaryWindow query
fetched: 2026-02-26T12:00:00Z
official_docs: https://docs.rs/bevy/latest/bevy/camera/prelude/enum.Visibility.html
---

# Visibility, Projection, and Window in Bevy 0.18

## Visibility Enum

```rust
pub enum Visibility {
    Inherited,  // Inherits from parent (default). Root-level Inherited = visible.
    Hidden,     // Unconditionally hidden. Children set to Inherited also hidden.
    Visible,    // Unconditionally visible, even if parent is hidden.
}
```

### Usage
- `Visibility::Hidden` — hides entity and all `Inherited` descendants
- `Visibility::Visible` — forces visible even if parent is hidden
- `Visibility::Inherited` — follows parent's visibility (default)

Propagation is handled by `visibility_propagate_system` which updates `InheritedVisibility`.

## Projection Enum

```rust
pub enum Projection {
    Perspective(PerspectiveProjection),
    Orthographic(OrthographicProjection),
    Custom(CustomProjection),
}
```

### Pattern matching works:
```rust
if let Projection::Orthographic(ref ortho) = *projection {
    // access ortho.scale, ortho.scaling_mode, etc.
}
```

### OrthographicProjection fields:
- `near: f32` (default 0.0)
- `far: f32` (default 1000.0)
- `viewport_origin: Vec2` (default (0.5, 0.5))
- `scaling_mode: ScalingMode` (default WindowSize)
- `scale: f32` (default 1.0)
- `area: Rect` (auto-updated by camera_system)

### Example: configure projection
```rust
let projection = Projection::Orthographic(OrthographicProjection {
    scaling_mode: ScalingMode::WindowSize,
    scale: 0.01,
    ..OrthographicProjection::default_2d()
});
```

### ScalingMode variants:
- `WindowSize` — 1:1 world units to pixels (at scale 1.0)
- `Fixed { width, height }` — fixed size, stretches
- `AutoMin { min_width, min_height }` — keeps aspect, minimum bounds
- `AutoMax { max_width, max_height }` — keeps aspect, maximum bounds
- `FixedVertical { viewport_height }` — constant height
- `FixedHorizontal { viewport_width }` — constant width

## Window + PrimaryWindow Query

`PrimaryWindow` is a marker component. Query pattern:

```rust
fn my_system(
    window_query: Query<&Window, With<PrimaryWindow>>,
) {
    let window = window_query.single();
    let width = window.width();
    let height = window.height();
}
```

- `PrimaryWindow` is a unit struct marker: `pub struct PrimaryWindow;`
- Added by `WindowPlugin` when `primary_window` is `Some`
- Assumed to exist on only 1 entity at a time
- `Query<&Window, With<PrimaryWindow>>` is the correct pattern
