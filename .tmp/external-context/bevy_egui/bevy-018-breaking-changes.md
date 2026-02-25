---
source: GitHub releases (vladbat00/bevy_egui)
library: bevy_egui
package: bevy_egui
topic: Bevy 0.18 breaking changes, migration
fetched: 2025-02-25T12:00:00Z
official_docs: https://github.com/vladbat00/bevy_egui/releases
---

# bevy_egui 0.39.x: Bevy 0.18 Breaking Changes & Migration

## 0.39.0 (Bevy 0.18 release)

### Changed
- **Update to Bevy 0.18.**
- **Removed deprecated `PICKING_ORDER` constant.** Picking order is now calculated dynamically via `EguiPickingOrder`.

### Fixed
- Fix broken inputs when using custom EventLoop events.
- Fix IME breaking backspace and arrow buttons on Linux.
- No longer insert the `CursorIcon` component into context entities.
- Fix some unresolved doc links.

## 0.39.1 (Bugfix)

### Fixed
- Fix text AA by no longer re-premultiplying alpha for egui textures.
  - **Potentially breaking rendering**: Even though not a breaking API change, it changes rendering of user textures. If you were passing custom textures to egui, check the PR for migration notes.

## Important Changes from 0.35.0+ (still relevant)

These breaking changes were introduced before Bevy 0.18 but are critical to understand:

### 1. EguiPlugin must use `::default()`
```rust
// OLD (deprecated):
// .add_plugins(EguiPlugin { enable_multipass_for_primary_context: true })

// NEW (0.35+):
.add_plugins(EguiPlugin::default())
```

### 2. EguiContext attached to cameras (not windows)
- Egui contexts are now attached to **camera entities**, not window entities.
- At least one camera must exist for egui to render.
- Egui auto-attaches to the first created camera.

### 3. Systems must return Result (Bevy 0.16+ pattern)
```rust
// OLD:
fn ui_system(mut contexts: EguiContexts) {
    let ctx = contexts.ctx_mut();
    // ...
}

// NEW (0.35+):
fn ui_system(mut contexts: EguiContexts) -> Result {
    let ctx = contexts.ctx_mut()?;  // Note the ? operator
    // ...
    Ok(())
}
```

### 4. Use `EguiPrimaryContextPass` schedule
```rust
// Systems go in EguiPrimaryContextPass, NOT Update
.add_systems(EguiPrimaryContextPass, my_ui_system)
```

### 5. Custom plugin integration pattern
```rust
pub struct MyPlugin;

impl Plugin for MyPlugin {
    fn build(&self, app: &mut App) {
        assert!(app.is_plugin_added::<EguiPlugin>());
        app.add_systems(EguiPrimaryContextPass, ui_system);
    }
}
```
