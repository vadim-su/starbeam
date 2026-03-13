# Weather System Overhaul Design

## Overview

Complete rework of the weather module to support multiple precipitation types (rain, snow, fog, sandstorm) with a planetary weather model and temperature-based type resolution.

## Requirements

- Planetary-global weather cycle (clear -> precipitation -> clear), like Starbound
- Precipitation type determined by local temperature at render time
- Temperature model: `planet.base_temperature + day_night_celsius_offset + biome.temperature_offset`
- Allowed weather types configured per Planet Type RON with temperature thresholds
- 4 weather types: snow, rain, fog, sandstorm
- Rain: Terraria style -- long diagonal streaks + splash on surface impact
- Fog: semi-transparent screen overlay + drifting cloud sprites
- Sandstorm: horizontal particles driven by wind
- Snow: existing overlay system (tiles + trees) preserved
- Visual only -- no gameplay effects
- No environmental changes (sky color, sounds) -- separate future task

## Temperature Model

### Formula

```
local_temp = planet.base_temperature
           + day_night_celsius_offset
           + biome.temperature_offset
```

### Data Sources

- `planet.base_temperature`: new field in `PlanetTypeAsset` (e.g. garden = 15.0). Units: degrees Celsius-like. Star zone temperature (`hot`/`warm`/`cold`) can inform procedural defaults.
- `day_night_celsius_offset`: **new field** in `DayNightConfig` and `WorldTime`. The existing `temperature_modifiers` field (normalized 0-1 scale, values like `[-0.2, 0.0, -0.05, -0.1]`) is kept for backward compatibility. A new `temperature_celsius_offsets: [f32; 4]` field is added, representing per-phase offsets in degrees (e.g. `[-5.0, 0.0, -3.0, -10.0]`). `WorldTime` gets a new `temperature_celsius_offset: f32` field, lerped per phase just like the existing modifier.
- `biome.temperature_offset`: new field in `BiomeAsset` (e.g. tundra = -20.0, forest = -3.0, desert = +15.0)

### Backward Compatibility

The existing `WorldTime.temperature_modifier` (0-1 scale) remains unchanged and continues to work for any non-weather consumers. The new `temperature_celsius_offset` is an independent, parallel field. The weather system uses only the new Celsius-based field.

### Existing Consumers of temperature_modifier

These must be migrated to use the new Celsius-based temperature:

1. `snow_overlay.rs` line 153: melting check `temperature_modifier > 0.5` -- replace with `local_temp > 2.0`
2. `weather_state.rs` line 98: snow probability `biome.snow_base_chance * (1.0 - temperature_modifier)` -- entire function is rewritten; this code is removed

### Temperature Ranges by Type (defaults)

| Type | temp_min | temp_max |
|------|----------|----------|
| Snow | -inf | 0.0 |
| Rain | 0.0 | 35.0 |
| Fog | 5.0 | 20.0 |
| Sandstorm | 30.0 | +inf |

Ranges are configurable per Planet Type in RON. When `temp_min` is omitted in RON, it defaults to `f32::NEG_INFINITY`. When `temp_max` is omitted, it defaults to `f32::INFINITY`. These defaults are implemented via `#[serde(default)]` with custom default functions.

### Worked Example: Tundra Temperature Range

Garden planet: `base_temperature = 15.0`
Tundra biome: `temperature_offset = -20.0`
Day/night offsets: Dawn = -3.0, Day = 0.0, Sunset = -3.0, Night = -8.0

| Phase | Calculation | local_temp | Precipitation |
|-------|-------------|------------|---------------|
| Dawn | 15 + (-3) + (-20) | -8.0 | Snow |
| Day | 15 + 0 + (-20) | -5.0 | Snow |
| Sunset | 15 + (-3) + (-20) | -8.0 | Snow |
| Night | 15 + (-8) + (-20) | -13.0 | Snow |

Tundra always below 0 -> permanent snow. Confirmed.

### Worked Example: Forest Temperature Range

Garden planet: `base_temperature = 15.0`
Forest biome: `temperature_offset = -3.0`
Day/night offsets: Dawn = -3.0, Day = 0.0, Sunset = -3.0, Night = -8.0

