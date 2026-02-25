---
source: Context7 API + docs.rs/bevy/0.18.0
library: Bevy
package: bevy
version: "0.18.0"
topic: App setup, plugins, DefaultPlugins
fetched: 2025-02-25T00:00:00Z
official_docs: https://docs.rs/bevy/0.18.0/bevy/app/struct.App.html
---

# App Setup & Plugin Registration (Bevy 0.18)

## App Builder Pattern

```rust
use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(MyResource { value: 42 })
        .init_resource::<AnotherResource>()  // requires Default impl
        .add_systems(Startup, setup)
        .add_systems(Update, (system_a, system_b))
        .add_systems(FixedUpdate, physics_system)
        .add_event::<MyEvent>()
        .run();
}
```

### Key `App` Methods

```rust
// Core builder methods:
App::new() -> App
app.add_plugins(DefaultPlugins) -> &mut App
app.add_plugins(MyPlugin) -> &mut App
app.add_systems(schedule, systems) -> &mut App  // schedule: Startup, Update, FixedUpdate, etc.
app.insert_resource(resource) -> &mut App       // insert a resource with a value
app.init_resource::<T>() -> &mut App            // insert resource using Default::default()
app.add_event::<T>() -> &mut App                // register an event type
app.run()                                        // starts the app loop
```

## DefaultPlugins

```rust
pub struct DefaultPlugins;
```

Includes (among others):
- `WindowPlugin` — window management
- `InputPlugin` — keyboard, mouse, gamepad input
- `TimePlugin` — `Time` resource
- `TransformPlugin` — Transform/GlobalTransform propagation
- `RenderPlugin` — rendering pipeline
- `SpritePlugin` — 2D sprite rendering
- `CameraPlugin` — camera components
- `AssetPlugin` — asset loading
- `ImagePlugin` — image handling
- `AudioPlugin` — audio playback
- `StatesPlugin` — state management

## Implementing a Custom Plugin

```rust
pub struct MyGamePlugin;

impl Plugin for MyGamePlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<GameState>()
            .add_event::<ScoreEvent>()
            .add_systems(Startup, setup_game)
            .add_systems(Update, (
                handle_input,
                update_physics,
                render_ui,
            ));
    }
}

// Usage:
fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(MyGamePlugin)
        .run();
}
```

### Plugin Trait Full Signature

```rust
impl Plugin for MyPlugin {
    fn build(&self, app: &mut App) {
        // Configures the App to which this plugin is added. (REQUIRED)
    }

    fn ready(&self, _app: &App) -> bool {
        // Has the plugin finished its setup? Default: true
        true
    }

    fn finish(&self, _app: &mut App) {
        // Finish adding this plugin to the App (runs after build).
    }

    fn cleanup(&self, _app: &mut App) {
        // Runs after all plugins are built and finished.
    }

    fn name(&self) -> &str {
        // Configures a name for the Plugin.
        "MyPlugin"
    }

    fn is_unique(&self) -> bool {
        // If the plugin can be instantiated several times, return false.
        true
    }
}
```

## Schedules (where systems run)

- `Startup` — runs once at app start
- `Update` — runs every frame
- `FixedUpdate` — runs at a fixed timestep
- `PostUpdate` — runs after Update (GlobalTransform propagation happens here)

## Full Example: Minimal 2D App

```rust
use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        .add_systems(Update, hello_world)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn hello_world() {
    println!("hello world");
}
```
