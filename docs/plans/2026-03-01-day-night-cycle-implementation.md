# Day/Night Cycle Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a configurable day/night cycle that modulates the RC lighting sun color/intensity and tints the parallax sky background, with architecture ready for future gameplay effects.

**Architecture:** A `WorldTime` resource driven by `DayNightConfig` (loaded from RON) advances `time_of_day` each frame, interpolates sun color/intensity/ambient between 4 phases, and feeds these values into the RC lighting pipeline via modified GPU uniforms. Parallax sky/background sprites are tinted via `sprite.color`. A `DayPhaseChanged` event enables future gameplay hooks.

**Tech Stack:** Rust, Bevy 0.18, WGSL compute shaders, RON configs, bevy_egui

---

### Task 1: DayNightConfig — RON config and resource

**Files:**
- Create: `src/world/day_night.rs`
- Create: `assets/world/day_night.config.ron`

**Step 1: Create the RON config file**

```ron
// assets/world/day_night.config.ron
(
    cycle_duration_secs: 900.0,
    dawn_ratio: 0.10,
    day_ratio: 0.40,
    sunset_ratio: 0.10,
    night_ratio: 0.40,
    sun_colors: [
        (1.0, 0.65, 0.35),
        (1.0, 0.98, 0.90),
        (1.0, 0.50, 0.25),
        (0.15, 0.15, 0.35),
    ],
    sun_intensities: [0.6, 1.0, 0.5, 0.0],
    ambient_mins: [0.08, 0.0, 0.06, 0.04],
    sky_colors: [
        (0.95, 0.55, 0.35, 1.0),
        (1.0,  1.0,  1.0,  1.0),
        (0.90, 0.40, 0.30, 1.0),
        (0.08, 0.08, 0.18, 1.0),
    ],
    danger_multipliers: [0.5, 0.0, 0.5, 1.0],
    temperature_modifiers: [-0.1, 0.0, -0.05, -0.2],
)
```

Arrays are ordered: [dawn, day, sunset, night].

**Step 2: Create `src/world/day_night.rs` with config struct**

```rust
use bevy::prelude::*;
use serde::Deserialize;

/// Day phase indices into the config arrays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DayPhase {
    Dawn = 0,
    Day = 1,
    Sunset = 2,
    Night = 3,
}

impl DayPhase {
    pub fn index(self) -> usize {
        self as usize
    }

    /// Next phase in cycle.
    pub fn next(self) -> Self {
        match self {
            Self::Dawn => Self::Day,
            Self::Day => Self::Sunset,
            Self::Sunset => Self::Night,
            Self::Night => Self::Dawn,
        }
    }
}

impl std::fmt::Display for DayPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dawn => write!(f, "Dawn"),
            Self::Day => write!(f, "Day"),
            Self::Sunset => write!(f, "Sunset"),
            Self::Night => write!(f, "Night"),
        }
    }
}

/// Configuration for day/night cycle, loaded from RON.
/// All arrays are ordered [dawn, day, sunset, night].
#[derive(Resource, Debug, Clone, Deserialize)]
pub struct DayNightConfig {
    pub cycle_duration_secs: f32,
    pub dawn_ratio: f32,
    pub day_ratio: f32,
    pub sunset_ratio: f32,
    pub night_ratio: f32,
    /// Sun RGB color per phase: [dawn, day, sunset, night]
    pub sun_colors: [(f32, f32, f32); 4],
    /// Sun intensity per phase: [dawn, day, sunset, night]
    pub sun_intensities: [f32; 4],
    /// Minimum ambient light per phase: [dawn, day, sunset, night]
    pub ambient_mins: [f32; 4],
    /// Sky tint RGBA per phase: [dawn, day, sunset, night]
    pub sky_colors: [(f32, f32, f32, f32); 4],
    /// Danger multiplier per phase (for future mob spawning)
    pub danger_multipliers: [f32; 4],
    /// Temperature modifier per phase (for future systems)
    pub temperature_modifiers: [f32; 4],
}

impl DayNightConfig {
    /// Returns the phase ratios as an array [dawn, day, sunset, night].
    pub fn phase_ratios(&self) -> [f32; 4] {
        [self.dawn_ratio, self.day_ratio, self.sunset_ratio, self.night_ratio]
    }
}
```