| Phase | Calculation | local_temp | Precipitation |
|-------|-------------|------------|---------------|
| Dawn | 15 + (-3) + (-3) | 9.0 | Rain or Fog |
| Day | 15 + 0 + (-3) | 12.0 | Rain or Fog |
| Sunset | 15 + (-3) + (-3) | 9.0 | Rain or Fog |
| Night | 15 + (-8) + (-3) | 4.0 | Rain |

Forest gets rain/fog during the day, rain at night. No snow on a garden planet.

## Planetary WeatherState

### Structure

```rust
pub struct WeatherState {
    pub phase: WeatherPhase,
    pub intensity: f32,           // 0.0..1.0, smooth ramp
    pub target_intensity: f32,
    pub duration: f32,
    pub elapsed: f32,
    pub cooldown: f32,
    pub check_timer: f32,
}

pub enum WeatherPhase {
    Clear,
    Precipitation,
}
```

Key change: `WeatherState` does NOT know the precipitation type. It only tracks "precipitation is happening at intensity X". The actual type is resolved at render time based on local temperature.

### Transition Logic

- **Clear phase**: every 5 seconds, roll `precipitation_chance` from planet config
  - On success: transition to `Precipitation`, randomize duration from config range, set target_intensity 0.5-1.0
  - `precipitation_chance` is planet-global (not per-biome). This is a deliberate change from the current system where each biome has its own `snow_base_chance`. All biomes on a planet share the same precipitation frequency; the biome only affects the **type** via temperature.
- **Precipitation phase**: ramp intensity toward target at `RAMP_SPEED` (0.2/sec)
  - When `elapsed >= duration`: set target_intensity to 0.0 (ramp down)
  - When intensity reaches 0.0: transition to Clear, set cooldown from config range

### Planet Config

```ron
// garden.planet.ron
(
    id: "garden",
    base_temperature: 15.0,
    weather: Some((
        precipitation_chance: 0.3,
        precipitation_duration: (60, 180),
        cooldown: (60, 300),
        types: [
            (kind: "snow",      temp_max: 0.0),
            (kind: "rain",      temp_min: 0.0, temp_max: 35.0),
            (kind: "fog",       temp_min: 5.0, temp_max: 20.0),
        ],
    )),
)
```

### Type Resolution When Ranges Overlap

When multiple weather types match the current temperature (e.g. Rain and Fog both match at 10C), one is selected randomly with equal weight. The selection happens once when precipitation starts and is cached for the duration of the event per-biome-region. This prevents flickering between types.

However, when the player moves between biomes with different temperatures, the resolved type may change. In this case, old particles live out their lifetime and new particles spawn with the new config -- smooth transition, no abrupt switch.

## Unified Particle System

### WeatherParticleConfig

Each precipitation type defines particle parameters:

```rust
pub struct WeatherParticleConfig {
    // Movement
    pub fall_speed: (f32, f32),       // min/max fall speed (px/sec)
    pub wind_influence: f32,          // 0.0..1.0
    pub angle: f32,                   // base angle in degrees (0 = straight down, 90 = horizontal)

    // Visual
    pub color: (u8, u8, u8, u8),      // RGBA
    pub size: (f32, f32),             // min/max size (px)
    pub length: (f32, f32),           // streak length (rain: long, snow: ~size)
    pub wobble: bool,                 // sinusoidal wobble (snow only)

    // Spawn
    pub spawn_rate: (f32, f32),       // particles/sec at intensity 0..1
    pub lifetime: (f32, f32),         // seconds

    // Collision
    pub splash: bool,                 // spawn splash on surface hit
}
```

### Default Configs (hardcoded)

| Parameter | Snow | Rain | Sandstorm |
|-----------|------|------|-----------|
| fall_speed | 80-120 | 300-500 | 40-80 |
| wind_influence | 0.8 | 0.3 | 1.0 |
| angle | 0 | 5-15 (from wind) | 80-90 |
| color | (240,245,255,230) | (140,170,220,180) | (210,180,120,160) |
| size | 1-4 | 1-2 | 1-3 |
| length | =size | 8-16 | 2-4 |
| wobble | yes | no | no |
| spawn_rate | 30-80 | 60-150 | 100-200 |
| lifetime | 6-12 | 3-6 | 4-8 |
| splash | no | yes | no |

Rain angle formula: `base_angle + wind.direction.x.signum() * wind.strength * 10.0` degrees. Wind pushes rain diagonally; at max wind the angle reaches ~15 degrees from vertical.

