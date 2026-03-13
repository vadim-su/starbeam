# Weather System Overhaul Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the snow-only weather system with a unified multi-weather system (rain, snow, fog, sandstorm) driven by planetary temperature and configurable per planet type.

**Architecture:** Planetary WeatherState cycles Clear/Precipitation globally. Local temperature (planet base + day/night offset + biome offset) determines precipitation type at render time. A unified particle pool renders all precipitation types. Fog uses a separate overlay+cloud system.

**Tech Stack:** Rust, Bevy 0.18, RON config files, procedural textures

**Spec:** `docs/superpowers/specs/2026-03-13-weather-system-overhaul-design.md`

---

## Chunk 1: Temperature Model & Data Layer

### Task 1: Add temperature_offset to BiomeDef and BiomeAsset

**Files:**
- Modify: `src/registry/assets.rs:291-320` (BiomeAsset struct)
- Modify: `src/registry/biome.rs:19-32` (BiomeDef struct)
- Modify: `src/registry/loading.rs:718-723` (BiomeAsset -> BiomeDef conversion)
- Modify: `src/registry/hot_reload.rs:158-164` (hot reload conversion)
- Modify: `src/cosmos/ship_hull.rs:254-259` (ship hull biome def)
- Modify: `src/test_helpers.rs:52-78` (test biome registry)
- Modify: `src/registry/biome.rs:168-268` (existing tests that create BiomeDef)
- Modify: `assets/content/biomes/meadow/meadow.biome.ron`
- Modify: `assets/content/biomes/forest/forest.biome.ron`
- Modify: `assets/content/biomes/tundra/tundra.biome.ron`

- [ ] **Step 1: Add `temperature_offset` field to BiomeDef**

In `src/registry/biome.rs`, add to `BiomeDef`:
```rust
pub temperature_offset: f32,
```

- [ ] **Step 2: Add `temperature_offset` field to BiomeAsset**

In `src/registry/assets.rs`, add to `BiomeAsset`:
```rust
#[serde(default)]
pub temperature_offset: f32,
```

- [ ] **Step 3: Wire up conversion and all BiomeDef construction sites**

In `src/registry/loading.rs` (~line 721), add to BiomeDef construction:
```rust
temperature_offset: asset.temperature_offset,
```

In `src/registry/hot_reload.rs` (~line 161), add to BiomeDef construction:
```rust
temperature_offset: asset.temperature_offset,
```

In `src/cosmos/ship_hull.rs` (~line 257), add to BiomeDef construction:
```rust
temperature_offset: 0.0,
```

In `src/test_helpers.rs` `test_biome_registry()`, add to EVERY BiomeDef in the loop:
```rust
temperature_offset: 0.0,
```

In `src/registry/biome.rs` tests (`biome_registry_insert_and_get` and `biome_registry_insert_updates_existing`), add to every BiomeDef construction:
```rust
temperature_offset: 0.0,
```

- [ ] **Step 4: Update biome RON files**

`assets/content/biomes/tundra/tundra.biome.ron` -- add `temperature_offset: -20.0,`
`assets/content/biomes/forest/forest.biome.ron` -- add `temperature_offset: -3.0,`
`assets/content/biomes/meadow/meadow.biome.ron` -- add `temperature_offset: 0.0,`

Other biome RON files don't need the field -- `#[serde(default)]` gives 0.0.

- [ ] **Step 5: Run tests**

Run: `cargo test`
Expected: all 305+ tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/registry/assets.rs src/registry/biome.rs src/registry/loading.rs \
       src/registry/hot_reload.rs src/cosmos/ship_hull.rs src/test_helpers.rs \
       assets/content/biomes/
git commit -m "feat(weather): add temperature_offset field to BiomeDef and BiomeAsset"
```

---

### Task 2: Add temperature_celsius_offsets to DayNightConfig and WorldTime

**Files:**
- Modify: `src/world/day_night.rs:44-56` (DayNightConfig struct)
- Modify: `src/world/day_night.rs:79-108` (WorldTime struct and Default)
- Modify: `src/world/day_night.rs:164-196` (tick_world_time system)
- Modify: `src/world/day_night.rs:221-237` (WorldTime::from_config)
- Modify: `src/world/day_night.rs:247-271` (test_config)
- Modify: `src/cosmos/generation.rs:178-308` (generate_day_night)
- Modify: `src/cosmos/warp.rs:410` (ship DayNightConfig construction)
- Modify: `src/registry/loading.rs:483` (ship bootstrap DayNightConfig)
- Modify: `src/registry/assets.rs:257-289` (PlanetTypeAsset)

- [ ] **Step 1: Add field to DayNightConfig**

In `src/world/day_night.rs`, add to `DayNightConfig`:
```rust
#[serde(default)]
pub temperature_celsius_offsets: [f32; 4],
```

- [ ] **Step 2: Add field to WorldTime**

Add to `WorldTime` struct:
```rust
pub temperature_celsius_offset: f32,
```

Add to `WorldTime::default()`:
```rust
temperature_celsius_offset: 0.0,
```

- [ ] **Step 3: Update tick_world_time**

After line ~194 (`world_time.temperature_modifier = ...`), add:
```rust
world_time.temperature_celsius_offset =
    lerp_phase_value(&config.temperature_celsius_offsets, phase, progress);
