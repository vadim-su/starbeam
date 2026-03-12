# Snow & Weather System Design

## Overview

A weather system that adds snowfall as both a biome feature (snowy biomes with permanent snow) and a dynamic weather event (temperature-driven snowfall on any biome). Purely cosmetic — no gameplay impact on tiles or physics.

## Module Structure

New module: `src/weather/`

| File | Responsibility |
|---|---|
| `mod.rs` | `WeatherPlugin`, system registration, runs in `GameSet::WorldUpdate` + `AppState::InGame` |
| `wind.rs` | `Wind` resource, smooth direction/strength changes over time |
| `snow_particles.rs` | Dedicated snow particle pool, spawning, rendering |
| `snow_overlay.rs` | Snow cap overlays on exposed surface blocks |
| `weather_state.rs` | `WeatherState` resource, timers, temperature-based probability |

## Wind System (`wind.rs`)

### `Wind` Resource
- `direction: f32` — angle in radians
- `strength: f32` — 0.0 to 1.0
- `target_direction: f32` — lerp target
- `target_strength: f32` — lerp target
- `change_timer: Timer` — 5-15 seconds between target changes

### `update_wind()` System
- On timer expiry: pick new `target_direction` (±60° from current) and `target_strength`
- Each frame: lerp `direction` and `strength` toward targets with small coefficient (~0.02)
- Result: wind slowly drifts, creating natural feel

### Wind Effect on Particles
- Added to each particle's velocity:
  - X: `cos(direction) * strength * MAX_WIND_SPEED`
  - Y: `sin(direction) * strength * MAX_WIND_SPEED * 0.3` (horizontal dominates)

## Snow Particle Pool (`snow_particles.rs`)

### `SnowParticlePool` Resource
- Capacity: 1500 particles (separate from game `ParticlePool`)
- Same ring-buffer architecture as existing `ParticlePool`

### Particle Properties
- **Size:** random 1-4px
- **Fall speed:** inversely proportional to size (large=40-80 px/s, small=80-120 px/s) — depth illusion
- **Color:** white, alpha variation 0.6-1.0, larger flakes slightly more transparent
- **Gravity scale:** 0.1-0.3 (light, floaty)
- **Wobble:** per-particle sine wave on X axis (random phase, small amplitude) layered on top of wind
- **Lifetime:** dies when below camera bottom or after 8-12 seconds

### Spawning
- Spawn zone: strip above camera top (camera.top + 16..48px), full camera width
- Spawn rate: 30-80 particles/sec, scaled by `Snowing.intensity`
- Only active when `WeatherState` is `Snowing`

### Rendering
- Separate batched mesh (same technique as game particles)
- Z = 5.0 (above tiles and objects, below UI)
- Per-vertex colors with alpha

## Snow Overlay (`snow_overlay.rs`)

### `SnowOverlay` Component
- `tile_x: i32`, `tile_y: i32`

### `SnowOverlayTexture` Resource
- Single procedurally generated 16x4px white texture with irregular bottom edge (pixel art style)
- Created once at plugin startup

### Overlay Placement
- Sprite positioned at block position + ~6px vertical offset
- Z = 0.1 (just above tile layer)

### `update_snow_overlays()` System
- Runs on 0.5 second timer (not every frame)
- Iterates loaded chunks in view
- Candidate: tile is solid AND tile above is air
- **Snowy biome (`snow_permanent: true`):** overlays spawned on chunk load, never removed
- **Weather snow:** overlays appear gradually (random chance per tick while snowing)
- **Melting:** when `temperature_modifier` is high enough (daytime in non-permanent biomes), overlays removed gradually (random chance per tick)
- On tile destruction: remove overlay from destroyed block, recheck neighbors

## Weather State (`weather_state.rs`)

### `WeatherKind` Enum
```rust
enum WeatherKind {
    Clear,
    Snowing {
        intensity: f32,   // 0.0-1.0, affects spawn rate
        elapsed: f32,     // seconds since start
        duration: f32,    // total duration (30-120 sec)
    },
}
```

### `WeatherState` Resource
- `current: WeatherKind`
- `cooldown_timer: Timer` — 60-180 seconds between snowfalls
- `check_timer: Timer` — ~5 seconds between probability rolls

### `update_weather()` System
- Every `check_timer` tick, roll for snowfall:
  - Probability: `biome.snow_base_chance * (1.0 - world_time.temperature_modifier)`
  - Higher chance = colder temperature, snowy biomes
- On success: transition to `Snowing` with random duration 30-120s and intensity 0.5-1.0
- When `elapsed >= duration`: transition to `Clear`, start cooldown
- Biome determined by camera position

## Biome Integration

### BiomeDef Extension
Two new fields with defaults:
- `snow_base_chance: f32` — default `0.0` (no snow for existing biomes)
- `snow_permanent: bool` — default `false`

### New Biome: Tundra
```ron
(
    id: "tundra",
    surface_block: "snow_dirt",
    subsurface_block: "frozen_dirt",
    subsurface_depth: 4,
    fill_block: "stone",
    cave_threshold: 0.35,
    snow_base_chance: 0.8,
    snow_permanent: true,
    parallax: Some("content/biomes/tundra/tundra.parallax.ron"),
)
```

### New Tiles
- **`snow_dirt`** — white/snowy top layer, dirt bottom (autotile variant needed)
- **`frozen_dirt`** — grey-blue frozen earth

### Tundra Parallax
- Pale blue/grey sky
- Snowy hills (far/near layers)
- Placeholder art initially, proper art as separate task

## Implementation Order
1. `Wind` resource and system
2. `SnowParticlePool` — pool, spawning, rendering
3. `WeatherState` — state machine, temperature integration
4. `SnowOverlay` — cap sprites, placement logic
5. `BiomeDef` extension — `snow_base_chance`, `snow_permanent` fields
6. Tundra biome — definition, placeholder tiles, parallax
7. Polish — tuning constants, visual feel
