# Parallax Background Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a configurable, hot-reloadable parallax background system with per-layer scroll speeds, repeat flags, and z-ordering — all driven from a RON file.

**Architecture:** New `src/parallax/` module with 4 files. A `ParallaxConfigAsset` loaded through the existing `registry` pipeline (same pattern as `WorldConfig`/`PlayerConfig`). Each layer spawns sprite entities positioned relative to the camera. A scroll system runs every frame after `camera_follow_player`.

**Tech Stack:** Bevy 0.18 (Sprite, Asset, Timer), RON config, existing `RonLoader<T>`.

---

### Task 1: RON Config & Asset Types

**Files:**
- Create: `assets/data/parallax.ron`
- Create: `src/parallax/config.rs`
- Modify: `src/registry/assets.rs` — add `ParallaxConfigAsset`
- Modify: `src/registry/mod.rs` — register loader, loading, hot-reload

**Step 1: Create the RON config file**

Create `assets/data/parallax.ron`:

```ron
(
  layers: [
    (
      name: "sky",
      image: "backgrounds/sky.png",
      speed_x: 0.0,
      speed_y: 0.0,
      repeat_x: false,
      repeat_y: false,
      z_order: -100.0,
    ),
    (
      name: "far_mountains",
      image: "backgrounds/far_mountains.png",
      speed_x: 0.1,
      speed_y: 0.05,
      repeat_x: true,
      repeat_y: false,
      z_order: -90.0,
    ),
    (
      name: "near_mountains",
      image: "backgrounds/near_mountains.png",
      speed_x: 0.3,
      speed_y: 0.15,
      repeat_x: true,
      repeat_y: false,
      z_order: -80.0,
    ),
  ],
)
```