```

- [ ] **Step 4: Update WorldTime::from_config**

After the existing `temperature_modifier` line (~234), add:
```rust
wt.temperature_celsius_offset =
    lerp_phase_value(&config.temperature_celsius_offsets, phase, progress);
```

- [ ] **Step 5: Add to PlanetTypeAsset**

In `src/registry/assets.rs`, add to `PlanetTypeAsset`:
```rust
#[serde(default)]
pub temperature_celsius_offsets: Option<[f32; 4]>,
```

- [ ] **Step 6: Generate temperature_celsius_offsets in generation.rs**

In `generate_day_night()` (after line ~286), add:
```rust
let temperature_celsius_offsets = planet.temperature_celsius_offsets.unwrap_or_else(|| {
    // Derive from orbit position: closer = warmer, farther = colder
    if orbit_factor < 0.3 {
        // Hot zone
        [25.0, 35.0, 25.0, 15.0]
    } else if orbit_factor > 0.7 {
        // Cold zone
        [-10.0, -5.0, -10.0, -20.0]
    } else {
        // Warm zone (default)
        [-3.0, 0.0, -3.0, -8.0]
    }
});
```

Add to the `DayNightConfig { ... }` struct literal at line ~296:
```rust
temperature_celsius_offsets,
```

- [ ] **Step 7: Update ship DayNightConfig constructions**

In `src/cosmos/warp.rs` (~line 410), find the `DayNightConfig { ... }` and add:
```rust
temperature_celsius_offsets: [0.0; 4],
```

In `src/registry/loading.rs` (~line 483), find the `DayNightConfig { ... }` and add:
```rust
temperature_celsius_offsets: [0.0; 4],
```

- [ ] **Step 8: Update test_config in day_night.rs tests**

In `src/world/day_night.rs` `test_config()` (~line 248), add:
```rust
temperature_celsius_offsets: [-3.0, 0.0, -3.0, -8.0],
```

- [ ] **Step 9: Run tests**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 10: Commit**

```bash
git add src/world/day_night.rs src/cosmos/generation.rs src/cosmos/warp.rs \
       src/registry/assets.rs src/registry/loading.rs
git commit -m "feat(weather): add Celsius temperature offsets to DayNightConfig and WorldTime"
```

---

### Task 3: Add base_temperature, weather config, and temperature calculation

**Files:**
- Modify: `src/registry/assets.rs` (WeatherConfig, WeatherTypeEntry structs + PlanetTypeAsset fields)
- Modify: `src/registry/world.rs` (ActiveWorld fields)
- Modify: `src/registry/loading.rs:463` (ActiveWorld construction -- ship bootstrap)
- Modify: `src/cosmos/warp.rs:190` (ActiveWorld construction -- warp to planet)
- Modify: `src/cosmos/warp.rs:394` (ActiveWorld construction -- warp to ship)
- Modify: `src/cosmos/ship_hull.rs:230` (ActiveWorld construction -- ship test)
- Modify: `src/test_helpers.rs:20` (test_active_world)
- Modify: `src/registry/world.rs:64` (test_config in world.rs tests, if exists)
- Create: `src/weather/temperature.rs`
- Modify: `src/weather/mod.rs`
- Modify: `assets/worlds/planet_types/garden/garden.planet.ron`

- [ ] **Step 1: Define weather config types**

In `src/registry/assets.rs`, add near `PlanetTypeAsset`:
```rust
/// Weather type entry with temperature thresholds.
#[derive(Debug, Clone, Deserialize)]
pub struct WeatherTypeEntry {
    pub kind: String,
    #[serde(default = "default_neg_inf")]
    pub temp_min: f32,
    #[serde(default = "default_pos_inf")]
    pub temp_max: f32,
}

fn default_neg_inf() -> f32 { f32::NEG_INFINITY }
fn default_pos_inf() -> f32 { f32::INFINITY }

