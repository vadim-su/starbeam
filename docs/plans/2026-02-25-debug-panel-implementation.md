# Debug Panel Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the single-line debug HUD with a full egui inspector panel (F3 toggle, collapsible sections, cursor tile info).

**Architecture:** Add `bevy_egui` crate. One resource (`DebugUiState`) tracks visibility and section collapse state. One system in `Update` handles F3 toggle. One system in `EguiPrimaryContextPass` draws the panel. Remove old `debug_hud.rs`.

**Tech Stack:** Bevy 0.18, bevy_egui 0.39, egui 0.33

---

### Task 1: Add dependencies and FrameTimeDiagnosticsPlugin

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/main.rs`

**Step 1: Add bevy_egui to Cargo.toml**

In `Cargo.toml`, add to `[dependencies]`:

```toml
bevy_egui = "0.39"
```

**Step 2: Add EguiPlugin and FrameTimeDiagnosticsPlugin to main.rs**

In `src/main.rs`, add imports:

```rust
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy_egui::EguiPlugin;
```

In `fn main()`, add plugins after `TilemapPlugin`:

```rust
.add_plugins(EguiPlugin::default())
.add_plugins(FrameTimeDiagnosticsPlugin::default())
```

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: compiles with no errors (new deps downloaded)

**Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs
git commit -m "feat: add bevy_egui and FrameTimeDiagnosticsPlugin dependencies"
```

---

### Task 2: Create DebugUiState resource and toggle system

**Files:**
- Create: `src/ui/debug_panel.rs`
- Modify: `src/ui/mod.rs`

**Step 1: Create debug_panel.rs with DebugUiState and toggle**

Create `src/ui/debug_panel.rs`:

```rust
use bevy::prelude::*;

/// Tracks debug panel visibility and section collapsed states.
#[derive(Resource)]
pub struct DebugUiState {
    pub visible: bool,
    pub show_performance: bool,
    pub show_player: bool,
    pub show_cursor: bool,
    pub show_world: bool,
}

impl Default for DebugUiState {
    fn default() -> Self {
        Self {
            visible: false,
            show_performance: true,
            show_player: true,
            show_cursor: true,
            show_world: true,
        }
    }
}

/// Toggles debug panel visibility on F3 press.
pub fn toggle_debug_panel(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<DebugUiState>,
) {
    if keyboard.just_pressed(KeyCode::F3) {
        state.visible = !state.visible;
    }
}
```

**Step 2: Register in UiPlugin (alongside old HUD for now)**

In `src/ui/mod.rs`, add `pub mod debug_panel;` and register the resource + toggle system:

```rust
pub mod debug_hud;
pub mod debug_panel;

use bevy::prelude::*;

use crate::registry::AppState;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<debug_panel::DebugUiState>()
            .add_systems(OnEnter(AppState::InGame), debug_hud::spawn_debug_hud)
            .add_systems(
                Update,
                (
                    debug_hud::update_debug_hud,
                    debug_panel::toggle_debug_panel,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
```

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: compiles, game still works with old HUD

**Step 4: Commit**

```bash
git add src/ui/debug_panel.rs src/ui/mod.rs
git commit -m "feat: add DebugUiState resource and F3 toggle system"
```

---

### Task 3: Implement the draw_debug_panel system — Performance and Player sections

**Files:**
- Modify: `src/ui/debug_panel.rs`

**Step 1: Add imports and the draw system skeleton**

Add to the top of `src/ui/debug_panel.rs`:

```rust
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContexts};

use crate::player::{Grounded, Player, Velocity};
use crate::registry::tile::{TerrainTiles, TileRegistry};
use crate::registry::world::WorldConfig;
use crate::world::chunk::{world_to_tile, tile_to_chunk, LoadedChunks, WorldMap};
```

**Step 2: Write the full draw_debug_panel system**

Add to `src/ui/debug_panel.rs`:

