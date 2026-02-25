# Debug Panel Design

## Overview

Replace the minimal single-line debug HUD with a full inspector-style panel using `bevy_egui`. The panel overlays the right side of the screen, toggled by F3, with collapsible sections showing player, cursor, world, and performance info.

## Approach

**bevy_egui** — immediate-mode GUI via egui, the de-facto standard for Bevy debug UI. Provides `CollapsingHeader`, semi-transparent panels, and scrolling out of the box. ~20 lines per section.

## Architecture

### New dependency

`bevy_egui` in `Cargo.toml`.

### Resource

```rust
#[derive(Resource)]
struct DebugUiState {
    visible: bool,           // F3 toggle, default: false
    show_player: bool,       // section collapsed state
    show_cursor: bool,
    show_world: bool,
    show_performance: bool,
}
```

### File changes

- **Delete** `src/ui/debug_hud.rs` (old HUD)
- **Create** `src/ui/debug_panel.rs` (new panel)
- **Update** `src/ui/mod.rs` (register new systems)
- **Update** `main.rs` (add `FrameTimeDiagnosticsPlugin`)

### Systems

| System | Schedule | Purpose |
|--------|----------|---------|
| `toggle_debug_panel` | `Update` | Listen F3, flip `visible` |
| `draw_debug_panel` | `Update` | egui immediate-mode draw (only if `visible`) |

## Panel Layout

- `egui::SidePanel::right("debug_panel")`, width ~280px, not resizable
- Semi-transparent dark background (alpha ~200/255)
- Overlay — does not shrink the game viewport
- Hidden by default, F3 to toggle

```
┌─────────────────────────┐
│ Debug Panel              │
├─────────────────────────┤
│ ▼ Performance           │
│   FPS: 60.0             │
│   Entities: 1284        │
├─────────────────────────┤
│ ▼ Player                │
│   Position: 320, 640    │
│   Tile: 10, 20          │
│   Velocity: 0.0, -2.1   │
│   Grounded: true        │
│   Chunk: 1, 2           │
├─────────────────────────┤
│ ▼ Cursor                │
│   World: 450.5, 320.2   │
│   Tile: 14, 10          │
│   Block: Stone           │
│   Solid: true           │
│   Chunk: 1, 1           │
├─────────────────────────┤
│ ▼ World                 │
│   Seed: 42              │
│   Size: 2048 x 512      │
│   Loaded chunks: 12     │
└─────────────────────────┘
```

Each section is an `egui::CollapsingHeader`, default open. Collapsed state stored in `DebugUiState`.

## Data Sources

### Performance

| Field | Source |
|-------|--------|
| FPS | `Res<DiagnosticsStore>` → `FrameTimeDiagnosticsPlugin` → `fps().smoothed()` |
| Entities | `Query<Entity>` → `.iter().count()` |

### Player

| Field | Source |
|-------|--------|
| Position (px) | `Query<&Transform, With<Player>>` → `translation.x, y` |
| Tile | `(px / TILE_SIZE).floor() as i32` |
| Velocity | `Query<&Velocity, With<Player>>` → `x, y` |
| Grounded | `Query<&Grounded, With<Player>>` → `.0` |
| Chunk | `tile / CHUNK_SIZE` |

### Cursor

| Field | Source |
|-------|--------|
| World pos | `window.cursor_position()` → `camera.viewport_to_world_2d()` |
| Tile | `(world / TILE_SIZE).floor() as i32` |
| Block type | `WorldMap.get_tile(tx, ty)` → `TileRegistry.get(tile_id)` → `name` |
| Solid | `TileDef.solid` |
| Chunk | `tile / CHUNK_SIZE` |

### World

| Field | Source |
|-------|--------|
| Seed | `Res<WorldMap>` → `seed` |
| Size | `Res<WorldConfig>` → `width, height` |
| Loaded chunks | `Res<LoadedChunks>` → `map.len()` |

## Edge Cases

| Case | Handling |
|------|----------|
| Cursor outside window | `cursor_position()` → `None` → show `"— (cursor outside)"` |
| Tile out of bounds | `WorldMap.get_tile()` → `None` → Block: `"Out of bounds"`, Solid: `"—"` |
| No player entity | Player query empty → show `"No player entity"` |
| FPS not ready | `smoothed()` → `None` on first frames → show `"..."` |
| Wrap-around coords | Pass cursor tile through `wrap_tile_x()` before `WorldMap` lookup |