/// Planet-level weather configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct WeatherConfig {
    pub precipitation_chance: f32,
    pub precipitation_duration: (f32, f32),
    pub cooldown: (f32, f32),
    pub types: Vec<WeatherTypeEntry>,
}
```

Add to `PlanetTypeAsset`:
```rust
#[serde(default)]
pub base_temperature: Option<f32>,
#[serde(default)]
pub weather: Option<WeatherConfig>,
```

Note: `BiomeAsset` already has a `weather: Option<Vec<String>>` field -- this is a different field on a different struct. Leave the BiomeAsset one as-is.

- [ ] **Step 2: Add fields to ActiveWorld**

In `src/registry/world.rs`, add to `ActiveWorld`:
```rust
pub base_temperature: f32,
pub weather_config: Option<crate::registry::assets::WeatherConfig>,
```

- [ ] **Step 3: Update ALL ActiveWorld construction sites**

**`src/test_helpers.rs:20`** (`test_active_world()`):
```rust
base_temperature: 15.0,
weather_config: None,
```

**`src/registry/loading.rs:463`** (ship bootstrap -- find `ActiveWorld {`):
```rust
base_temperature: 0.0,
weather_config: None,
```

**`src/cosmos/warp.rs:190`** (warp to planet -- find `ActiveWorld {`):
Wire from the planet template. This function has access to the `PlanetTypeAsset` via the loaded assets. Add:
```rust
base_temperature: planet_template.base_temperature.unwrap_or(15.0),
weather_config: planet_template.weather.clone(),
```
(Verify variable names by reading the function context.)

**`src/cosmos/warp.rs:394`** (warp to ship -- find `ActiveWorld {`):
```rust
base_temperature: 0.0,
weather_config: None,
```

**`src/cosmos/ship_hull.rs:230`** (`ship_world()` test helper):
```rust
base_temperature: 0.0,
weather_config: None,
```

**`src/registry/world.rs`** (if there is a test that constructs ActiveWorld -- check `test_config()`):
```rust
base_temperature: 15.0,
weather_config: None,
```

- [ ] **Step 4: Create temperature.rs**

Create `src/weather/temperature.rs`:
```rust
use crate::registry::biome::BiomeRegistry;
use crate::registry::world::ActiveWorld;
use crate::world::biome_map::BiomeMap;
use crate::world::day_night::WorldTime;

/// Compute the local temperature at a given tile X position.
pub fn local_temperature(
    tile_x: i32,
    world: &ActiveWorld,
    world_time: &WorldTime,
    biome_map: &BiomeMap,
    biome_registry: &BiomeRegistry,
) -> f32 {
    let wrapped_x = world.wrap_tile_x(tile_x).max(0) as u32;
    let biome_id = biome_map.biome_at(wrapped_x);
    let biome = biome_registry.get(biome_id);

    world.base_temperature
        + world_time.temperature_celsius_offset
        + biome.temperature_offset
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::fixtures;

    #[test]
    fn local_temp_uses_base_plus_offsets() {
        let mut world = fixtures::test_active_world();
        world.base_temperature = 15.0;
        let br = fixtures::test_biome_registry();
        let bm = fixtures::test_biome_map(&br);
        let mut wt = WorldTime::default();
        wt.temperature_celsius_offset = -5.0;

        // meadow has temperature_offset = 0.0 in test fixtures
        let temp = local_temperature(100, &world, &wt, &bm, &br);
        assert!((temp - 10.0).abs() < 0.01); // 15 + (-5) + 0
    }
}
```

- [ ] **Step 5: Register module**

Add `pub mod temperature;` to `src/weather/mod.rs`.

- [ ] **Step 6: Update garden.planet.ron**

Add to `garden.planet.ron`:
```ron
    base_temperature: Some(15.0),
    weather: Some((
        precipitation_chance: 0.3,
        precipitation_duration: (60.0, 180.0),
        cooldown: (60.0, 300.0),
        types: [
            (kind: "snow",      temp_max: 0.0),
            (kind: "rain",      temp_min: 0.0, temp_max: 35.0),
            (kind: "fog",       temp_min: 5.0, temp_max: 20.0),
        ],
    )),
```

- [ ] **Step 7: Run tests**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/registry/assets.rs src/registry/world.rs src/registry/loading.rs \
       src/cosmos/warp.rs src/cosmos/ship_hull.rs src/test_helpers.rs \
       src/weather/temperature.rs src/weather/mod.rs \
       assets/worlds/planet_types/garden/garden.planet.ron
git commit -m "feat(weather): add base_temperature, weather config, and temperature calculation"
```

---

## Chunk 2: WeatherState Overhaul & Precipitation Resolution

### Task 4: Rewrite WeatherState and update mod.rs together

This task rewrites `weather_state.rs` AND updates `mod.rs` system registration in one step to avoid intermediate compilation failures. The old `update_weather` system takes `camera_q`, `biome_map`, `biome_registry`, `world_time` params that the new version doesn't need.

**Files:**
- Modify: `src/weather/weather_state.rs` (complete rewrite)
- Modify: `src/weather/mod.rs` (update system registration for update_weather)

- [ ] **Step 1: Rewrite weather_state.rs**

Replace the entire file:
```rust
use bevy::prelude::*;
use rand::Rng;

use crate::registry::world::ActiveWorld;

/// Speed at which intensity ramps up and down per second.
const RAMP_SPEED: f32 = 0.2;

/// The current weather phase -- global to the planet.
#[derive(Debug, Clone, PartialEq)]
pub enum WeatherPhase {
    Clear,
    Precipitation,
}

/// Resource tracking the planetary weather state.
#[derive(Resource)]
pub struct WeatherState {
    pub phase: WeatherPhase,
    pub intensity: f32,
    pub target_intensity: f32,
    pub duration: f32,
    pub elapsed: f32,
    pub cooldown: f32,
    pub check_timer: f32,
    /// Seed set when precipitation starts, used for deterministic type selection.
    /// Stays constant for the duration of the event to prevent flickering.
    pub precipitation_seed: u32,
}

impl Default for WeatherState {
    fn default() -> Self {
        let mut rng = rand::thread_rng();
        Self {
            phase: WeatherPhase::Clear,
            intensity: 0.0,
            target_intensity: 0.0,
            duration: 0.0,
            elapsed: 0.0,
            cooldown: rng.gen_range(60.0..180.0),
            check_timer: 5.0,
            precipitation_seed: 0,
        }
    }
}

impl WeatherState {
    pub fn intensity(&self) -> f32 {
        self.intensity
    }

    pub fn is_precipitating(&self) -> bool {
        self.phase == WeatherPhase::Precipitation
    }
}

/// System that drives weather phase transitions based on planet config.
pub fn update_weather(
    mut state: ResMut<WeatherState>,
    time: Res<Time>,
    world: Res<ActiveWorld>,
) {
    let dt = time.delta_secs();

    let Some(config) = &world.weather_config else {
        return;
    };

    match state.phase {
        WeatherPhase::Clear => {
            if state.cooldown > 0.0 {
                state.cooldown -= dt;
                return;
            }

            state.check_timer -= dt;
            if state.check_timer > 0.0 {
                return;
            }
            state.check_timer = 5.0;

            let mut rng = rand::thread_rng();
            if rng.r#gen::<f32>() < config.precipitation_chance {
                let duration = rng.gen_range(
                    config.precipitation_duration.0..config.precipitation_duration.1,
                );
                let target_intensity = rng.gen_range(0.5..1.0);
                state.phase = WeatherPhase::Precipitation;
                state.intensity = 0.0;
                state.target_intensity = target_intensity;
                state.elapsed = 0.0;
                state.duration = duration;
                state.precipitation_seed = rng.r#gen::<u32>();
            }
        }
        WeatherPhase::Precipitation => {
            state.elapsed += dt;

            if state.elapsed >= state.duration {
                state.target_intensity = 0.0;
            }

            let diff = state.target_intensity - state.intensity;
            if diff.abs() < 0.001 && state.target_intensity == 0.0 {
                let mut rng = rand::thread_rng();
                state.phase = WeatherPhase::Clear;
                state.intensity = 0.0;
                state.cooldown = rng.gen_range(config.cooldown.0..config.cooldown.1);
                state.check_timer = 5.0;
            } else {
                state.intensity += diff.signum() * RAMP_SPEED * dt;
                state.intensity = state.intensity.clamp(0.0, 1.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_starts_clear() {
        let state = WeatherState::default();
        assert_eq!(state.phase, WeatherPhase::Clear);
        assert_eq!(state.intensity(), 0.0);
        assert!(!state.is_precipitating());
    }

    #[test]
    fn is_precipitating_when_precipitation_phase() {
        let mut state = WeatherState::default();
        state.phase = WeatherPhase::Precipitation;
        state.intensity = 0.5;
        assert!(state.is_precipitating());
    }
}
```

- [ ] **Step 2: Update mod.rs system registration**

In `src/weather/mod.rs`, the `update_weather` system registration currently sits alongside `wind::update_wind`. The new `update_weather` signature only takes `ResMut<WeatherState>`, `Res<Time>`, `Res<ActiveWorld>` -- Bevy resolves params automatically, so the system registration line itself doesn't change. Just verify it compiles.

Also update the re-exports -- remove `WeatherKind` if it was exported, replace `is_snowing` references.

- [ ] **Step 3: Fix any callers of the old WeatherState API**

Search for `weather.is_snowing()` and `WeatherKind` across the codebase. These exist in:
- `src/weather/snow_overlay.rs` -- will be fixed in Task 8, but must compile now.

Temporarily add backward-compatible methods to WeatherState:
```rust
/// Backward compatibility -- will be removed in Task 8.
pub fn is_snowing(&self) -> bool {
    self.is_precipitating()
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: all tests pass. The snow overlay still uses `is_snowing()` which now maps to `is_precipitating()` -- functionally equivalent for now.

- [ ] **Step 5: Commit**

```bash
git add src/weather/weather_state.rs src/weather/mod.rs
git commit -m "feat(weather): rewrite WeatherState to use Clear/Precipitation phases"
```

---

### Task 5: Create precipitation type resolution

**Files:**
- Create: `src/weather/precipitation.rs`
- Modify: `src/weather/mod.rs`

- [ ] **Step 1: Create precipitation.rs**

Create `src/weather/precipitation.rs`:
```rust
use bevy::prelude::*;

use crate::registry::assets::WeatherConfig;
use crate::registry::biome::BiomeRegistry;
use crate::registry::world::ActiveWorld;
use crate::weather::temperature;
use crate::weather::weather_state::WeatherState;
use crate::world::biome_map::BiomeMap;
use crate::world::day_night::WorldTime;

/// Resolved precipitation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrecipitationType {
    Snow,
    Rain,
    Fog,
    Sandstorm,
}

impl PrecipitationType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "snow" => Some(Self::Snow),
            "rain" => Some(Self::Rain),
            "fog" => Some(Self::Fog),
            "sandstorm" => Some(Self::Sandstorm),
            _ => None,
        }
    }
}