```rust
/// Draws the debug inspector panel using egui.
pub fn draw_debug_panel(
    mut contexts: EguiContexts,
    mut state: ResMut<DebugUiState>,
    // Player
    player_query: Query<(&Transform, &Velocity, &Grounded), With<Player>>,
    // Cursor
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    // World
    mut world_map: ResMut<WorldMap>,
    world_config: Res<WorldConfig>,
    terrain_tiles: Res<TerrainTiles>,
    tile_registry: Res<TileRegistry>,
    loaded_chunks: Res<LoadedChunks>,
    // Performance
    diagnostics: Res<DiagnosticsStore>,
    entities: Query<Entity>,
) -> Result {
    if !state.visible {
        return Ok(());
    }

    let ctx = contexts.ctx_mut()?;

    let panel_frame = egui::Frame::NONE
        .fill(egui::Color32::from_rgba_unmultiplied(20, 20, 30, 200))
        .inner_margin(egui::Margin::same(8))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)));

    egui::SidePanel::right("debug_panel")
        .default_width(280.0)
        .resizable(false)
        .frame(panel_frame)
        .show(ctx, |ui| {
            ui.heading("Debug Panel");
            ui.separator();

            // --- Performance ---
            let id = ui.make_persistent_id("perf_section");
            egui::collapsing_header::CollapsingState::load_with_default_open(ctx, id, state.show_performance)
                .show_header(ui, |ui| {
                    ui.strong("Performance");
                })
                .body(|ui| {
                    egui::Grid::new("perf_grid")
                        .num_columns(2)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("FPS:");
                            let fps_text = diagnostics
                                .get(&FrameTimeDiagnosticsPlugin::FPS)
                                .and_then(|d| d.smoothed())
                                .map(|v| format!("{v:.1}"))
                                .unwrap_or_else(|| "...".to_string());
                            ui.colored_label(egui::Color32::LIGHT_GREEN, &fps_text);
                            ui.end_row();

                            ui.label("Entities:");
                            ui.label(format!("{}", entities.iter().count()));
                            ui.end_row();
                        });
                });

            // --- Player ---
            let id = ui.make_persistent_id("player_section");
            egui::collapsing_header::CollapsingState::load_with_default_open(ctx, id, state.show_player)
                .show_header(ui, |ui| {
                    ui.strong("Player");
                })
                .body(|ui| {
                    if let Ok((transform, velocity, grounded)) = player_query.single() {
                        let px = transform.translation.x;
                        let py = transform.translation.y;
                        let (tx, ty) = world_to_tile(px, py, world_config.tile_size);
                        let (cx, cy) = tile_to_chunk(tx, ty, world_config.chunk_size);

                        egui::Grid::new("player_grid")
                            .num_columns(2)
                            .spacing([20.0, 4.0])
                            .show(ui, |ui| {
                                ui.label("Position:");
                                ui.monospace(format!("{px:.1}, {py:.1}"));
                                ui.end_row();

                                ui.label("Tile:");
                                ui.monospace(format!("{tx}, {ty}"));
                                ui.end_row();

                                ui.label("Velocity:");
                                ui.monospace(format!("{:.1}, {:.1}", velocity.x, velocity.y));
                                ui.end_row();

                                ui.label("Grounded:");
                                ui.label(if grounded.0 { "true" } else { "false" });
                                ui.end_row();

                                ui.label("Chunk:");
                                ui.monospace(format!("{cx}, {cy}"));
                                ui.end_row();
                            });
                    } else {
                        ui.label("No player entity");
                    }
                });

            // --- Cursor ---
            let id = ui.make_persistent_id("cursor_section");
            egui::collapsing_header::CollapsingState::load_with_default_open(ctx, id, state.show_cursor)
                .show_header(ui, |ui| {
                    ui.strong("Cursor");
                })
                .body(|ui| {
                    let cursor_info = (|| {
                        let window = windows.single().ok()?;
                        let cursor_pos = window.cursor_position()?;
                        let (camera, camera_gt) = camera_query.single().ok()?;
                        let world_pos = camera.viewport_to_world_2d(camera_gt, cursor_pos).ok()?;
                        Some(world_pos)
                    })();

                    if let Some(world_pos) = cursor_info {
                        let (tx, ty) = world_to_tile(world_pos.x, world_pos.y, world_config.tile_size);
                        let wrapped_tx = world_config.wrap_tile_x(tx);
                        let (cx, cy) = tile_to_chunk(wrapped_tx, ty, world_config.chunk_size);

                        // Get tile info
                        let tile_id = world_map.get_tile(tx, ty, &world_config, &terrain_tiles);
                        let tile_def = tile_registry.get(tile_id);

                        egui::Grid::new("cursor_grid")
                            .num_columns(2)
                            .spacing([20.0, 4.0])
                            .show(ui, |ui| {
                                ui.label("World:");
                                ui.monospace(format!("{:.1}, {:.1}", world_pos.x, world_pos.y));
                                ui.end_row();

                                ui.label("Tile:");
                                ui.monospace(format!("{tx}, {ty}"));
                                ui.end_row();

                                ui.label("Block:");
                                ui.colored_label(
                                    if tile_def.solid {
                                        egui::Color32::LIGHT_BLUE
                                    } else {
                                        egui::Color32::GRAY
                                    },
                                    &tile_def.id,
                                );
                                ui.end_row();

                                ui.label("Solid:");
                                ui.label(if tile_def.solid { "true" } else { "false" });
                                ui.end_row();

                                ui.label("Chunk:");
                                ui.monospace(format!("{cx}, {cy}"));
                                ui.end_row();
                            });
                    } else {
                        ui.label("— (cursor outside)");
                    }
                });

            // --- World ---
            let id = ui.make_persistent_id("world_section");
            egui::collapsing_header::CollapsingState::load_with_default_open(ctx, id, state.show_world)
                .show_header(ui, |ui| {
                    ui.strong("World");
                })
                .body(|ui| {
                    egui::Grid::new("world_grid")
                        .num_columns(2)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Seed:");
                            ui.monospace(format!("{}", world_config.seed));
                            ui.end_row();

                            ui.label("Size:");
                            ui.monospace(format!(
                                "{} × {} tiles",
                                world_config.width_tiles, world_config.height_tiles
                            ));
                            ui.end_row();

                            ui.label("Loaded chunks:");
                            ui.label(format!("{}", loaded_chunks.map.len()));
                            ui.end_row();
                        });
                });
        });

    Ok(())
}
```

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: compiles (system not yet registered, just needs to compile)

