---
source: Context7 API + docs.rs/bevy/0.18.0 + bevy-cheatbook
library: Bevy
package: bevy
version: "0.18.0"
topic: Input handling - keyboard, mouse, cursor
fetched: 2025-02-25T00:00:00Z
official_docs: https://docs.rs/bevy/0.18.0/bevy/input/prelude/struct.ButtonInput.html
---

# Input Handling (Bevy 0.18)

## Keyboard Input via Resource

```rust
fn keyboard_input(keys: Res<ButtonInput<KeyCode>>) {
    if keys.just_pressed(KeyCode::Space) {
        // Space was pressed this frame
    }
    if keys.just_released(KeyCode::ControlLeft) {
        // Left Ctrl was released this frame
    }
    if keys.pressed(KeyCode::KeyW) {
        // W is being held down
    }
    // Check multiple keys at once
    if keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
        // Either shift is held
    }
    if keys.any_just_pressed([KeyCode::Delete, KeyCode::Backspace]) {
        // Either delete or backspace was just pressed
    }
}
```

### Common KeyCode Values

```rust
KeyCode::KeyA .. KeyCode::KeyZ    // Letter keys
KeyCode::Digit0 .. KeyCode::Digit9  // Number keys
KeyCode::Space
KeyCode::Enter
KeyCode::Escape
KeyCode::Tab
KeyCode::Backspace
KeyCode::Delete
KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::ArrowLeft, KeyCode::ArrowRight
KeyCode::ShiftLeft, KeyCode::ShiftRight
KeyCode::ControlLeft, KeyCode::ControlRight
KeyCode::AltLeft, KeyCode::AltRight
KeyCode::F1 .. KeyCode::F12
```

### ButtonInput<KeyCode> Methods

```rust
keys.pressed(KeyCode)       -> bool  // currently held down
keys.just_pressed(KeyCode)  -> bool  // pressed this frame
keys.just_released(KeyCode) -> bool  // released this frame
keys.any_pressed([...])     -> bool  // any of the keys held
keys.any_just_pressed([...])-> bool  // any just pressed
keys.any_just_released([...])-> bool // any just released
keys.get_pressed()          -> impl Iterator<Item = &KeyCode>  // all pressed keys
keys.get_just_pressed()     -> impl Iterator<Item = &KeyCode>
keys.get_just_released()    -> impl Iterator<Item = &KeyCode>
```

## Run Conditions for Input (Declarative)

```rust
use bevy::input::common_conditions::*;

app.add_systems(Update, (
    handle_jump.run_if(input_just_pressed(KeyCode::Space)),
    handle_shooting.run_if(input_pressed(KeyCode::Enter)),
));
```

## Mouse Button Input

```rust
fn mouse_click_system(mouse_button_input: Res<ButtonInput<MouseButton>>) {
    if mouse_button_input.pressed(MouseButton::Left) {
        info!("left mouse currently pressed");
    }
    if mouse_button_input.just_pressed(MouseButton::Left) {
        info!("left mouse just pressed");
    }
    if mouse_button_input.just_released(MouseButton::Left) {
        info!("left mouse just released");
    }
}
```

### MouseButton Variants

```rust
MouseButton::Left
MouseButton::Right
MouseButton::Middle
MouseButton::Back
MouseButton::Forward
MouseButton::Other(u16)
```

## Cursor Position

### Via Window Resource

```rust
fn cursor_system(windows: Query<&Window, With<PrimaryWindow>>) {
    let window = windows.single();
    if let Some(cursor_position) = window.cursor_position() {
        // cursor_position is Vec2 in logical pixels
        // Origin is top-left of window
        info!("Cursor at: {:?}", cursor_position);
    }
}
```

### Via CursorMoved Event

```rust
fn cursor_events(mut cursor_evr: EventReader<CursorMoved>) {
    for ev in cursor_evr.read() {
        info!(
            "Cursor moved: window={:?}, position={:?}, delta={:?}",
            ev.window, ev.position, ev.delta
        );
    }
}
```

#### CursorMoved Struct

```rust
pub struct CursorMoved {
    pub window: Entity,       // The window entity
    pub position: Vec2,       // Cursor position in logical pixels
    pub delta: Option<Vec2>,  // Change since last event (None if cursor was outside)
}
```

## Keyboard Events (Low-Level)

```rust
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};

fn text_input(
    mut evr_kbd: EventReader<KeyboardInput>,
    mut string: Local<String>,
) {
    for ev in evr_kbd.read() {
        if ev.state == ButtonState::Released {
            continue;
        }
        match &ev.logical_key {
            Key::Enter => {
                println!("Text input: {}", &*string);
                string.clear();
            }
            Key::Backspace => { string.pop(); }
            Key::Character(input) => {
                if input.chars().any(|c| c.is_control()) { continue; }
                string.push_str(&input);
            }
            _ => {}
        }
    }
}
```

## Converting Screen Coords to World Coords (2D)

```rust
fn screen_to_world(
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
) {
    let window = windows.single();
    let (camera, camera_transform) = camera_q.single();

    if let Some(cursor_pos) = window.cursor_position() {
        if let Ok(world_pos) = camera.viewport_to_world_2d(camera_transform, cursor_pos) {
            info!("World position: {:?}", world_pos);
        }
    }
}
```