/// Resolve which precipitation type to use given local temperature and config.
/// `seed` should be constant for the duration of a precipitation event to prevent flickering.
pub fn resolve_precipitation_type(
    local_temp: f32,
    config: &WeatherConfig,
    seed: u32,
) -> Option<PrecipitationType> {
    let matching: Vec<PrecipitationType> = config
        .types
        .iter()
        .filter(|entry| local_temp >= entry.temp_min && local_temp < entry.temp_max)
        .filter_map(|entry| PrecipitationType::from_str(&entry.kind))
        .collect();

    if matching.is_empty() {
        return None;
    }

    let index = (seed as usize) % matching.len();
    Some(matching[index])
}

/// Resource tracking the currently resolved weather type (set each frame).
#[derive(Resource)]
pub struct ResolvedWeatherType(pub Option<PrecipitationType>);

/// System that resolves the current weather type each frame.
pub fn resolve_weather_type_system(
    mut commands: Commands,
    weather: Res<WeatherState>,
    world: Res<ActiveWorld>,
    world_time: Res<WorldTime>,
    biome_map: Res<BiomeMap>,
    biome_registry: Res<BiomeRegistry>,
    camera_q: Query<&Transform, With<Camera2d>>,
) {
    if !weather.is_precipitating() {
        commands.insert_resource(ResolvedWeatherType(None));
        return;
    }

    let Ok(cam_tf) = camera_q.single() else {
        commands.insert_resource(ResolvedWeatherType(None));
        return;
    };

    let Some(config) = &world.weather_config else {
        commands.insert_resource(ResolvedWeatherType(None));
        return;
    };

    let tile_x = (cam_tf.translation.x / world.tile_size) as i32;
    let local_temp = temperature::local_temperature(
        tile_x, &world, &world_time, &biome_map, &biome_registry,
    );

    // Use precipitation_seed from WeatherState -- constant for duration of event
    let resolved = resolve_precipitation_type(local_temp, config, weather.precipitation_seed);
    commands.insert_resource(ResolvedWeatherType(resolved));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::assets::{WeatherConfig, WeatherTypeEntry};

    fn test_config() -> WeatherConfig {
        WeatherConfig {
            precipitation_chance: 0.3,
            precipitation_duration: (60.0, 180.0),
            cooldown: (60.0, 300.0),
            types: vec![
                WeatherTypeEntry { kind: "snow".into(), temp_min: f32::NEG_INFINITY, temp_max: 0.0 },
                WeatherTypeEntry { kind: "rain".into(), temp_min: 0.0, temp_max: 35.0 },
                WeatherTypeEntry { kind: "fog".into(), temp_min: 5.0, temp_max: 20.0 },
                WeatherTypeEntry { kind: "sandstorm".into(), temp_min: 30.0, temp_max: f32::INFINITY },
            ],
        }
    }

    #[test]
    fn resolve_snow_below_zero() {
        let config = test_config();
        assert_eq!(
            resolve_precipitation_type(-10.0, &config, 0),
            Some(PrecipitationType::Snow),
        );
    }

    #[test]
    fn resolve_rain_at_warm_temp() {
        let config = test_config();
        assert_eq!(
            resolve_precipitation_type(25.0, &config, 0),
            Some(PrecipitationType::Rain),
        );
    }

    #[test]
    fn resolve_overlap_selects_deterministically() {
        let config = test_config();
        // At 10.0, both rain (0-35) and fog (5-20) match
        let r0 = resolve_precipitation_type(10.0, &config, 0);
        let r1 = resolve_precipitation_type(10.0, &config, 1);
        // Both should be valid types
        assert!(r0 == Some(PrecipitationType::Rain) || r0 == Some(PrecipitationType::Fog));
        assert!(r1 == Some(PrecipitationType::Rain) || r1 == Some(PrecipitationType::Fog));
        // Same seed should give same result
        assert_eq!(r0, resolve_precipitation_type(10.0, &config, 0));
    }

    #[test]
    fn resolve_none_when_no_match() {
        let config = WeatherConfig {
            precipitation_chance: 0.3,
            precipitation_duration: (60.0, 180.0),
            cooldown: (60.0, 300.0),
            types: vec![
                WeatherTypeEntry { kind: "snow".into(), temp_min: f32::NEG_INFINITY, temp_max: 0.0 },
            ],
        };
        assert_eq!(resolve_precipitation_type(20.0, &config, 0), None);
    }
}
```

- [ ] **Step 2: Register module and system in mod.rs**

Add `pub mod precipitation;` to `src/weather/mod.rs`.

Add system registration (after `update_weather`):
```rust
.add_systems(
    Update,
    precipitation::resolve_weather_type_system
        .in_set(GameSet::WorldUpdate)
        .run_if(in_state(AppState::InGame))
        .after(weather_state::update_weather),
)
```

- [ ] **Step 3: Run tests**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/weather/precipitation.rs src/weather/mod.rs
git commit -m "feat(weather): add precipitation type resolution by temperature"
```