**Step 3: Build and verify it compiles**

Run: `cargo build 2>&1 | head -5`
Expected: compiles (module not yet registered, just checking struct syntax)

**Step 4: Commit**

```
git add src/world/day_night.rs assets/world/day_night.config.ron
git commit -m "feat: add DayNightConfig resource and RON config for day/night cycle"
```

---

### Task 2: WorldTime resource and tick system

**Files:**
- Modify: `src/world/day_night.rs`

**Step 1: Add WorldTime resource and DayPhaseChanged event**

Add to `src/world/day_night.rs`:

```rust
/// Fired when the day phase changes (e.g., Dawn → Day).
#[derive(Event)]
pub struct DayPhaseChanged {
    pub previous: DayPhase,
    pub current: DayPhase,
    pub time_of_day: f32,
}

/// Runtime state for the day/night cycle.
#[derive(Resource, Debug)]
pub struct WorldTime {
    /// Normalized time: 0.0 = midnight, 0.25 = dawn, 0.5 = noon, 0.75 = sunset
    pub time_of_day: f32,
    /// Current phase.
    pub phase: DayPhase,
    /// Progress within current phase, 0.0..1.0
    pub phase_progress: f32,
    /// Computed sun color (interpolated from config).
    pub sun_color: Vec3,
    /// Computed sun intensity, 0.0..1.0
    pub sun_intensity: f32,
    /// Computed ambient minimum.
    pub ambient_min: f32,
    /// Computed sky tint color.
    pub sky_color: Color,
    /// Danger multiplier (for future mob spawning systems).
    pub danger_multiplier: f32,
    /// Temperature modifier (for future systems).
    pub temperature_modifier: f32,
    /// If true, time does not advance (debug).
    pub paused: bool,
}

impl Default for WorldTime {
    fn default() -> Self {
        Self {
            time_of_day: 0.25, // dawn
            phase: DayPhase::Dawn,
            phase_progress: 0.0,
            sun_color: Vec3::new(1.0, 0.98, 0.9),
            sun_intensity: 1.0,
            ambient_min: 0.0,
            sky_color: Color::WHITE,
            danger_multiplier: 0.0,
            temperature_modifier: 0.0,
            paused: false,
        }
    }
}
```

**Step 2: Add tick_world_time system**

Add to `src/world/day_night.rs`:

