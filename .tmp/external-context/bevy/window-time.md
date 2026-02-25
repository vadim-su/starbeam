---
source: Context7 API + docs.rs/bevy/0.18.0
library: Bevy
package: bevy
version: "0.18.0"
topic: Window, PrimaryWindow, Time, delta time
fetched: 2025-02-25T00:00:00Z
official_docs: https://docs.rs/bevy/0.18.0/bevy/window/struct.Window.html
---

# Window & Time (Bevy 0.18)

## PrimaryWindow

```rust
pub struct PrimaryWindow;  // Marker component
```

Identifies the primary window. Added automatically by `WindowPlugin`.

### Querying the Primary Window

```rust
fn system(windows: Query<&Window, With<PrimaryWindow>>) {
    let window = windows.single();

    // Window size in logical pixels
    let size: Vec2 = window.size();
    let width: f32 = window.width();
    let height: f32 = window.height();

    // Cursor position (None if cursor is outside window)
    if let Some(cursor_pos) = window.cursor_position() {
        // cursor_pos is Vec2, origin at top-left
    }
}
```

### Window Methods

```rust
window.size() -> Vec2           // client area size in logical pixels
window.width() -> f32           // width in logical pixels
window.height() -> f32          // height in logical pixels
window.cursor_position() -> Option<Vec2>  // cursor pos, top-left origin
window.physical_width() -> u32  // physical pixels
window.physical_height() -> u32
window.scale_factor() -> f32    // DPI scale
```

### WindowResized Event

```rust
pub struct WindowResized {
    pub window: Entity,
    pub width: f32,
    pub height: f32,
}

fn on_resize(mut resize_reader: EventReader<WindowResized>) {
    for e in resize_reader.read() {
        println!("Window resized to {}x{}", e.width, e.height);
    }
}
```

### Configuring the Window

```rust
App::new()
    .add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "My Game".into(),
            resolution: (1280.0, 720.0).into(),
            resizable: true,
            ..default()
        }),
        ..default()
    }))
```

## Time

### Default Time (Res<Time>)

In `Update` schedule, this is `Time<Virtual>`. In `FixedUpdate`, this is `Time<Fixed>`.

```rust
fn system(time: Res<Time>) {
    // Delta time since last frame
    let dt: Duration = time.delta();
    let dt_secs: f32 = time.delta_secs();

    // Total elapsed time
    let elapsed: Duration = time.elapsed();
    let elapsed_secs: f32 = time.elapsed_secs();
}
```

### Key Time Methods

```rust
time.delta()         -> Duration   // time since last update
time.delta_secs()    -> f32        // delta as f32 seconds
time.elapsed()       -> Duration   // total elapsed time
time.elapsed_secs()  -> f32        // total elapsed as f32 seconds
```

### Real Time (unaffected by pause/speed)

```rust
fn system(time: Res<Time<Real>>) {
    let real_dt = time.delta_secs();
    // Always real wall-clock time, even if game is paused
}
```

### Virtual Time (pausable, scalable)

```rust
fn system(time: Res<Time<Virtual>>) {
    let virtual_dt = time.delta_secs();
    let speed = time.effective_speed();
}
```

### Pausing Game Time

```rust
fn pause_system(mut time: ResMut<Time<Virtual>>) {
    time.pause();
    // or
    time.unpause();
}
```

### Fixed Time

```rust
fn fixed_system(time: Res<Time<Fixed>>) {
    // In FixedUpdate schedule, this gives the fixed timestep
    let fixed_dt = time.delta_secs();
}
```

## Common Pattern: Frame-Rate Independent Movement

```rust
fn move_player(
    time: Res<Time>,
    mut query: Query<&mut Transform, With<Player>>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    let speed = 200.0; // pixels per second
    let dt = time.delta_secs();

    for mut transform in &mut query {
        let mut direction = Vec3::ZERO;

        if keys.pressed(KeyCode::KeyW) { direction.y += 1.0; }
        if keys.pressed(KeyCode::KeyS) { direction.y -= 1.0; }
        if keys.pressed(KeyCode::KeyA) { direction.x -= 1.0; }
        if keys.pressed(KeyCode::KeyD) { direction.x += 1.0; }

        if direction != Vec3::ZERO {
            direction = direction.normalize();
        }

        transform.translation += direction * speed * dt;
    }
}
```