---

## Chunk 3: Unified Particle System

### Task 6: Create unified particle system (replaces snow_particles.rs)

**Files:**
- Create: `src/weather/particles.rs`
- Modify: `src/weather/mod.rs`

This is the largest task. Port the pool/spawn/update/render logic from `snow_particles.rs` and generalize it.

- [ ] **Step 1: Create particles.rs with config structs and pool**

Create `src/weather/particles.rs` with:

```rust
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;
use rand::Rng;

use crate::registry::biome::BiomeRegistry;
use crate::registry::world::ActiveWorld;
use crate::weather::precipitation::{PrecipitationType, ResolvedWeatherType};
use crate::weather::weather_state::WeatherState;
use crate::weather::wind::Wind;
use crate::world::biome_map::BiomeMap;
use crate::world::chunk::{Layer, WorldMap};
use crate::world::ctx::WorldCtx;
use crate::world::day_night::WorldTime;

const POOL_CAPACITY: usize = 2500;
const WEATHER_Z: f32 = 3.0;

/// Configuration for a precipitation type's particles.
pub struct WeatherParticleConfig {
    pub fall_speed: (f32, f32),
    pub wind_influence: f32,
    pub angle: f32,
    pub color: (u8, u8, u8, u8),
    pub size: (f32, f32),
    pub length: (f32, f32),
    pub wobble: bool,
    pub spawn_rate: (f32, f32),
    pub lifetime: (f32, f32),
    pub splash: bool,
}

pub fn snow_config() -> WeatherParticleConfig { /* ... hardcoded values from spec ... */ }
pub fn rain_config() -> WeatherParticleConfig { /* ... hardcoded values from spec ... */ }
pub fn sandstorm_config() -> WeatherParticleConfig { /* ... hardcoded values from spec ... */ }

pub fn config_for_type(t: PrecipitationType) -> WeatherParticleConfig {
    match t {
        PrecipitationType::Snow => snow_config(),
        PrecipitationType::Rain => rain_config(),
        PrecipitationType::Sandstorm => sandstorm_config(),
        PrecipitationType::Fog => snow_config(), // should never be called for fog
    }
}
```

