---
source: Context7 API + GitHub releases + crates.io
library: bevy_egui
package: bevy_egui
topic: setup, compatibility, Bevy 0.18
fetched: 2025-02-25T12:00:00Z
official_docs: https://docs.rs/bevy_egui/0.39.1/bevy_egui/
---

# bevy_egui: Setup & Bevy 0.18 Compatibility

## Version Compatibility

| Bevy Version | bevy_egui Version | egui Version |
|---|---|---|
| **0.18** | **0.39.x** (latest: 0.39.1) | 0.33 |
| 0.17 | 0.37.x | 0.32 |
| 0.16 | 0.35.x - 0.36.x | 0.31 - 0.32 |

## Cargo.toml Dependency

```toml
[dependencies]
bevy_egui = "0.39"
```

Or pin to exact:
```toml
[dependencies]
bevy_egui = "0.39.1"
```

Default features include: `manage_clipboard`, `open_url`, `default_fonts`, `render`, `bevy_ui`, `picking`.

To disable picking (if not needed):
```toml
[dependencies]
bevy_egui = { version = "0.39", default-features = false, features = ["manage_clipboard", "open_url", "default_fonts", "render"] }
```

## Minimum Rust Version

Rust 1.89.0 (edition 2024)

## Basic Setup

```rust
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin, EguiPrimaryContextPass};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin::default())
        .add_systems(Startup, setup_camera_system)
        .add_systems(EguiPrimaryContextPass, ui_example_system)
        .run();
}

fn setup_camera_system(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn ui_example_system(mut contexts: EguiContexts) -> Result {
    egui::Window::new("Hello").show(contexts.ctx_mut()?, |ui| {
        ui.label("world");
    });
    Ok(())
}
```

### Key Points

1. **`EguiPlugin::default()`** - Always use `::default()`. The old single-pass mode is deprecated.
2. **`EguiPrimaryContextPass`** - This is the schedule where UI systems must be added (NOT `Update`).
3. **`EguiContexts`** - The system parameter to access egui contexts.
4. **`ctx_mut()?`** - Returns `Result<&mut Context, QuerySingleError>`. Systems must return `Result`.
5. **Camera required** - Egui attaches itself to the first camera. At least one camera must exist.

## Imports Cheat Sheet

```rust
use bevy_egui::{
    egui,                      // Re-exported egui crate
    EguiContexts,              // SystemParam for accessing egui contexts
    EguiPlugin,                // The main plugin
    EguiPrimaryContextPass,    // Schedule for UI systems
    EguiContext,               // Component on camera entities
    EguiGlobalSettings,        // Resource for global config
    PrimaryEguiContext,        // Marker component for primary context camera
};
```