**Step 4: Commit**

```bash
git add src/ui/debug_panel.rs
git commit -m "feat: implement draw_debug_panel with all four sections"
```

---

### Task 4: Wire up the new panel and remove old HUD

**Files:**
- Modify: `src/ui/mod.rs`
- Delete: `src/ui/debug_hud.rs`

**Step 1: Update UiPlugin to use new panel, remove old HUD**

Replace `src/ui/mod.rs` entirely:

```rust
pub mod debug_panel;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

use crate::registry::AppState;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<debug_panel::DebugUiState>()
            .add_systems(
                Update,
                debug_panel::toggle_debug_panel.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                debug_panel::draw_debug_panel.run_if(in_state(AppState::InGame)),
            );
    }
}
```

**Step 2: Delete old debug_hud.rs**

```bash
rm src/ui/debug_hud.rs
```

**Step 3: Verify it compiles and runs**

Run: `cargo build`
Expected: compiles with no errors

Run: `cargo run`
Expected: game starts, no debug panel visible. Press F3 → panel appears on right side with all four sections. Press F3 again → panel hides.

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: replace debug HUD with egui inspector panel"
```

---

### Task 5: Polish and verify

**Step 1: Manual testing checklist**

Run: `cargo run`

Verify:
- [ ] F3 toggles panel on/off
- [ ] Performance section shows FPS (non-zero) and entity count
- [ ] Player section shows position, tile, velocity, grounded, chunk
- [ ] Player values update as you move (WASD/Space)
- [ ] Cursor section shows world pos, tile, block name, solid, chunk
- [ ] Cursor section shows "— (cursor outside)" when mouse leaves window
- [ ] Moving cursor over different blocks (air, grass, dirt, stone) shows correct names
- [ ] World section shows seed, size, loaded chunks count
- [ ] Each section collapses/expands on header click
- [ ] Panel is semi-transparent (game visible behind it)
- [ ] Breaking/placing blocks still works with panel open
- [ ] No performance drop with panel open (FPS stays ~60)

**Step 2: Fix any issues found during testing**

If any issues found, fix and re-test.

**Step 3: Final commit (if fixes were needed)**

```bash
git add -A
git commit -m "fix: polish debug panel after manual testing"
```