Fill in the actual config values from the spec's default config table.

- [ ] **Step 2: Add WeatherParticle and WeatherParticlePool**

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
    pub splash: bool,
    pub wobble_phase: f32,
    pub wobble_speed: f32,
    pub wobble_amplitude: f32,
}

#[derive(Resource)]
pub struct WeatherParticlePool {
    pub particles: Vec<WeatherParticle>,
    next_free: usize,
    pub spawn_accumulator: f32,
}

impl Default for WeatherParticlePool { /* ... capacity POOL_CAPACITY, empty vec ... */ }

impl WeatherParticlePool {
    pub fn allocate(&mut self) -> usize { /* ... ring buffer, same logic as SnowParticlePool ... */ }
}
```

Port the `allocate()` logic directly from `snow_particles.rs`.

- [ ] **Step 3: Add material and mesh entity resources**

```rust
#[derive(Resource)]
pub struct WeatherParticleMaterial {
    pub handle: Handle<ColorMaterial>,
}

#[derive(Component)]
pub struct WeatherMeshEntity;
```

- [ ] **Step 4: Implement init_weather_render**

Port from `init_snow_render` in `snow_particles.rs`. Creates `ColorMaterial`, empty mesh with `WeatherMeshEntity`, stores handle in `WeatherParticleMaterial`.

- [ ] **Step 5: Implement spawn_weather_particles**

System params: `WeatherState`, `ResolvedWeatherType` (optional), camera, window, wind, pool, time.

Logic:
1. If `weather.intensity() == 0.0` or resolved type is `None` or `Fog`, return
2. Get `WeatherParticleConfig` for resolved type
3. Compute spawn rate: `lerp(config.spawn_rate.0, config.spawn_rate.1, intensity)`
4. Use `spawn_accumulator` for sub-frame handling (same as current snow)
5. For each particle to spawn:
   - Position: random X in camera viewport, Y above viewport
   - Velocity: computed from `config.angle` + wind influence
   - Rain angle formula: `base_angle + wind.velocity().x.signum() * wind.strength * 10.0`
   - Color, size, length: randomized from config ranges
   - If `config.wobble`: set wobble params, else zero them

- [ ] **Step 6: Implement update_weather_particles**

Port from `update_snow_particles` in `snow_particles.rs`.

Key changes:
- Use `particle.velocity` instead of computing from `base_fall_speed`
- Apply wind: `position += wind_vel * wind_influence_factor * dt` (wind_influence is baked into velocity at spawn)
- Apply wobble if `wobble_amplitude > 0`
- Collision check: if solid tile AND `particle.splash`, spawn 2-3 splash particles in the same pool (short lifetime 0.1-0.2s, small size 1px, outward velocity)

- [ ] **Step 7: Implement rebuild_weather_mesh**

Port from `rebuild_snow_mesh` in `snow_particles.rs`.

Key change: oriented rectangles. For each alive particle:
- If `length <= size * 1.5`: axis-aligned square (snow/sandstorm)
- Else: rotated rectangle along velocity direction (rain streaks)
  - Compute direction: `velocity.normalize()`
  - Perpendicular: `Vec2::new(-dir.y, dir.x)`
  - 4 corners: center +/- perpendicular * half_size, center +/- direction * half_length
- Alpha fade in last 20% of lifetime (same as snow)

- [ ] **Step 8: Write unit tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_allocate_returns_valid_index() {
        let mut pool = WeatherParticlePool::default();
        let idx = pool.allocate();
        assert!(idx < pool.particles.len());
        assert!(pool.particles[idx].alive);
    }

    #[test]
    fn pool_recycles_dead_particles() {
        let mut pool = WeatherParticlePool::default();
        let idx1 = pool.allocate();
        pool.particles[idx1].alive = false;
        let idx2 = pool.allocate();
        assert_eq!(idx1, idx2);
    }

    #[test]
    fn snow_config_has_wobble_no_splash() {
        let cfg = snow_config();
        assert!(cfg.wobble);
        assert!(!cfg.splash);
    }

    #[test]
    fn rain_config_has_splash_no_wobble() {
        let cfg = rain_config();
        assert!(cfg.splash);
        assert!(!cfg.wobble);
    }

    #[test]
    fn sandstorm_config_high_angle() {
        let cfg = sandstorm_config();
        assert!(cfg.angle > 70.0);
    }
}
```