### Unified Particle Pool

```rust
pub struct WeatherParticle {
    pub position: Vec2,
    pub velocity: Vec2,
    pub lifetime: f32,
    pub age: f32,
    pub size: f32,
    pub length: f32,
    pub color: [f32; 4],
    pub alive: bool,
    pub wobble_phase: f32,
    pub wobble_speed: f32,
    pub wobble_amplitude: f32,
}
```

Single `WeatherParticlePool` with capacity 2500. Ring-buffer allocation (same O(1) amortized strategy as current snow pool).

Capacity sizing: worst case is sandstorm at full intensity -- 200 particles/sec * 8 sec lifetime = 1600 alive. 2500 provides headroom for transitions between types where old and new particles coexist.

### Rendering

Single mesh with oriented rectangles along velocity vector. Snow particles render as squares (length ~ size), rain as elongated streaks along the fall direction, sandstorm as small horizontal rectangles.

All particles are rendered in the same mesh pass at `WEATHER_Z = 3.0` (same as current snow).

### Splash System

Separate small pool `SplashPool` (~200 entries). When a rain particle hits a solid tile, spawn a splash:

```rust
pub struct SplashParticle {
    pub position: Vec2,
    pub velocity: Vec2,       // outward scatter
    pub lifetime: f32,        // very short: 0.1-0.2 sec
    pub age: f32,
    pub size: f32,            // 1px
    pub color: [f32; 4],      // same as rain but lower alpha
    pub alive: bool,
}
```

Each splash spawns 2-3 tiny particles scattering outward and upward. Rendered in the same mesh pass as weather particles. Splash particles use the same `WeatherParticle` struct with short lifetime and small size -- no separate struct needed.

## Fog System

Separate from particle system. Fog does NOT use `WeatherParticleConfig` or the particle pool.

### Two Layers

1. **Overlay**: fullscreen semi-transparent white/gray sprite. Opacity = `intensity * 0.3` max. Z above tiles, below UI. Single entity with `Sprite`.

2. **Fog clouds**: 5-10 large semi-transparent sprites (procedural 64x32px texture). Drift along wind direction. Alpha pulses sinusoidally. Respawn when leaving camera view.

### Fog Cloud Texture

Procedurally generated 64x32 image. Radial gradient from center: alpha = `max_alpha * (1.0 - (distance_from_center / radius).powi(2))`, clamped to 0. Color: white `(255, 255, 255)`. Max alpha ~80. This produces a soft, round cloud shape with smooth falloff. Generated once at init, like `generate_snow_cap_image()`.

### Components

```rust
pub struct FogOverlay;

pub struct FogCloud {
    pub drift_speed: f32,
    pub alpha_phase: f32,
}
```

### Systems

- `update_fog_overlay` -- sets overlay alpha based on weather intensity when resolved type = fog. When resolved type is NOT fog, alpha is set to 0. Smooth lerp for transitions.
- `update_fog_clouds` -- moves clouds along wind, pulses alpha, respawns off-screen. Clouds are hidden (alpha=0) when fog is not active.

### Data Flow Integration

In the main data flow (step 2), when the resolved weather type is fog: skip particle spawning entirely, activate fog systems instead. The particle spawn system checks `resolved_type != Fog` before spawning.

## Snow Overlay Adaptation

Existing `snow_overlay.rs` preserved with trigger changes:

- Instead of `weather.is_snowing()`, check: `local_temp < 0.0 && weather.phase == Precipitation`
- Snow permanent behavior: instead of `biome.snow_permanent`, derive from temperature. If `local_temp` at a tile never rises above 0.0 across all day phases, snow is permanent there. In practice, check current `local_temp < 0.0` -- if temperature is always below 0, snow accumulates and never melts.
- Melting condition: `local_temp > 2.0 && weather.phase != Precipitation` (or precipitation type at this location is not snow). This means rain in an adjacent warm biome does NOT melt snow in a cold biome -- each tile's melting is based on its own local temperature.
- Tree snow caps: same logic, same trigger change
- Biome boundary falloff: preserved as-is

## Biome RON Changes

Remove `snow_base_chance` and `snow_permanent`, add `temperature_offset`:

```ron
// tundra.biome.ron
(
    id: "tundra",
    temperature_offset: -20.0,
    // snow_base_chance and snow_permanent removed from struct
)

// meadow.biome.ron
( id: "meadow", temperature_offset: 0.0, ... )

// forest.biome.ron
( id: "forest", temperature_offset: -3.0, ... )
```