```rust
/// Compute which phase and progress within that phase for a given time_of_day.
fn compute_phase_and_progress(time_of_day: f32, config: &DayNightConfig) -> (DayPhase, f32) {
    let ratios = config.phase_ratios();
    // Phase boundaries on the 0..1 timeline:
    // Dawn:   0.25 .. 0.25 + dawn_ratio
    // Day:    after dawn .. + day_ratio
    // Sunset: after day .. + sunset_ratio
    // Night:  remaining (wraps around midnight)
    //
    // We use time_of_day=0.0 as midnight. Night straddles midnight.
    // Layout: [Night/2] [Dawn] [Day] [Sunset] [Night/2]
    // Simplified: offset so dawn starts at 0.25, iterate phases from dawn.

    let phases = [DayPhase::Dawn, DayPhase::Day, DayPhase::Sunset, DayPhase::Night];

    // Shift time so dawn=0.0 for easier phase calculation
    let t = (time_of_day - 0.25).rem_euclid(1.0);
    let mut accumulated = 0.0;

    for (i, phase) in phases.iter().enumerate() {
        let ratio = ratios[i];
        if t < accumulated + ratio {
            let progress = (t - accumulated) / ratio;
            return (*phase, progress.clamp(0.0, 1.0));
        }
        accumulated += ratio;
    }

    // Fallback (shouldn't reach due to ratios summing to 1.0)
    (DayPhase::Night, 1.0)
}

/// Linearly interpolate between current phase value and next phase value.
fn lerp_phase_value(values: &[f32; 4], phase: DayPhase, progress: f32) -> f32 {
    let a = values[phase.index()];
    let b = values[phase.next().index()];
    a + (b - a) * progress
}

/// Linearly interpolate between current phase color and next phase color.
fn lerp_phase_color(colors: &[(f32, f32, f32); 4], phase: DayPhase, progress: f32) -> Vec3 {
    let (ar, ag, ab) = colors[phase.index()];
    let (br, bg, bb) = colors[phase.next().index()];
    let a = Vec3::new(ar, ag, ab);
    let b = Vec3::new(br, bg, bb);
    a + (b - a) * progress
}

fn lerp_phase_color4(colors: &[(f32, f32, f32, f32); 4], phase: DayPhase, progress: f32) -> Color {
    let (ar, ag, ab, aa) = colors[phase.index()];
    let (br, bg, bb, ba) = colors[phase.next().index()];
    Color::srgba(
        ar + (br - ar) * progress,
        ag + (bg - ag) * progress,
        ab + (bb - ab) * progress,
        aa + (ba - aa) * progress,
    )
}

/// Advance world time and compute derived values each frame.
pub fn tick_world_time(
    time: Res<Time>,
    config: Res<DayNightConfig>,
    mut world_time: ResMut<WorldTime>,
    mut phase_events: EventWriter<DayPhaseChanged>,
) {
    if world_time.paused {
        // Still recompute derived values (debug slider may have changed time_of_day)
    } else {
        // Advance time
        let dt = time.delta_secs();
        world_time.time_of_day += dt / config.cycle_duration_secs;
        world_time.time_of_day = world_time.time_of_day.rem_euclid(1.0);
    }

    let (phase, progress) = compute_phase_and_progress(world_time.time_of_day, &config);

    // Detect phase change
    if phase != world_time.phase {
        phase_events.write(DayPhaseChanged {
            previous: world_time.phase,
            current: phase,
            time_of_day: world_time.time_of_day,
        });
        info!("Day phase: {} → {}", world_time.phase, phase);
    }

    world_time.phase = phase;
    world_time.phase_progress = progress;

    // Interpolate all derived values
    world_time.sun_color = lerp_phase_color(&config.sun_colors, phase, progress);
    world_time.sun_intensity = lerp_phase_value(&config.sun_intensities, phase, progress);
    world_time.ambient_min = lerp_phase_value(&config.ambient_mins, phase, progress);
    world_time.sky_color = lerp_phase_color4(&config.sky_colors, phase, progress);
    world_time.danger_multiplier = lerp_phase_value(&config.danger_multipliers, phase, progress);
    world_time.temperature_modifier = lerp_phase_value(&config.temperature_modifiers, phase, progress);
}
```

**Step 3: Add unit tests**

Add to `src/world/day_night.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> DayNightConfig {
        DayNightConfig {
            cycle_duration_secs: 100.0,
            dawn_ratio: 0.10,
            day_ratio: 0.40,
            sunset_ratio: 0.10,
            night_ratio: 0.40,
            sun_colors: [
                (1.0, 0.65, 0.35),
                (1.0, 0.98, 0.90),
                (1.0, 0.50, 0.25),
                (0.15, 0.15, 0.35),
            ],
            sun_intensities: [0.6, 1.0, 0.5, 0.0],
            ambient_mins: [0.08, 0.0, 0.06, 0.04],
            sky_colors: [
                (0.95, 0.55, 0.35, 1.0),
                (1.0, 1.0, 1.0, 1.0),
                (0.90, 0.40, 0.30, 1.0),
                (0.08, 0.08, 0.18, 1.0),
            ],
            danger_multipliers: [0.5, 0.0, 0.5, 1.0],
            temperature_modifiers: [-0.1, 0.0, -0.05, -0.2],
        }
    }

    #[test]
    fn phase_at_dawn_start() {
        let config = test_config();
        let (phase, progress) = compute_phase_and_progress(0.25, &config);
        assert_eq!(phase, DayPhase::Dawn);
        assert!(progress.abs() < 0.01);
    }

    #[test]
    fn phase_at_noon() {
        // dawn starts at 0.25, lasts 0.10 → day starts at 0.35
        // day lasts 0.40 → mid-day at 0.35 + 0.20 = 0.55
        let config = test_config();
        let (phase, progress) = compute_phase_and_progress(0.55, &config);
        assert_eq!(phase, DayPhase::Day);
        assert!((progress - 0.5).abs() < 0.01);
    }

    #[test]
    fn phase_at_midnight() {
        // Night straddles midnight. night starts after sunset.
        // dawn=0.10, day=0.40, sunset=0.10, night=0.40
        // dawn ends at 0.25+0.10=0.35, day ends at 0.75, sunset ends at 0.85
        // night: 0.85..1.25 (wraps), so midnight=0.0 is in night
        let config = test_config();
        let (phase, _) = compute_phase_and_progress(0.0, &config);
        assert_eq!(phase, DayPhase::Night);
    }

    #[test]
    fn phase_at_sunset() {
        // sunset starts at 0.25+0.10+0.40 = 0.75
        let config = test_config();
        let (phase, progress) = compute_phase_and_progress(0.75, &config);
        assert_eq!(phase, DayPhase::Sunset);
        assert!(progress.abs() < 0.01);
    }

    #[test]
    fn phase_ratios_sum_to_one() {
        let config = test_config();
        let ratios = config.phase_ratios();
        let sum: f32 = ratios.iter().sum();
        assert!((sum - 1.0).abs() < 0.001);
    }

    #[test]
    fn lerp_phase_value_at_boundaries() {
        let vals = [0.6, 1.0, 0.5, 0.0];
        assert!((lerp_phase_value(&vals, DayPhase::Dawn, 0.0) - 0.6).abs() < 0.001);
        assert!((lerp_phase_value(&vals, DayPhase::Dawn, 1.0) - 1.0).abs() < 0.001);
        assert!((lerp_phase_value(&vals, DayPhase::Dawn, 0.5) - 0.8).abs() < 0.001);
    }

    #[test]
    fn day_phase_next_cycles() {
        assert_eq!(DayPhase::Dawn.next(), DayPhase::Day);
        assert_eq!(DayPhase::Day.next(), DayPhase::Sunset);
        assert_eq!(DayPhase::Sunset.next(), DayPhase::Night);
        assert_eq!(DayPhase::Night.next(), DayPhase::Dawn);
    }
}
```