- [ ] **Step 9: Run tests**

Run: `cargo test particles`
Expected: all tests pass.

- [ ] **Step 10: Commit**

```bash
git add src/weather/particles.rs
git commit -m "feat(weather): add unified particle system with rain, snow, sandstorm configs"
```

---

## Chunk 4: Fog System

### Task 7: Create fog system

**Files:**
- Create: `src/weather/fog.rs`
- Modify: `src/weather/mod.rs`

- [ ] **Step 1: Create fog.rs**

Create `src/weather/fog.rs` with:
- `FogOverlay` component (marker for fullscreen sprite)
- `FogCloud` component (drift_speed, alpha_phase, alpha_speed, base_alpha)
- `FogCloudTexture` resource
- `generate_fog_cloud_image()` -- procedural 64x32 texture, radial gradient, white with max alpha ~80
- `init_fog()` system -- creates cloud texture resource, spawns FogOverlay entity (initially alpha=0, z=2.5, large custom_size), spawns 8 FogCloud entities spread across screen
- `update_fog_overlay()` -- follows camera position, lerps alpha toward `intensity * 0.3` when resolved type is Fog, toward 0 otherwise
- `update_fog_clouds()` -- drift along wind * 0.3, pulse alpha sinusoidally, wrap when too far from camera (>1200px: reposition to opposite side relative to camera)

Import `ResolvedWeatherType` from `precipitation.rs` (not defined in fog.rs).

Fix the cloud wrapping logic:
```rust
if dist_x > 1200.0 {
    // Wrap to opposite side of camera
    let side = if transform.translation.x > cam_tf.translation.x { -1.0 } else { 1.0 };
    transform.translation.x = cam_tf.translation.x + side * 1100.0;
}
```

- [ ] **Step 2: Register module and systems in mod.rs**

Add `pub mod fog;` to `src/weather/mod.rs`.

Register:
```rust
.add_systems(
    OnEnter(AppState::InGame),
    (
        snow_overlay::init_snow_overlay_texture,
        fog::init_fog,
    ),
)
```

```rust
.add_systems(
    Update,
    (
        fog::update_fog_overlay,
        fog::update_fog_clouds,
    )
        .in_set(GameSet::WorldUpdate)
        .run_if(in_state(AppState::InGame))
        .after(precipitation::resolve_weather_type_system),
)
```