Note: these reference images the user will provide later. The system will handle missing images gracefully (Bevy logs a warning but doesn't crash).

**Step 2: Create `src/parallax/config.rs`**

```rust
use bevy::prelude::*;
use serde::Deserialize;

/// Definition of a single parallax layer from RON.
#[derive(Debug, Clone, Deserialize)]
pub struct ParallaxLayerDef {
    pub name: String,
    pub image: String,
    pub speed_x: f32,
    pub speed_y: f32,
    pub repeat_x: bool,
    pub repeat_y: bool,
    pub z_order: f32,
}

/// Runtime resource holding the parallax configuration.
#[derive(Resource, Debug, Clone, Deserialize)]
pub struct ParallaxConfig {
    pub layers: Vec<ParallaxLayerDef>,
}
```

**Step 3: Add `ParallaxConfigAsset` to `src/registry/assets.rs`**

Add after existing assets:

```rust
use super::parallax::config::ParallaxLayerDef;

/// Asset loaded from parallax.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct ParallaxConfigAsset {
    pub layers: Vec<ParallaxLayerDef>,
}
```

Note: `ParallaxLayerDef` is reused from `parallax::config`. The `registry` module needs access to `parallax` — add `use crate::parallax` in assets.rs.

**Step 4: Register in `src/registry/mod.rs`**

Changes needed:
1. Add `ParallaxConfigAsset` to imports
2. Add `Handle<ParallaxConfigAsset>` to `RegistryHandles` and `LoadingAssets`
3. Register `RonLoader::<ParallaxConfigAsset>::new(&["parallax.ron"])` in `RegistryPlugin::build`
4. `init_asset::<ParallaxConfigAsset>()` in `build`
5. In `start_loading`: `asset_server.load::<ParallaxConfigAsset>("data/parallax.ron")`
6. In `check_loading`: extract `ParallaxConfig` resource from the asset
7. Add `hot_reload_parallax` system

The hot-reload for parallax differs from others: on config change, we need to despawn all existing `ParallaxLayer` entities and trigger a respawn. Use a `ParallaxDirty` resource flag that the spawn system checks.

```rust
// In check_loading, after all assets verified loaded:
commands.insert_resource(ParallaxConfig {
    layers: parallax_cfg.layers.clone(),
});

// hot_reload_parallax:
fn hot_reload_parallax(
    mut events: MessageReader<AssetEvent<ParallaxConfigAsset>>,
    handles: Res<RegistryHandles>,
    assets: Res<Assets<ParallaxConfigAsset>>,
    mut config: ResMut<ParallaxConfig>,
    mut commands: Commands,
    layer_query: Query<Entity, With<crate::parallax::spawn::ParallaxLayer>>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event {
            if *id == handles.parallax.id() {
                if let Some(asset) = assets.get(&handles.parallax) {
                    config.layers = asset.layers.clone();
                    // Despawn all existing layers — spawn system will recreate
                    for entity in &layer_query {
                        commands.entity(entity).despawn();
                    }
                    info!("Hot-reloaded ParallaxConfig ({} layers)", asset.layers.len());
                }
            }
        }
    }
}
```

**Step 5: Commit**

```bash
git add src/parallax/config.rs src/registry/assets.rs src/registry/mod.rs assets/data/parallax.ron
git commit -m "feat(parallax): add RON config and asset loading pipeline"
```

---

### Task 2: Parallax Module — Spawn System

**Files:**
- Create: `src/parallax/spawn.rs`
- Create: `src/parallax/mod.rs`

**Step 1: Create `src/parallax/spawn.rs`**

The spawn system creates sprite entities for each layer. For repeat layers, it spawns enough copies to cover the screen plus margin.

```rust
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::config::{ParallaxConfig, ParallaxLayerDef};

/// Marker component for a parallax layer root entity.
#[derive(Component)]
pub struct ParallaxLayer {
    pub speed_x: f32,
    pub speed_y: f32,
    pub repeat_x: bool,
    pub repeat_y: bool,
    pub texture_size: Vec2,
}

/// Marker for individual tile sprites within a repeating layer.
#[derive(Component)]
pub struct ParallaxTile;

/// System: spawn parallax layers from config.
/// Runs on OnEnter(InGame) and after hot-reload despawn.
pub fn spawn_parallax_layers(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    config: Res<ParallaxConfig>,
    existing: Query<Entity, With<ParallaxLayer>>,
) {
    // Don't double-spawn if layers already exist
    if !existing.is_empty() {
        return;
    }

    for layer_def in &config.layers {
        let image_handle: Handle<Image> = asset_server.load(&layer_def.image);

        commands.spawn((
            ParallaxLayer {
                speed_x: layer_def.speed_x,
                speed_y: layer_def.speed_y,
                repeat_x: layer_def.repeat_x,
                repeat_y: layer_def.repeat_y,
                texture_size: Vec2::ZERO, // filled in scroll system once image loads
            },
            Sprite::from_image(image_handle),
            Transform::from_xyz(0.0, 0.0, layer_def.z_order),
            Visibility::default(),
        ));
    }
}
```

Note: `texture_size` starts as ZERO because we don't know image dimensions at spawn time. The scroll system will fill it in once the `Image` asset is loaded.

**Step 2: Create `src/parallax/mod.rs`**

```rust
pub mod config;
pub mod scroll;
pub mod spawn;

use bevy::prelude::*;

use crate::camera::follow::camera_follow_player;
use crate::registry::AppState;

pub struct ParallaxPlugin;

impl Plugin for ParallaxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn::spawn_parallax_layers)
            .add_systems(
                Update,
                (spawn::spawn_parallax_layers, scroll::parallax_scroll)
                    .chain()
                    .after(camera_follow_player)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
```

`spawn_parallax_layers` also runs in Update (after hot-reload despawns, it respawns on next frame thanks to the `existing.is_empty()` guard).

**Step 3: Register in `src/main.rs`**

Add `mod parallax;` and `.add_plugins(parallax::ParallaxPlugin)`.

**Step 4: Commit**

```bash
git add src/parallax/ src/main.rs
git commit -m "feat(parallax): add spawn system and plugin registration"
```

---

### Task 3: Scroll System

**Files:**
- Create: `src/parallax/scroll.rs`

**Step 1: Create `src/parallax/scroll.rs`**

The core logic: position each layer's sprite(s) relative to the camera using the speed factors.

For non-repeat layers: single sprite, offset = camera_pos * speed.
For repeat layers: calculate how many copies tile the visible area, position them with modular offset.

```rust
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::spawn::ParallaxLayer;

/// System: scroll parallax layers based on camera position.
pub fn parallax_scroll(
    camera_query: Query<(&Transform, &Projection), With<Camera2d>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    images: Res<Assets<Image>>,
    mut layer_query: Query<(&mut ParallaxLayer, &mut Transform, &Sprite), Without<Camera2d>>,
) {
    let Ok((camera_tf, projection)) = camera_query.single() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

    let proj_scale = match projection {
        Projection::Orthographic(ortho) => ortho.scale,
        _ => 1.0,
    };

    let cam_x = camera_tf.translation.x;
    let cam_y = camera_tf.translation.y;
    let visible_w = window.width() * proj_scale;
    let visible_h = window.height() * proj_scale;

    for (mut layer, mut transform, sprite) in &mut layer_query {
        // Resolve texture size if not yet known
        if layer.texture_size == Vec2::ZERO {
            if let Some(image) = images.get(&sprite.image) {
                let size = image.size();
                layer.texture_size = Vec2::new(size.x as f32, size.y as f32);
            } else {
                continue; // image not loaded yet
            }
        }

        let tex_w = layer.texture_size.x;
        let tex_h = layer.texture_size.y;

        // Calculate parallax offset
        let offset_x = cam_x * layer.speed_x;
        let offset_y = cam_y * layer.speed_y;

        if layer.repeat_x {
            // For repeat: sprite position wraps, anchored to camera
            // The sprite is centered on camera X, shifted by the modular offset
            let wrapped_x = if tex_w > 0.0 {
                offset_x - (offset_x / tex_w).floor() * tex_w
            } else {
                0.0
            };
            // Position so the leftmost copy covers screen left edge
            // Camera is at cam_x, visible left = cam_x - visible_w/2
            // We place sprite so it tiles from there
            transform.translation.x = cam_x - wrapped_x;
        } else {
            // Non-repeat: move with camera at reduced speed
            transform.translation.x = cam_x - offset_x;
        }

        if layer.repeat_y {
            let wrapped_y = if tex_h > 0.0 {
                offset_y - (offset_y / tex_h).floor() * tex_h
            } else {
                0.0
            };
            transform.translation.y = cam_y - wrapped_y;
        } else {
            transform.translation.y = cam_y - offset_y;
        }
    }
}
```

Important: for repeat layers with a single sprite, the texture must be large enough to cover the screen. For a proper tiling implementation, we'll use `custom_size` on the sprite to stretch it to cover the visible area plus margins, combined with Bevy's `ImageMode::Tiled` if available, OR spawn multiple sprite copies.

**Approach for repeat tiling — spawn multiple copies as children:**

A simpler and more robust approach: for repeat layers, spawn enough child sprite entities to cover `visible_area + texture_size` (one extra copy per edge). Reposition children each frame.

However, since the visible area depends on the camera and window which may change (resize), let's use a simpler single-sprite approach first:

- Each layer = 1 sprite
- For repeat layers: set `custom_size` to cover the visible area, and use Bevy's sprite tiling if supported
- If Bevy 0.18 doesn't support sprite tiling natively, we spawn 3×3 grid of copies

Let's check Bevy 0.18 for `ImageMode::Tiled` and decide. For now, implement the single-sprite non-repeat case and leave a TODO for tiling in the follow-up step.

**Step 2: Verify build compiles**

Run: `cargo build`
Expected: compiles with warnings about unused fields (repeat handling incomplete).

**Step 3: Commit**

```bash
git add src/parallax/scroll.rs
git commit -m "feat(parallax): add scroll system with camera-relative positioning"
```

---

### Task 4: Repeat Tiling with Multiple Sprites

**Files:**
- Modify: `src/parallax/spawn.rs` — spawn grid of children for repeat layers
- Modify: `src/parallax/scroll.rs` — reposition children each frame

**Step 1: Update spawn to create child grid**

For layers with `repeat_x` or `repeat_y`, the parent entity is the layer root (holds `ParallaxLayer` component). Child entities are the actual visible sprites.

Update `spawn_parallax_layers`:
- Non-repeat: parent entity has the `Sprite` directly (as before)
- Repeat: parent entity has no sprite, children are sprites. Number of children determined dynamically in scroll system.

We'll use a different approach — each repeat layer spawns a pool of tile sprites (e.g., 9 = 3×3 grid). The scroll system repositions them. If more are needed (large textures, small screen), the system spawns additional ones.

For simplicity and correctness, let's spawn children dynamically in the scroll system the first time `texture_size` is resolved.

**Step 2: Update scroll system**

When `texture_size` is first resolved for a repeat layer:
1. Calculate copies_x = `ceil(visible_w / tex_w) + 2`
2. Calculate copies_y = `ceil(visible_h / tex_h) + 2` (or 1 if no repeat_y)
3. Spawn `copies_x * copies_y` child sprites
4. Each frame: reposition children in grid around camera

```rust
// ParallaxLayer gets additional field:
pub initialized: bool,

// In scroll system, when texture_size first known and !initialized:
// Spawn children, set initialized = true

// Each frame for repeat layers:
// offset_x = (cam_x * speed_x) % tex_w
// child[i].x = cam_x - visible_w/2 - tex_w + offset_x + i * tex_w
```

**Step 3: Full build + test**

Run: `cargo build && cargo run`
Expected: parallax layers visible behind tilemap (once user provides PNG assets).

**Step 4: Commit**

```bash
git add src/parallax/
git commit -m "feat(parallax): implement repeat tiling with dynamic child sprites"
```

---

### Task 5: Integration & Polish

**Files:**
- Modify: `src/registry/mod.rs` — final hot-reload wiring
- Modify: `src/ui/debug_panel.rs` — optionally show parallax info in debug panel

**Step 1: Verify hot-reload works**

1. Run the game
2. Edit `parallax.ron` — change a `speed_x` value
3. Verify old layers despawn and new ones appear with updated speed

**Step 2: Add parallax section to debug panel (optional)**

In `debug_panel.rs`, add a collapsible section showing:
- Number of active parallax layers
- Per-layer: name, speed_x, speed_y, repeat flags

**Step 3: Create placeholder `assets/backgrounds/` directory**

Create `assets/backgrounds/.gitkeep` so the directory exists in git. User will add PNG files there.

**Step 4: Final build + run**

Run: `cargo build && cargo run`
Expected: no crashes, parallax system loads config, logs warnings for missing images.

**Step 5: Commit**

```bash
git add -A
git commit -m "feat(parallax): integration, debug panel, placeholder directory"
```

---

## File Summary

| File | Action | Purpose |
|------|--------|---------|
| `assets/data/parallax.ron` | Create | Layer configuration |
| `assets/backgrounds/.gitkeep` | Create | Placeholder for user assets |
| `src/parallax/mod.rs` | Create | Plugin, module exports |
| `src/parallax/config.rs` | Create | ParallaxConfig, ParallaxLayerDef |
| `src/parallax/spawn.rs` | Create | Spawn system |
| `src/parallax/scroll.rs` | Create | Scroll + tiling system |
| `src/registry/assets.rs` | Modify | Add ParallaxConfigAsset |
| `src/registry/mod.rs` | Modify | Register loader, loading, hot-reload |
| `src/main.rs` | Modify | Add parallax module + plugin |
| `src/ui/debug_panel.rs` | Modify | Optional parallax debug info |