**Step 4: Run tests**

Run: `cargo test day_night -- --nocapture`
Expected: all tests pass

**Step 5: Commit**

```
git add src/world/day_night.rs
git commit -m "feat: add WorldTime resource, tick system, and phase interpolation with tests"
```

---

### Task 3: Config loading and plugin wiring

**Files:**
- Modify: `src/world/day_night.rs` (add loading system)
- Modify: `src/world/mod.rs` (register module and systems)
- Modify: `src/registry/mod.rs` (register asset loader for day_night config)

**Step 1: Add config loading to day_night.rs**

Add to `src/world/day_night.rs`:

```rust
/// Load DayNightConfig from RON file and insert as resource.
pub fn load_day_night_config(mut commands: Commands) {
    let ron_str = include_str!("../../assets/world/day_night.config.ron");
    let config: DayNightConfig = ron::from_str(ron_str).expect("Failed to parse day_night.config.ron");
    info!(
        "Loaded DayNightConfig: cycle={}s, phases={:.0}/{:.0}/{:.0}/{:.0}%",
        config.cycle_duration_secs,
        config.dawn_ratio * 100.0,
        config.day_ratio * 100.0,
        config.sunset_ratio * 100.0,
        config.night_ratio * 100.0,
    );
    commands.insert_resource(config);
    commands.insert_resource(WorldTime::default());
}
```

**Step 2: Register module and systems in `src/world/mod.rs`**

Add `pub mod day_night;` to module list (line 1 area).

In `WorldPlugin::build`, add:
```rust
.add_event::<day_night::DayPhaseChanged>()
.add_systems(
    OnEnter(AppState::InGame),
    day_night::load_day_night_config,
)
.add_systems(
    Update,
    day_night::tick_world_time
        .in_set(GameSet::WorldUpdate)
        .run_if(resource_exists::<day_night::WorldTime>),
)
```

**Step 3: Build and run tests**

Run: `cargo build && cargo test`
Expected: compiles, all existing + new tests pass

**Step 4: Commit**

```
git add src/world/day_night.rs src/world/mod.rs
git commit -m "feat: wire DayNightConfig loading and tick_world_time system into game loop"
```

---

### Task 4: Pass sun_color and ambient_min through RC pipeline uniforms

**Files:**
- Modify: `src/world/rc_lighting.rs` (use WorldTime instead of const SUN_COLOR)
- Modify: `src/world/rc_pipeline.rs` (add sun_color/ambient_min to uniform structs)