- [ ] **Step 3: Run tests**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/weather/fog.rs src/weather/mod.rs
git commit -m "feat(weather): add fog overlay and cloud system"
```

---

## Chunk 5: Integration & Cleanup

### Task 8: Adapt snow_overlay.rs to temperature-based triggers

**Files:**
- Modify: `src/weather/snow_overlay.rs`

- [ ] **Step 1: Update imports**

Add:
```rust
use crate::weather::temperature::local_temperature;
```

Remove imports of old types if present (`WeatherKind`, etc.).

- [ ] **Step 2: Update `update_snow_overlays`**

Replace `let is_snowing = weather.is_snowing();` with `let is_precipitating = weather.is_precipitating();`

For the melting section, replace `world_time.temperature_modifier > 0.5 && !is_snowing` with:
```rust
// Compute local temperature at overlay position
let local_temp = local_temperature(overlay.tile_x, &world, &world_time, &biome_map, &biome_registry);
if local_temp > 2.0 && !is_precipitating {
    // melt...
}
```

For the adding section, replace `wants_snow` logic. For each tile:
```rust
let local_temp = local_temperature(wrapped_tx, &world, &world_time, &biome_map, &biome_registry);
let wants_snow = local_temp < 0.0 && is_precipitating;
// For permanent snow (always cold biomes), also add when temp < 0 even without precipitation
let wants_snow = local_temp < 0.0 && (is_precipitating || local_temp < -5.0);
```

The biome boundary falloff based on `biome.snow_permanent` should be replaced: use the temperature gradient naturally (temperature changes smoothly via biome offset). The existing 4-tile falloff can be removed or kept as a visual smoother based on temperature difference.

Remove the `biome.snow_permanent` check. Remove `biome.snow_base_chance` references (for gradual appearance, use a fixed low chance like the existing `0.05`).

- [ ] **Step 3: Update `update_tree_snow`**

Same changes: replace `biome.snow_permanent` and `is_snowing` with temperature checks.

- [ ] **Step 4: Remove backward-compat `is_snowing()` method from WeatherState**

In `weather_state.rs`, remove the temporary `is_snowing()` method added in Task 4.

- [ ] **Step 5: Run tests**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/weather/snow_overlay.rs src/weather/weather_state.rs
git commit -m "refactor(weather): adapt snow overlays to temperature-based triggers"
```

---

### Task 9: Delete snow_particles.rs, finalize mod.rs wiring

**Files:**
- Delete: `src/weather/snow_particles.rs`
- Modify: `src/weather/mod.rs`

- [ ] **Step 1: Delete snow_particles.rs**

```bash
rm src/weather/snow_particles.rs
```

- [ ] **Step 2: Update mod.rs**

Remove `pub mod snow_particles;` if present.
Remove imports from `snow_particles` (`rebuild_snow_mesh`, `spawn_snow_particles`, etc.).

Replace old particle system registration with new:
```rust
.add_systems(
    Startup,
    particles::init_weather_render,
)
// ...
.add_systems(
    Update,
    (
        particles::spawn_weather_particles,
        particles::update_weather_particles,
        particles::rebuild_weather_mesh,
    )
        .chain()
        .in_set(GameSet::WorldUpdate)
        .run_if(in_state(AppState::InGame))
        .run_if(resource_exists::<particles::WeatherParticleMaterial>)
        .after(precipitation::resolve_weather_type_system),
)
```

Remove old `SnowParticlePool` init. Replace with `WeatherParticlePool`.

- [ ] **Step 3: Build and fix any remaining compilation errors**

Run: `cargo build`
Fix any references to old types. Common things to search:
```
SnowParticlePool, SharedSnowMaterial, SnowMeshEntity, WeatherKind, is_snowing
```

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor(weather): remove snow_particles.rs, finalize unified weather plugin"
```

---

### Task 10: Remove legacy snow_base_chance and snow_permanent fields

**Files:**
- Modify: `src/registry/biome.rs` (BiomeDef)
- Modify: `src/registry/assets.rs` (BiomeAsset)
- Modify: `src/registry/loading.rs`
- Modify: `src/registry/hot_reload.rs`
- Modify: `src/cosmos/ship_hull.rs`
- Modify: `src/test_helpers.rs`
- Modify: `src/registry/biome.rs` tests
- Modify: biome RON files

- [ ] **Step 1: Remove fields from BiomeDef**

Remove `snow_base_chance: f32` and `snow_permanent: bool` from BiomeDef in `src/registry/biome.rs`.

- [ ] **Step 2: Remove from BiomeAsset**

Remove the two fields from `src/registry/assets.rs`.

- [ ] **Step 3: Remove from all construction sites**

Remove from: `loading.rs`, `hot_reload.rs`, `ship_hull.rs`, `test_helpers.rs`, biome.rs tests.

- [ ] **Step 4: Remove from biome RON files**

Remove `snow_base_chance` and `snow_permanent` from meadow, forest, tundra RON files.

- [ ] **Step 5: Verify no remaining references**

Run: `cargo build`
Grep: `grep -r "snow_base_chance\|snow_permanent" src/`
Expected: no hits.

- [ ] **Step 6: Run tests**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor(weather): remove legacy snow_base_chance and snow_permanent fields"
```

---

### Task 11: Smoke test

- [ ] **Step 1: Run the game**

```bash
cargo run
```

- [ ] **Step 2: Verify weather cycle**

- Wait for precipitation to start
- Verify particles match the biome temperature (rain in warm, snow in cold)
- Move between biomes -- verify smooth particle transition
- Wait for precipitation to end -- verify clean stop
- Verify fog overlay and clouds if fog type resolves

- [ ] **Step 3: Verify snow overlays**

- Travel to tundra -- verify permanent snow on tiles and trees
- Verify melting in warm biomes when not precipitating
- Verify no snow underground

- [ ] **Step 4: Fix visual issues and tune params**

Adjust particle configs as needed.

- [ ] **Step 5: Commit any tuning**

```bash
git add -A
git commit -m "fix(weather): tune particle parameters after smoke testing"
```