### Migration

- Remove `snow_base_chance` and `snow_permanent` fields from `BiomeDef` struct and `BiomeAsset` struct
- Remove these fields from all biome RON files (tundra, meadow, forest)
- Add `temperature_offset: f32` with `#[serde(default)]` (defaults to 0.0) to `BiomeAsset`
- Add `temperature_offset` to all biome RON files
- Old RON files with removed fields will still parse due to `#[serde(default)]` on the Rust side -- RON parsing ignores unknown fields. But the old fields should be removed from RON files in the same PR for cleanliness.

## DayNightConfig Changes

### New Fields

Add to `DayNightConfig`:
```rust
pub temperature_celsius_offsets: [f32; 4],  // [dawn, day, sunset, night] in degrees
```

Add to `WorldTime`:
```rust
pub temperature_celsius_offset: f32,  // lerped per phase, same as temperature_modifier
```

### Generation

In `generation.rs`, generate `temperature_celsius_offsets` from star zone / orbit:
- Hot zone: `[25.0, 35.0, 25.0, 15.0]`
- Warm zone: `[-3.0, 0.0, -3.0, -8.0]`
- Cold zone: `[-10.0, -5.0, -10.0, -20.0]`

These are added to `planet.base_temperature + biome.temperature_offset` to get the final local temperature.

### PlanetTypeAsset

Add optional field:
```rust
pub temperature_celsius_offsets: Option<[f32; 4]>,
```

If provided in RON, overrides the generated defaults. Same pattern as existing `temperature_modifiers`.

## Data Flow Per Frame

```
1. WeatherState ticks globally
   |-- Clear: every 5 sec roll precipitation_chance
   +-- Precipitation: ramp intensity, track duration

2. Resolve weather type (every frame):
   |-- Get camera position
   |-- Determine biome under camera -> temperature_offset
   |-- local_temp = base_temp + day_night_celsius_offset + biome_offset
   |-- Filter planet.weather.types by local_temp
   |-- If multiple match: random selection (cached per event)
   +-- Result: resolved type (Snow/Rain/Sandstorm/Fog/None)

3. Particle rendering (every frame, if resolved type != Fog):
   |-- Get WeatherParticleConfig for resolved type
   |-- Spawn new particles based on intensity and config
   |-- Update existing particles (movement, wind, wobble, collision)
   |-- On collision with solid tile: if splash=true, spawn splash
   +-- Rebuild mesh from alive particles + splashes

4. Fog (every frame, if resolved type == Fog):
   |-- Set overlay alpha = intensity * 0.3
   +-- Move clouds along wind, pulse alpha

5. Snow overlay (every 0.5 sec):
   |-- For each visible tile:
   |   |-- Compute local_temp at tile position
   |   |-- If local_temp < 0 and precipitating -> add snow overlay
   |   +-- If local_temp > 2 and (not precipitating or resolved type != snow) -> melt
   +-- Same for tree snow caps
```

## Biome Transition

When the player moves between biomes, temperature changes and precipitation type may switch (snow -> rain). Transition is smooth: old particles live out their lifetime, new particles spawn with new config. No abrupt switching.

## File Structure After Refactor

```
src/weather/
  mod.rs              -- plugin, system registration
  weather_state.rs    -- planetary WeatherState (Clear/Precipitation phases)
  wind.rs             -- unchanged
  temperature.rs      -- local temperature calculation
  precipitation.rs    -- WeatherParticleConfig definitions, type resolution by temperature
  particles.rs        -- unified pool, spawn, physics, render, splash
  fog.rs              -- overlay + cloud sprites
  snow_overlay.rs     -- tile/tree snow overlays (adapted triggers)
```

## Removed Files

- `snow_particles.rs` -- replaced by `particles.rs`

## Renamed Resources

- `SnowParticlePool` -> `WeatherParticlePool`
- `SharedSnowMaterial` -> `WeatherParticleMaterial`
- `SnowMeshEntity` -> `WeatherMeshEntity`

## Out of Scope

- Gameplay effects (damage, slippery surfaces, crop watering)
- Environmental visual changes (sky darkening, color tinting, sound)
- Seasons / long-term climate cycles
- Lightning / thunder effects