**Step 1: Add sun_color and ambient_min to RcLightingConfig**

In `src/world/rc_lighting.rs`, add fields to `RcLightingConfig`:
```rust
/// Dynamic sun color from day/night cycle (RGB, 0.0..1.0 per channel).
pub sun_color: Vec3,
/// Minimum ambient light level (lune glow). 0.0..1.0.
pub ambient_min: f32,
```

In `Default for RcLightingConfig`, set:
```rust
sun_color: Vec3::new(1.0, 0.98, 0.9),
ambient_min: 0.0,
```

**Step 2: Update extract_lighting_data to use WorldTime**

In `src/world/rc_lighting.rs`, add `world_time: Option<Res<crate::world::day_night::WorldTime>>` to `extract_lighting_data` parameters.

Replace lines that use `SUN_COLOR` constant:
```rust
// Compute effective sun color from day/night cycle
let sun = if let Some(ref wt) = world_time {
    [
        wt.sun_color.x * wt.sun_intensity,
        wt.sun_color.y * wt.sun_intensity,
        wt.sun_color.z * wt.sun_intensity,
    ]
} else {
    SUN_COLOR // fallback when WorldTime not yet available
};
```

Replace all `SUN_COLOR[0]`, `SUN_COLOR[1]`, `SUN_COLOR[2]` references in emissive fill with `sun[0]`, `sun[1]`, `sun[2]`.

Also update `config.sun_color` and `config.ambient_min`:
```rust
if let Some(ref wt) = world_time {
    config.sun_color = wt.sun_color * wt.sun_intensity;
    config.ambient_min = wt.ambient_min;
}
```

**Step 3: Add sun_color to RcUniformsGpu in rc_pipeline.rs**

In `src/world/rc_pipeline.rs`, modify `RcUniformsGpu`:
```rust
struct RcUniformsGpu {
    input_size: UVec2,
    cascade_index: u32,
    cascade_count: u32,
    viewport_offset: UVec2,
    viewport_size: UVec2,
    bounce_damping: f32,
    _pad0: f32,
    grid_origin: IVec2,
    bounce_offset: IVec2,
    sun_color: Vec3,     // NEW — replaces _pad1
    _pad1: f32,          // alignment padding
}
```

And `FinalizeUniformsGpu`:
```rust
struct FinalizeUniformsGpu {
    input_size: UVec2,
    viewport_offset: UVec2,
    viewport_size: UVec2,
    ambient_min: f32,    // NEW — replaces _pad
    _pad: f32,           // alignment
}
```

**Important**: Both structs must remain 64 bytes for GPU alignment. Count carefully:
- `RcUniformsGpu`: 2×u32 + u32 + u32 + 2×u32 + 2×u32 + f32 + f32 + 2×i32 + 2×i32 + 3×f32 + f32 = 64 bytes ✓
- `FinalizeUniformsGpu`: 2×u32 + 2×u32 + 2×u32 + f32 + f32 = 32 bytes. Need to check actual size and pad as needed.

**Step 4: Fill uniforms from config in prepare_rc_bind_groups**

In `prepare_rc_bind_groups`, when building `RcUniformsGpu`, add:
```rust
sun_color: config.sun_color,
_pad1: 0.0,
```

When building `FinalizeUniformsGpu`, add:
```rust
ambient_min: config.ambient_min,
```

**Step 5: Build**

Run: `cargo build`
Expected: compiles (shaders will need matching changes next)

**Step 6: Commit**

```
git add src/world/rc_lighting.rs src/world/rc_pipeline.rs
git commit -m "feat: pass dynamic sun_color and ambient_min through RC pipeline uniforms"
```

---

### Task 5: Update WGSL shaders to use dynamic sun color and ambient

**Files:**
- Modify: `assets/shaders/radiance_cascades.wgsl`
- Modify: `assets/shaders/rc_finalize.wgsl`

**Step 1: Update radiance_cascades.wgsl uniform struct**

