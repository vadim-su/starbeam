# Parallax Background — Design

## Overview

Decorative, non-editable parallax background with configurable layers. Starbound-style: multiple horizontal strips (sky, distant mountains, near hills, etc.) each scrolling at different speeds relative to camera movement. Purely visual — no collision, no player interaction.

## Decisions

- **Style**: Starbound (multiple layers, depth effect)
- **Layer count**: Configurable via RON, any number
- **Scroll**: Both axes (X and Y), independent speed factors per layer per axis
- **Repeat**: Per-layer `repeat_x` / `repeat_y` flags in RON
- **World wrap**: Independent — layers tile infinitely by their own formula, not synced with world width
- **Assets**: User-provided PNGs, no auto-generation

## Configuration

File: `assets/data/parallax.ron`

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
  ],
)
```

Fields per layer:
- `name` — identifier (for debug panel)
- `image` — path relative to `assets/`
- `speed_x`, `speed_y` — camera movement multiplier (0.0 = static, 1.0 = moves with world)
- `repeat_x`, `repeat_y` — tile texture along axis
- `z_order` — draw order (negative = behind tilemap)

## Architecture

### Module structure

```
src/parallax/
  mod.rs       — ParallaxPlugin, system registration
  config.rs    — ParallaxConfig, ParallaxLayerDef (RON deserialization)
  spawn.rs     — spawn_parallax_layers (OnEnter(InGame))
  scroll.rs    — parallax_scroll (Update, after camera_follow_player)
```

### Components & Resources

- `ParallaxConfig` (Resource/Asset) — loaded via `registry` pipeline, hot-reloadable
- `ParallaxLayer` (Component) — marker on layer entity, stores speed_x, speed_y, repeat flags
- Each layer = one or more `Sprite` entities (multiple copies for repeat-tiling)

### Scroll logic

```
offset_x = camera_x * layer.speed_x
offset_y = camera_y * layer.speed_y

For repeat layers: offset = offset % texture_size
Position sprite copies to cover visible area
```

### Repeat tiling

For `repeat_x: true` / `repeat_y: true`:
1. Get visible area: `window_size * projection_scale`
2. Calculate copies needed: `ceil(visible_size / texture_size) + 1` (extra for scroll margin)
3. Each frame: `base_offset = (camera_pos * speed) % texture_size`, position copies at `base_offset + i * texture_size` relative to camera

For `repeat: false`: single sprite, position = `camera_pos * speed_factor`.

### Z-ordering

- Parallax: z from -100 to -50 (configured per layer in RON)
- Tilemap: z ~ 0
- Player: z ~ 10
- UI: egui overlay

### Loading & Hot-reload

Same pattern as `WorldConfig` / `PlayerConfig`:
1. `AppState::Loading` — load `parallax.ron` + all referenced textures
2. `OnEnter(InGame)` — `spawn_parallax_layers`
3. `Update` — `parallax_scroll` (after `camera_follow_player`)
4. Hot-reload: despawn all `ParallaxLayer` entities, respawn from new config

### Integration

- Plugin registered in `main.rs` as `parallax::ParallaxPlugin`
- Config loaded through existing `registry` asset pipeline
- Scroll system ordered after `camera_follow_player`