Replace the `RcUniforms` struct to match the new Rust layout:
```wgsl
struct RcUniforms {
    input_size: vec2<u32>,       // 0..8
    cascade_index: u32,          // 8..12
    cascade_count: u32,          // 12..16
    viewport_offset: vec2<u32>,  // 16..24
    viewport_size: vec2<u32>,    // 24..32
    bounce_damping: f32,         // 32..36
    _pad0: f32,                  // 36..40
    grid_origin: vec2<i32>,      // 40..48
    bounce_offset: vec2<i32>,    // 48..56
    sun_color: vec3<f32>,        // 56..68  (NEW)
    _pad1: f32,                  // 68..72  (alignment)
}
```

Wait — this changes the struct size from 64 to 72 bytes. Need to verify GPU alignment. Actually let me recalculate:
- vec2<u32> = 8
- u32 + u32 = 8
- vec2<u32> = 8
- vec2<u32> = 8
- f32 + f32 = 8
- vec2<i32> = 8
- vec2<i32> = 8
- Total so far: 56 bytes
- vec3<f32> = 12, needs 16-byte alignment → padded to 16
- Total: 56 + 16 = 72 bytes

But `ShaderType` derive in Rust handles alignment automatically. Both Rust and WGSL must agree on layout. The `encase` crate handles this — it will pad `sun_color: Vec3` to 16 bytes (vec3 aligns to 16 in std140/std430). So total = 56 + 16 = 72. We need to update Rust struct size expectations accordingly. Actually `encase::ShaderType` handles this automatically, no manual padding needed beyond what `_pad` fields provide. Let me simplify:

In Rust `RcUniformsGpu`:
```rust
struct RcUniformsGpu {
    input_size: UVec2,        // 8
    cascade_index: u32,       // 4
    cascade_count: u32,       // 4
    viewport_offset: UVec2,   // 8
    viewport_size: UVec2,     // 8
    bounce_damping: f32,      // 4
    _pad0: f32,               // 4
    grid_origin: IVec2,       // 8
    bounce_offset: IVec2,     // 8
    sun_color: Vec3,          // 12 (+4 pad by encase = 16)
}
```
Total: 8+4+4+8+8+4+4+8+8+16 = 72 bytes.

In WGSL, `vec3<f32>` at the end of a struct is fine — the struct will have size 72 with vec3 aligned to 16.

**Replace sky escape hardcode** (line 185 in `radiance_cascades.wgsl`):
```wgsl
// Was: radiance = vec3<f32>(1.0, 0.98, 0.9);
radiance = uniforms.sun_color;
```

**Step 2: Update rc_finalize.wgsl**

Update `FinalizeUniforms` struct:
```wgsl
struct FinalizeUniforms {
    input_size: vec2<u32>,
    viewport_offset: vec2<u32>,
    viewport_size: vec2<u32>,
    ambient_min: f32,
    _pad: f32,
}
```

Update main function to apply ambient_min:
```wgsl
let raw = probe_radiance(ix, iy) * BRIGHTNESS;
let irradiance = max(raw, vec3<f32>(uniforms.ambient_min));
```

**Step 3: Build and run game briefly to test**

Run: `cargo build && cargo run` (manual visual check — press F3 to verify debug panel still works)

**Step 4: Commit**

```
git add assets/shaders/radiance_cascades.wgsl assets/shaders/rc_finalize.wgsl
git commit -m "feat: shaders use dynamic sun_color for sky escape and ambient_min for night floor"
```

---

### Task 6: Parallax sky tinting

**Files:**
- Modify: `src/parallax/spawn.rs` (add `ParallaxSkyLayer` marker)
- Modify: `src/parallax/transition.rs` (add marker during spawn)
- Create or modify: `src/world/day_night.rs` (add `tint_parallax_layers` system)
- Modify: `src/world/mod.rs` (register tint system)

**Step 1: Add ParallaxSkyLayer marker in spawn.rs**

In `src/parallax/spawn.rs`, add:
```rust
/// Marker for the sky layer (z=-100, speed=0) — receives full day/night tint.
#[derive(Component)]
pub struct ParallaxSkyLayer;
```

**Step 2: Apply marker during spawn in transition.rs**

In `spawn_biome_parallax` in `src/parallax/transition.rs`, when spawning layers:
```rust
// After the commands.spawn((...)) call, check if this is a sky layer
// by name or z_order and conditionally add ParallaxSkyLayer
if layer_def.speed_x == 0.0 && layer_def.speed_y == 0.0 {
    // Sky layer — add marker
    // Need to chain .insert(ParallaxSkyLayer) onto the spawn
}
```

Concretely, modify the spawn to conditionally include the marker:
```rust
let mut entity_cmd = commands.spawn((
    ParallaxLayerConfig { ... },
    ParallaxLayerState::default(),
    Sprite { ... },
    Transform::from_xyz(0.0, 0.0, layer_def.z_order),
));
if layer_def.speed_x == 0.0 && layer_def.speed_y == 0.0 {
    entity_cmd.insert(super::spawn::ParallaxSkyLayer);
}
```

**Step 3: Add tint_parallax_layers system in day_night.rs**

```rust
use crate::parallax::spawn::{ParallaxSkyLayer, ParallaxTile};

/// Tint parallax layers based on time of day.
/// Sky layers get full tint; background layers (hills, trees) get 50% blend.
pub fn tint_parallax_layers(
    world_time: Res<WorldTime>,
    mut sky_query: Query<&mut Sprite, With<ParallaxSkyLayer>>,
    mut bg_query: Query<&mut Sprite, (With<ParallaxTile>, Without<ParallaxSkyLayer>)>,
) {
    let sky_tint = world_time.sky_color;

    // Sky: full RGB tint, preserve alpha (biome transition controls alpha)
    for mut sprite in &mut sky_query {
        let alpha = sprite.color.alpha();
        sprite.color = sky_tint.with_alpha(alpha);
    }

    // Background hills/trees: 50% blend toward sky tint, preserve alpha
    for mut sprite in &mut bg_query {
        let alpha = sprite.color.alpha();
        let blended = Color::WHITE.mix(&sky_tint, 0.5).with_alpha(alpha);
        sprite.color = blended;
    }
}
```

Note: `ParallaxTile` marks individual tile sprites within repeating layers (children). The parent layers without `ParallaxTile` are the layer entities themselves. Actually looking at the code more carefully:
- `ParallaxLayerConfig` is on the layer entity
- `ParallaxTile` is on child sprites for repeating layers
- Sky layers have `ParallaxSkyLayer` (new) on the layer entity

So the tint system should query layer entities with `ParallaxLayerConfig` + `Sprite`, not `ParallaxTile`. Let me adjust:

```rust
pub fn tint_parallax_layers(
    world_time: Res<WorldTime>,
    mut sky_query: Query<&mut Sprite, With<ParallaxSkyLayer>>,
    mut layer_query: Query<&mut Sprite, (With<ParallaxLayerConfig>, Without<ParallaxSkyLayer>)>,
) {
    let sky_tint = world_time.sky_color;

    for mut sprite in &mut sky_query {
        let alpha = sprite.color.alpha();
        sprite.color = sky_tint.with_alpha(alpha);
    }

    for mut sprite in &mut layer_query {
        let alpha = sprite.color.alpha();
        let blended = Color::WHITE.mix(&sky_tint, 0.5).with_alpha(alpha);
        sprite.color = blended;
    }
}
```

Need to import `ParallaxLayerConfig` from the parallax module.

**Step 4: Register tint system in world/mod.rs**

Add to WorldPlugin::build:
```rust
.add_systems(
    Update,
    day_night::tint_parallax_layers
        .in_set(GameSet::Parallax)
        .run_if(resource_exists::<day_night::WorldTime>)
        .before(crate::parallax::transition::parallax_transition_system),
)
```

Wait — `parallax_transition_system` is registered in `ParallaxPlugin`, not `WorldPlugin`. The tint system needs to run before the transition system in the `Parallax` set. Since both sets are chained and run in order within `GameSet::Parallax`, we need `.before()` ordering.

Actually, it's simpler to add the tint system to `ParallaxPlugin` or to just add it in `WorldPlugin` with explicit ordering. Since it depends on `WorldTime` (from world module), putting it in `WorldPlugin` with `.in_set(GameSet::Parallax)` and `.before()` makes sense.

But actually, we want tinting to run BEFORE biome transition, because the transition system sets alpha. The tint system sets RGB. If tint runs after transition, it would overwrite the alpha that transition set. So order:
1. `tint_parallax_layers` — sets RGB, preserves alpha
2. `parallax_transition_system` — sets alpha, preserves RGB

Since both modify `sprite.color`, they need ordering. The tint system preserves alpha from current `sprite.color`, and transition sets alpha. If tint runs first with `sprite.color = sky_tint.with_alpha(current_alpha)`, then transition runs and sets alpha, this works correctly.

**Step 5: Build and test**

Run: `cargo build && cargo test`

**Step 6: Commit**

```
git add src/parallax/spawn.rs src/parallax/transition.rs src/world/day_night.rs src/world/mod.rs
git commit -m "feat: tint parallax sky and background layers based on day/night phase"
```

---

### Task 7: Debug panel — day/night section

**Files:**
- Modify: `src/ui/debug_panel.rs`

**Step 1: Add WorldTime to debug panel parameters**

In `draw_debug_panel` function signature, add:
```rust
mut world_time: Option<ResMut<crate::world::day_night::WorldTime>>,
```

**Step 2: Add Day/Night section after Lighting section**

After the Lighting collapsing header (around line 306), add:

```rust
// --- Day/Night ---
if let Some(ref mut wt) = world_time {
    egui::CollapsingHeader::new(egui::RichText::new("Day/Night").strong())
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("day_night_grid")
                .num_columns(2)
                .spacing([20.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Phase:");
                    ui.monospace(format!("{}", wt.phase));
                    ui.end_row();

                    ui.label("Progress:");
                    ui.monospace(format!("{:.1}%", wt.phase_progress * 100.0));
                    ui.end_row();

                    ui.label("Sun color:");
                    ui.monospace(format!(
                        "({:.2}, {:.2}, {:.2})",
                        wt.sun_color.x, wt.sun_color.y, wt.sun_color.z
                    ));
                    ui.end_row();

                    ui.label("Sun intensity:");
                    ui.monospace(format!("{:.2}", wt.sun_intensity));
                    ui.end_row();

                    ui.label("Ambient min:");
                    ui.monospace(format!("{:.3}", wt.ambient_min));
                    ui.end_row();
                });

            ui.separator();
            ui.label("Time of day:");
            ui.add(egui::Slider::new(&mut wt.time_of_day, 0.0..=0.999).step_by(0.001));

            ui.checkbox(&mut wt.paused, "Pause time");
        });
}
```

**Step 3: Build and test**

Run: `cargo build`
Expected: compiles

**Step 4: Commit**

```
git add src/ui/debug_panel.rs
git commit -m "feat: add day/night debug panel with time slider and pause toggle"
```

---

### Task 8: Integration test and final verification

**Files:**
- No new files — verification only

**Step 1: Run all tests**

Run: `cargo test`
Expected: all tests pass (existing + new day_night tests)

**Step 2: Run the game and visually verify**

Run: `cargo run`
Manual verification:
1. Open debug panel (F3)
2. See Day/Night section with phase=Dawn
3. Drag time slider to ~0.55 → should show Day phase, bright sky
4. Drag to ~0.80 → Sunset, orange tint
5. Drag to ~0.0 → Night, dark sky, reduced light on surface
6. Enable pause, verify time stops
7. Disable pause, verify time advances
8. Check underground (dig down) — lighting should NOT change with time

**Step 3: Final commit if any fixes needed**

```
git add -A
git commit -m "fix: day/night cycle integration fixes"
```

---

## Summary of all files

**Create:**
- `src/world/day_night.rs` — WorldTime, DayNightConfig, DayPhase, DayPhaseChanged, tick_world_time, tint_parallax_layers, load_day_night_config
- `assets/world/day_night.config.ron` — default day/night config

**Modify:**
- `src/world/mod.rs` — register day_night module, systems, event
- `src/world/rc_lighting.rs` — replace const SUN_COLOR with WorldTime, pass sun_color/ambient_min to config
- `src/world/rc_pipeline.rs` — add sun_color to RcUniformsGpu, ambient_min to FinalizeUniformsGpu
- `assets/shaders/radiance_cascades.wgsl` — update uniform struct, use uniforms.sun_color for sky escape
- `assets/shaders/rc_finalize.wgsl` — update uniform struct, apply ambient_min floor
- `src/parallax/spawn.rs` — add ParallaxSkyLayer marker component
- `src/parallax/transition.rs` — conditionally add ParallaxSkyLayer during spawn
- `src/ui/debug_panel.rs` — add Day/Night debug section with slider and pause
