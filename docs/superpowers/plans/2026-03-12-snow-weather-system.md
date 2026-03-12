# Snow & Weather System Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a snow/weather system with wind, falling snowflakes, snow cap overlays on blocks, temperature-driven weather events, and a tundra biome.

**Architecture:** New `src/weather/` module with 5 files (mod, wind, snow_particles, snow_overlay, weather_state). Follows existing patterns: ring-buffer particle pool (like `src/particles/`), sprite overlays (like `src/interaction/crack_overlay.rs`), biome data pipeline (BiomeAsset → BiomeDef → loading.rs).

**Tech Stack:** Bevy 0.18, Rust, RON config files

**Spec:** `docs/superpowers/specs/2026-03-12-snow-weather-system-design.md`

**API Notes (Bevy 0.18 + rand 0.8):**
- Camera query: `Query<(&Transform, &Projection), With<Camera2d>>` — extract ortho via `let Projection::Orthographic(ortho) = projection else { return; };`
- Viewport bounds: `let visible_w = window.width() * ortho.scale; let visible_h = window.height() * ortho.scale;` then `cam_x ± visible_w / 2.0`
- Window query: `Query<&Window, With<PrimaryWindow>>`
- `despawn()` in Bevy 0.18 is already recursive (despawns children) — no `despawn_recursive()` needed
- rand 0.8 API: `rand::thread_rng()`, `rng.gen_range(a..b)`, `rng.gen::<f32>()`
- See `src/parallax/scroll.rs:18-60` for the canonical camera + viewport pattern

---

## Chunk 1: Wind System + Weather State + BiomeDef Extension

### Task 1: BiomeDef Extension — Add Snow Fields

**Files:**
- Modify: `src/registry/assets.rs:293-319` (BiomeAsset struct)
- Modify: `src/registry/biome.rs:17-30` (BiomeDef struct)
- Modify: `src/registry/loading.rs:707-723` (BiomeAsset → BiomeDef mapping)
- Modify: `src/registry/biome.rs:192-235` (test fixtures constructing BiomeDef)

- [ ] **Step 1: Add fields to BiomeAsset**

In `src/registry/assets.rs`, add to the `BiomeAsset` struct after the `parallax` field (line 300):

```rust
#[serde(default)]
pub snow_base_chance: f32,
#[serde(default)]
pub snow_permanent: bool,
```

- [ ] **Step 2: Add fields to BiomeDef**

In `src/registry/biome.rs`, add to the `BiomeDef` struct after `parallax_path` (line 29):

```rust
pub snow_base_chance: f32,
pub snow_permanent: bool,
```

- [ ] **Step 3: Update BiomeDef construction in loading.rs**

In `src/registry/loading.rs`, add to the `BiomeDef { ... }` block at ~line 718:

```rust
snow_base_chance: asset.snow_base_chance,
snow_permanent: asset.snow_permanent,
```

- [ ] **Step 4: Fix test fixtures**

Search for all test code constructing `BiomeDef` and add the new fields. Known locations:
- `src/registry/biome.rs` lines 192-202, 216-224, 227-235 (three tests)
- Any `test_helpers` or fixture functions that build `BiomeDef`

Add to each:
```rust
snow_base_chance: 0.0,
snow_permanent: false,
```

- [ ] **Step 5: Verify tests pass**

Run: `cargo test --lib 2>&1 | tail -20`

- [ ] **Step 6: Commit**

```bash
git add src/registry/assets.rs src/registry/biome.rs src/registry/loading.rs
git commit -m "feat(weather): add snow_base_chance and snow_permanent to BiomeDef pipeline"
```

---

### Task 2: Wind Resource and System

**Files:**
- Create: `src/weather/wind.rs`
- Create: `src/weather/mod.rs`
- Modify: `src/main.rs` (add `mod weather;` and `.add_plugins(weather::WeatherPlugin)`)

- [ ] **Step 1: Create `src/weather/wind.rs` with Wind resource**

```rust
use bevy::prelude::*;
use rand::Rng;

pub const MAX_WIND_SPEED: f32 = 60.0;

#[derive(Resource)]
pub struct Wind {
    pub direction: f32,
    pub strength: f32,
    target_direction: f32,
    target_strength: f32,
    change_timer: Timer,
}

impl Default for Wind {
    fn default() -> Self {
        let mut rng = rand::thread_rng();
        Self {
            direction: rng.gen_range(0.0..std::f32::consts::TAU),
            strength: 0.3,
            target_direction: rng.gen_range(0.0..std::f32::consts::TAU),
            target_strength: 0.3,
            change_timer: Timer::from_seconds(
                rng.gen_range(5.0..15.0),
                TimerMode::Once,
            ),
        }
    }
}

impl Wind {
    pub fn velocity(&self) -> Vec2 {
        Vec2::new(
            self.direction.cos() * self.strength * MAX_WIND_SPEED,
            self.direction.sin() * self.strength * MAX_WIND_SPEED * 0.3,
        )
    }
}

pub fn update_wind(mut wind: ResMut<Wind>, time: Res<Time>) {
    wind.change_timer.tick(time.delta());

    if wind.change_timer.finished() {
        let mut rng = rand::thread_rng();
        let offset = rng.gen_range(-1.05..1.05); // ±60°
        wind.target_direction = wind.direction + offset;
        wind.target_strength = rng.gen_range(0.1..1.0);
        wind.change_timer = Timer::from_seconds(
            rng.gen_range(5.0..15.0),
            TimerMode::Once,
        );
    }

    let lerp_speed = 0.02;
    let diff = (wind.target_direction - wind.direction).rem_euclid(std::f32::consts::TAU);
    let shortest = if diff > std::f32::consts::PI {
        diff - std::f32::consts::TAU
    } else {
        diff
    };
    wind.direction += shortest * lerp_speed;
    wind.strength += (wind.target_strength - wind.strength) * lerp_speed;
}
```

- [ ] **Step 2: Create `src/weather/mod.rs` with WeatherPlugin**

```rust
pub mod wind;

use bevy::prelude::*;

use crate::registry::AppState;
use crate::sets::GameSet;

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<wind::Wind>()
            .add_systems(
                Update,
                wind::update_wind
                    .in_set(GameSet::WorldUpdate)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
```

- [ ] **Step 3: Register WeatherPlugin in main.rs**

Add `mod weather;` to module declarations and `.add_plugins(weather::WeatherPlugin)` after the particles plugin (~line 60).

- [ ] **Step 4: Verify compilation**

Run: `cargo build 2>&1 | tail -5`

- [ ] **Step 5: Commit**

```bash
git add src/weather/ src/main.rs
git commit -m "feat(weather): add Wind resource with smooth direction/strength interpolation"
```

---

### Task 3: WeatherState Resource and System

**Files:**
- Create: `src/weather/weather_state.rs`
- Modify: `src/weather/mod.rs`

- [ ] **Step 1: Create `src/weather/weather_state.rs`**

```rust
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use rand::Rng;

use crate::registry::biome::BiomeRegistry;
use crate::world::biome_map::BiomeMap;
use crate::world::day_night::WorldTime;
use crate::registry::world::ActiveWorld;

#[derive(Debug, Clone)]
pub enum WeatherKind {
    Clear,
    Snowing {
        intensity: f32,
        target_intensity: f32,
        elapsed: f32,
        duration: f32,
    },
}

#[derive(Resource)]
pub struct WeatherState {
    pub current: WeatherKind,
    pub cooldown_timer: Timer,
    pub check_timer: Timer,
}

impl Default for WeatherState {
    fn default() -> Self {
        Self {
            current: WeatherKind::Clear,
            cooldown_timer: Timer::from_seconds(0.0, TimerMode::Once),
            check_timer: Timer::from_seconds(5.0, TimerMode::Repeating),
        }
    }
}

impl WeatherState {
    pub fn intensity(&self) -> f32 {
        match &self.current {
            WeatherKind::Clear => 0.0,
            WeatherKind::Snowing { intensity, .. } => *intensity,
        }
    }

    pub fn is_snowing(&self) -> bool {
        matches!(self.current, WeatherKind::Snowing { .. })
    }
}

const RAMP_SPEED: f32 = 0.2;

pub fn update_weather(
    mut state: ResMut<WeatherState>,
    time: Res<Time>,
    world_time: Res<WorldTime>,
    biome_map: Res<BiomeMap>,
    biome_registry: Res<BiomeRegistry>,
    active_world: Res<ActiveWorld>,
    camera_q: Query<&Transform, With<Camera2d>>,
) {
    let dt = time.delta_secs();
    state.check_timer.tick(time.delta());
    state.cooldown_timer.tick(time.delta());

    let biome_def = if let Ok(cam_tf) = camera_q.single() {
        let tile_x = (cam_tf.translation.x / active_world.tile_size).floor() as i32;
        let wrapped_x = active_world.wrap_tile_x(tile_x).max(0) as u32;
        let biome_id = biome_map.biome_at(wrapped_x);
        biome_registry.get(biome_id).clone()
    } else {
        return;
    };

    match &mut state.current {
        WeatherKind::Clear => {
            if state.check_timer.just_finished() && state.cooldown_timer.finished() {
                let probability = biome_def.snow_base_chance
                    * (1.0 - world_time.temperature_modifier);
                let mut rng = rand::thread_rng();
                if rng.gen::<f32>() < probability {
                    let duration = rng.gen_range(30.0..120.0);
                    let target = rng.gen_range(0.5..1.0);
                    state.current = WeatherKind::Snowing {
                        intensity: 0.0,
                        target_intensity: target,
                        elapsed: 0.0,
                        duration,
                    };
                }
            }
        }
        WeatherKind::Snowing {
            intensity,
            target_intensity,
            elapsed,
            duration,
        } => {
            *elapsed += dt;

            if *elapsed >= *duration {
                *target_intensity = 0.0;
            }

            let diff = *target_intensity - *intensity;
            if diff.abs() < 0.001 && *target_intensity == 0.0 {
                let mut rng = rand::thread_rng();
                state.current = WeatherKind::Clear;
                state.cooldown_timer = Timer::from_seconds(
                    rng.gen_range(60.0..180.0),
                    TimerMode::Once,
                );
                return;
            }
            *intensity += diff.signum() * RAMP_SPEED * dt;
            *intensity = intensity.clamp(0.0, 1.0);
        }
    }
}
```

- [ ] **Step 2: Register in `src/weather/mod.rs`**

```rust
pub mod weather_state;
pub mod wind;

use bevy::prelude::*;

use crate::registry::AppState;
use crate::sets::GameSet;

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<wind::Wind>()
            .init_resource::<weather_state::WeatherState>()
            .add_systems(
                Update,
                (
                    wind::update_wind,
                    weather_state::update_weather,
                )
                    .in_set(GameSet::WorldUpdate)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build 2>&1 | tail -5`

- [ ] **Step 4: Commit**

```bash
git add src/weather/
git commit -m "feat(weather): add WeatherState with temperature-driven snowfall probability"
```

---

## Chunk 2: Snow Particle Pool

### Task 4: Snow Particle Data, Pool, and Rendering

**Files:**
- Create: `src/weather/snow_particles.rs`
- Modify: `src/weather/mod.rs`

- [ ] **Step 1: Create `src/weather/snow_particles.rs`**

Snow particle struct, pool, physics, spawning, and rendering in one file.

```rust
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::sprite_render::AlphaMode2d;
use bevy::window::PrimaryWindow;
use rand::Rng;

use crate::registry::world::ActiveWorld;
use crate::registry::tile::TileRegistry;
use crate::world::chunk::WorldMap;
use super::weather_state::WeatherState;
use super::wind::Wind;

const SNOW_Z: f32 = 3.0;
const POOL_CAPACITY: usize = 1500;
const BASE_SPAWN_RATE: f32 = 80.0;
const MIN_SPAWN_RATE: f32 = 30.0;

#[derive(Clone)]
struct SnowParticle {
    position: Vec2,
    base_fall_speed: f32,
    lifetime: f32,
    age: f32,
    size: f32,
    alpha: f32,
    alive: bool,
    wobble_phase: f32,
    wobble_speed: f32,
    wobble_amplitude: f32,
}

impl SnowParticle {
    fn is_dead(&self) -> bool {
        !self.alive || self.age >= self.lifetime
    }
}

#[derive(Resource)]
pub struct SnowParticlePool {
    particles: Vec<SnowParticle>,
    next_free: usize,
    spawn_accumulator: f32,
}

impl Default for SnowParticlePool {
    fn default() -> Self {
        Self {
            particles: Vec::with_capacity(POOL_CAPACITY),
            next_free: 0,
            spawn_accumulator: 0.0,
        }
    }
}

impl SnowParticlePool {
    fn spawn(&mut self, position: Vec2, base_fall_speed: f32, size: f32, alpha: f32) {
        let mut rng = rand::thread_rng();
        let particle = SnowParticle {
            position,
            base_fall_speed,
            lifetime: rng.gen_range(8.0..12.0),
            age: 0.0,
            size,
            alpha,
            alive: true,
            wobble_phase: rng.gen_range(0.0..std::f32::consts::TAU),
            wobble_speed: rng.gen_range(1.5..3.0),
            wobble_amplitude: rng.gen_range(3.0..8.0),
        };

        // Ring-buffer allocation (same pattern as ParticlePool in src/particles/pool.rs)
        let len = self.particles.len();
        for i in 0..len {
            let idx = (self.next_free + i) % len;
            if self.particles[idx].is_dead() {
                self.particles[idx] = particle;
                self.next_free = (idx + 1) % len;
                return;
            }
        }
        if len < POOL_CAPACITY {
            self.particles.push(particle);
            self.next_free = 0;
        }
    }
}

/// Spawn snow particles above camera.
pub fn spawn_snow_particles(
    mut pool: ResMut<SnowParticlePool>,
    weather: Res<WeatherState>,
    wind: Res<Wind>,
    time: Res<Time>,
    camera_q: Query<(&Transform, &Projection), With<Camera2d>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let intensity = weather.intensity();
    if intensity <= 0.0 {
        return;
    }

    let Ok((cam_tf, projection)) = camera_q.single() else { return; };
    let Projection::Orthographic(ortho) = projection else { return; };
    let Ok(window) = windows.single() else { return; };

    let dt = time.delta_secs();
    let spawn_rate = MIN_SPAWN_RATE + (BASE_SPAWN_RATE - MIN_SPAWN_RATE) * intensity;
    pool.spawn_accumulator += spawn_rate * dt;

    let scale = ortho.scale;
    let visible_w = window.width() * scale;
    let visible_h = window.height() * scale;

    let cam_x = cam_tf.translation.x;
    let cam_y = cam_tf.translation.y;
    let cam_left = cam_x - visible_w / 2.0;
    let cam_top = cam_y + visible_h / 2.0;

    let mut rng = rand::thread_rng();

    while pool.spawn_accumulator >= 1.0 {
        pool.spawn_accumulator -= 1.0;

        let x = cam_left + rng.gen_range(0.0..visible_w);
        let y = cam_top + rng.gen_range(16.0..48.0);

        // Size 1-4px; larger = slower fall (depth illusion)
        let size = rng.gen_range(1.0..4.0);
        let fall_speed = 120.0 - (size - 1.0) * (80.0 / 3.0);
        let alpha = rng.gen_range(0.6..1.0) * (1.0 - (size - 1.0) * 0.1);

        pool.spawn(Vec2::new(x, y), fall_speed, size, alpha);
    }
}

/// Update snow particle positions; kill on collision/lifetime/off-screen.
pub fn update_snow_particles(
    mut pool: ResMut<SnowParticlePool>,
    time: Res<Time>,
    wind: Res<Wind>,
    world_map: Res<WorldMap>,
    tile_registry: Res<TileRegistry>,
    active_world: Res<ActiveWorld>,
    camera_q: Query<(&Transform, &Projection), With<Camera2d>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let dt = time.delta_secs();
    let tile_size = active_world.tile_size;
    let chunk_size = active_world.chunk_size;
    let wind_vel = wind.velocity();

    let cam_bottom = if let (Ok((cam_tf, projection)), Ok(window)) =
        (camera_q.single(), windows.single())
    {
        let Projection::Orthographic(ortho) = projection else { return; };
        cam_tf.translation.y - window.height() * ortho.scale / 2.0
    } else {
        f32::MIN
    };

    for p in &mut pool.particles {
        if p.is_dead() {
            continue;
        }

        p.age += dt;
        if p.age >= p.lifetime {
            p.alive = false;
            continue;
        }

        // Wobble on X
        let wobble = (p.wobble_phase + p.wobble_speed * p.age).sin() * p.wobble_amplitude;

        // Constant fall speed + wind + wobble (no gravity acceleration)
        p.position.x += (wind_vel.x + wobble) * dt;
        p.position.y += (-p.base_fall_speed + wind_vel.y) * dt;

        // Below camera — kill
        if p.position.y < cam_bottom - 32.0 {
            p.alive = false;
            continue;
        }

        // Solid tile collision (same pattern as src/particles/physics.rs:44-60)
        let tile_x = (p.position.x / tile_size).floor() as i32;
        let tile_y = (p.position.y / tile_size).floor() as i32;
        let data_cx = active_world.wrap_chunk_x(tile_x.div_euclid(chunk_size as i32));
        let cy = tile_y.div_euclid(chunk_size as i32);
        let local_x = tile_x.rem_euclid(chunk_size as i32) as u32;
        let local_y = tile_y.rem_euclid(chunk_size as i32) as u32;

        if let Some(chunk) = world_map.chunks.get(&(data_cx, cy)) {
            let idx = (local_y * chunk_size + local_x) as usize;
            if idx < chunk.fg.tiles.len() {
                if tile_registry.is_solid(chunk.fg.tiles[idx]) {
                    p.alive = false;
                }
            }
        }
    }
}

// --- Rendering (mirrors src/particles/render.rs pattern) ---

#[derive(Component)]
pub struct SnowMeshEntity;

#[derive(Resource)]
pub struct SharedSnowMaterial {
    pub handle: Handle<ColorMaterial>,
}

pub fn init_snow_render(
    mut commands: Commands,
    mut color_materials: ResMut<Assets<ColorMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let mat = color_materials.add(ColorMaterial {
        color: Color::WHITE,
        alpha_mode: AlphaMode2d::Blend,
        ..Default::default()
    });
    commands.insert_resource(SharedSnowMaterial {
        handle: mat.clone(),
    });

    let empty_mesh = meshes.add(Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    ));
    commands.spawn((
        SnowMeshEntity,
        Mesh2d(empty_mesh),
        MeshMaterial2d(mat),
        Transform::from_translation(Vec3::new(0.0, 0.0, SNOW_Z)),
        Visibility::default(),
    ));
}

pub fn rebuild_snow_mesh(
    pool: Res<SnowParticlePool>,
    mut meshes: ResMut<Assets<Mesh>>,
    query: Query<&Mesh2d, With<SnowMeshEntity>>,
) {
    let Ok(mesh_2d) = query.single() else { return; };

    let alive: Vec<_> = pool.particles.iter().filter(|p| !p.is_dead()).collect();
    let n = alive.len();
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n * 4);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(n * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(n * 6);

    for (i, p) in alive.iter().enumerate() {
        let base = (i * 4) as u32;
        let x = p.position.x;
        let y = p.position.y;
        let r = p.size * 0.5;
        let c = [1.0, 1.0, 1.0, p.alpha];

        positions.push([x - r, y - r, 0.0]);
        positions.push([x + r, y - r, 0.0]);
        positions.push([x + r, y + r, 0.0]);
        positions.push([x - r, y + r, 0.0]);

        colors.push(c);
        colors.push(c);
        colors.push(c);
        colors.push(c);

        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));

    if let Some(existing) = meshes.get_mut(&mesh_2d.0) {
        *existing = mesh;
    }
}
```

- [ ] **Step 2: Register snow particle systems in `src/weather/mod.rs`**

```rust
pub mod snow_particles;
pub mod weather_state;
pub mod wind;

use bevy::prelude::*;

use crate::registry::AppState;
use crate::sets::GameSet;

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<wind::Wind>()
            .init_resource::<weather_state::WeatherState>()
            .init_resource::<snow_particles::SnowParticlePool>()
            .add_systems(Startup, snow_particles::init_snow_render)
            .add_systems(
                Update,
                (
                    wind::update_wind,
                    weather_state::update_weather,
                    snow_particles::spawn_snow_particles,
                    snow_particles::update_snow_particles,
                    snow_particles::rebuild_snow_mesh,
                )
                    .chain()
                    .in_set(GameSet::WorldUpdate)
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<snow_particles::SharedSnowMaterial>),
            );
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build 2>&1 | tail -10`

- [ ] **Step 4: Manual smoke test**

Temporarily set a biome's `snow_base_chance` to `0.9` in its `.biome.ron`. Run game. Verify:
- Snow particles fall from above screen
- Wind shifts them sideways
- Particles disappear when hitting ground

Revert the temporary change.

- [ ] **Step 5: Commit**

```bash
git add src/weather/
git commit -m "feat(weather): add snow particle pool with wind, collision, and batched rendering"
```

---

## Chunk 3: Snow Overlay System

### Task 5: Snow Overlay Texture, Component, and ChunkDirty Integration

**Files:**
- Create: `src/weather/snow_overlay.rs`
- Modify: `src/weather/mod.rs`

Note: `despawn()` in Bevy 0.18 already recursively despawns children, so no change to `despawn_chunk()` is needed. Parenting overlay entities to the chunk fg entity is sufficient for automatic cleanup.

- [ ] **Step 1: Create `src/weather/snow_overlay.rs`**

```rust
use bevy::prelude::*;
use bevy::image::{Image, ImageSampler};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;
use rand::Rng;

use crate::registry::biome::BiomeRegistry;
use crate::registry::world::ActiveWorld;
use crate::world::biome_map::BiomeMap;
use crate::world::chunk::{ChunkDirty, LoadedChunks, WorldMap};
use crate::world::day_night::WorldTime;
use super::weather_state::WeatherState;

const SNOW_OVERLAY_Z: f32 = 0.05;
const UPDATE_INTERVAL: f32 = 0.5;
const MELT_TEMP_THRESHOLD: f32 = 0.5;

#[derive(Component)]
pub struct SnowOverlay {
    pub tile_x: i32,
    pub tile_y: i32,
}

#[derive(Resource)]
pub struct SnowOverlayTexture {
    pub handle: Handle<Image>,
}

#[derive(Resource)]
pub struct SnowOverlayTimer {
    pub timer: Timer,
}

impl Default for SnowOverlayTimer {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(UPDATE_INTERVAL, TimerMode::Repeating),
        }
    }
}

/// Generate a 16x4px snow cap texture with irregular bottom edge (pixel art style).
/// Intentionally 16x4 — covers only the top quarter of a tile as a thin snow cap.
fn generate_snow_cap_image() -> Image {
    let w = 16u32;
    let h = 4u32;
    let mut data = vec![0u8; (w * h * 4) as usize];

    let depths: [u32; 16] = [2, 3, 3, 4, 4, 3, 4, 4, 3, 3, 4, 4, 3, 4, 3, 2];

    for x in 0..w {
        let depth = depths[x as usize];
        for y in 0..h {
            if y < depth {
                let idx = ((y * w + x) * 4) as usize;
                data[idx] = 240;
                data[idx + 1] = 245;
                data[idx + 2] = 255;
                data[idx + 3] = 230;
            }
        }
    }

    let mut image = Image::new(
        Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        Default::default(),
    );
    image.sampler = ImageSampler::nearest();
    image
}

pub fn init_snow_overlay_texture(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
) {
    let image = generate_snow_cap_image();
    let handle = images.add(image);
    commands.insert_resource(SnowOverlayTexture { handle });
}

/// Main overlay update: adds new snow caps and melts existing ones.
/// Runs on a 0.5s timer, not every frame.
pub fn update_snow_overlays(
    mut commands: Commands,
    mut timer: ResMut<SnowOverlayTimer>,
    time: Res<Time>,
    weather: Res<WeatherState>,
    world_time: Res<WorldTime>,
    world_map: Res<WorldMap>,
    loaded_chunks: Res<LoadedChunks>,
    biome_map: Res<BiomeMap>,
    biome_registry: Res<BiomeRegistry>,
    active_world: Res<ActiveWorld>,
    texture: Res<SnowOverlayTexture>,
    existing_overlays: Query<(Entity, &SnowOverlay)>,
    camera_q: Query<(&Transform, &Projection), With<Camera2d>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    timer.timer.tick(time.delta());
    if !timer.timer.just_finished() {
        return;
    }

    let Ok((cam_tf, projection)) = camera_q.single() else { return; };
    let Projection::Orthographic(ortho) = projection else { return; };
    let Ok(window) = windows.single() else { return; };

    let tile_size = active_world.tile_size;
    let chunk_size = active_world.chunk_size as i32;
    let scale = ortho.scale;
    let visible_w = window.width() * scale;
    let visible_h = window.height() * scale;

    let cam_left = ((cam_tf.translation.x - visible_w / 2.0) / tile_size).floor() as i32 - 1;
    let cam_right = ((cam_tf.translation.x + visible_w / 2.0) / tile_size).ceil() as i32 + 1;
    let cam_bottom = ((cam_tf.translation.y - visible_h / 2.0) / tile_size).floor() as i32 - 1;
    let cam_top = ((cam_tf.translation.y + visible_h / 2.0) / tile_size).ceil() as i32 + 1;

    let is_snowing = weather.is_snowing();
    let should_melt = world_time.temperature_modifier > MELT_TEMP_THRESHOLD;

    let mut rng = rand::thread_rng();

    // Collect existing overlay positions for quick lookup
    let mut existing_positions: std::collections::HashSet<(i32, i32)> =
        existing_overlays.iter().map(|(_, o)| (o.tile_x, o.tile_y)).collect();

    // Melting: remove some overlays in non-permanent biomes
    if should_melt && !is_snowing {
        for (entity, overlay) in &existing_overlays {
            let wrapped = active_world.wrap_tile_x(overlay.tile_x).max(0) as u32;
            let biome_id = biome_map.biome_at(wrapped);
            let biome = biome_registry.get(biome_id);
            if !biome.snow_permanent && rng.gen::<f32>() < 0.1 {
                commands.entity(entity).despawn();
                existing_positions.remove(&(overlay.tile_x, overlay.tile_y));
            }
        }
    }

    // Add new overlays
    for tile_y in cam_bottom..cam_top {
        for tile_x in cam_left..cam_right {
            if existing_positions.contains(&(tile_x, tile_y)) {
                continue;
            }

            let wrapped_x = active_world.wrap_tile_x(tile_x).max(0);
            let biome_id = biome_map.biome_at(wrapped_x as u32);
            let biome = biome_registry.get(biome_id);

            let wants_snow = biome.snow_permanent || is_snowing;
            if !wants_snow {
                continue;
            }

            // Gradual appearance for weather snow (not all at once)
            if !biome.snow_permanent && rng.gen::<f32>() > 0.05 {
                continue;
            }

            // Check: tile is solid AND tile above is air
            let data_cx = active_world.wrap_chunk_x(wrapped_x.div_euclid(chunk_size));
            let cy = tile_y.div_euclid(chunk_size);
            let local_x = wrapped_x.rem_euclid(chunk_size) as u32;
            let local_y = tile_y.rem_euclid(chunk_size) as u32;

            let above_y = tile_y + 1;
            let above_cy = above_y.div_euclid(chunk_size);
            let above_local_y = above_y.rem_euclid(chunk_size) as u32;

            let tile_solid = world_map
                .chunks
                .get(&(data_cx, cy))
                .map(|c| {
                    let idx = (local_y * chunk_size as u32 + local_x) as usize;
                    idx < c.fg.tiles.len() && c.fg.tiles[idx].0 != 0
                })
                .unwrap_or(false);

            let above_air = world_map
                .chunks
                .get(&(data_cx, above_cy))
                .map(|c| {
                    let idx = (above_local_y * chunk_size as u32 + local_x) as usize;
                    idx < c.fg.tiles.len() && c.fg.tiles[idx].0 == 0
                })
                .unwrap_or(true);

            if !tile_solid || !above_air {
                continue;
            }

            // Spawn overlay, parented to the chunk's fg entity
            let display_cx = tile_x.div_euclid(chunk_size);
            let display_cy = tile_y.div_euclid(chunk_size);

            if let Some(chunk_entities) = loaded_chunks.map.get(&(display_cx, display_cy)) {
                let world_x = tile_x as f32 * tile_size + tile_size * 0.5;
                let world_y = tile_y as f32 * tile_size + tile_size + 2.0;

                let overlay_entity = commands
                    .spawn((
                        SnowOverlay { tile_x, tile_y },
                        Sprite {
                            image: texture.handle.clone(),
                            anchor: bevy::sprite::Anchor::TopCenter,
                            ..default()
                        },
                        Transform::from_translation(Vec3::new(world_x, world_y, SNOW_OVERLAY_Z)),
                        Visibility::default(),
                    ))
                    .id();

                commands.entity(chunk_entities.fg).add_child(overlay_entity);
                existing_positions.insert((tile_x, tile_y));
            }
        }
    }
}

/// React to ChunkDirty: remove stale overlays on destroyed blocks,
/// add overlays on newly exposed surfaces.
pub fn handle_dirty_chunk_overlays(
    mut commands: Commands,
    dirty_chunks: Query<&crate::world::chunk::ChunkCoord, With<ChunkDirty>>,
    world_map: Res<WorldMap>,
    loaded_chunks: Res<LoadedChunks>,
    biome_map: Res<BiomeMap>,
    biome_registry: Res<BiomeRegistry>,
    active_world: Res<ActiveWorld>,
    weather: Res<WeatherState>,
    texture: Res<SnowOverlayTexture>,
    existing_overlays: Query<(Entity, &SnowOverlay)>,
) {
    if dirty_chunks.is_empty() {
        return;
    }

    let tile_size = active_world.tile_size;
    let chunk_size = active_world.chunk_size as i32;

    // Collect dirty chunk coords
    let dirty_set: std::collections::HashSet<(i32, i32)> =
        dirty_chunks.iter().map(|c| (c.x, c.y)).collect();

    // Remove overlays whose underlying tile is no longer valid
    for (entity, overlay) in &existing_overlays {
        let cx = overlay.tile_x.div_euclid(chunk_size);
        let cy = overlay.tile_y.div_euclid(chunk_size);
        if !dirty_set.contains(&(cx, cy)) {
            continue;
        }

        let wrapped_x = active_world.wrap_tile_x(overlay.tile_x).max(0);
        let data_cx = active_world.wrap_chunk_x(wrapped_x.div_euclid(chunk_size));
        let local_x = wrapped_x.rem_euclid(chunk_size) as u32;
        let local_y = overlay.tile_y.rem_euclid(chunk_size) as u32;

        let still_valid = world_map
            .chunks
            .get(&(data_cx, cy))
            .map(|c| {
                let idx = (local_y * chunk_size as u32 + local_x) as usize;
                idx < c.fg.tiles.len() && c.fg.tiles[idx].0 != 0
            })
            .unwrap_or(false);

        // Also check tile above is still air
        let above_y = overlay.tile_y + 1;
        let above_cy = above_y.div_euclid(chunk_size);
        let above_local_y = above_y.rem_euclid(chunk_size) as u32;
        let above_air = world_map
            .chunks
            .get(&(data_cx, above_cy))
            .map(|c| {
                let idx = (above_local_y * chunk_size as u32 + local_x) as usize;
                idx < c.fg.tiles.len() && c.fg.tiles[idx].0 == 0
            })
            .unwrap_or(true);

        if !still_valid || !above_air {
            commands.entity(entity).despawn();
        }
    }
}
```

- [ ] **Step 2: Register snow overlay systems in `src/weather/mod.rs`**

```rust
pub mod snow_overlay;
pub mod snow_particles;
pub mod weather_state;
pub mod wind;

use bevy::prelude::*;

use crate::registry::AppState;
use crate::sets::GameSet;

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<wind::Wind>()
            .init_resource::<weather_state::WeatherState>()
            .init_resource::<snow_particles::SnowParticlePool>()
            .init_resource::<snow_overlay::SnowOverlayTimer>()
            .add_systems(Startup, snow_particles::init_snow_render)
            .add_systems(
                OnEnter(AppState::InGame),
                snow_overlay::init_snow_overlay_texture,
            )
            .add_systems(
                Update,
                (
                    wind::update_wind,
                    weather_state::update_weather,
                    snow_particles::spawn_snow_particles,
                    snow_particles::update_snow_particles,
                    snow_particles::rebuild_snow_mesh,
                    snow_overlay::update_snow_overlays,
                    snow_overlay::handle_dirty_chunk_overlays,
                )
                    .chain()
                    .in_set(GameSet::WorldUpdate)
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<snow_particles::SharedSnowMaterial>),
            );
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build 2>&1 | tail -10`

Likely adjustments:
- Check `TileId(0)` for air — verify how air tiles are represented (grep for `TileId` usage in existing code)
- Check `Sprite` API — verify `Sprite { image: ... }` pattern matches Bevy 0.18
- Check `add_child` API availability

- [ ] **Step 4: Manual smoke test**

Set a biome's `snow_base_chance: 0.9` temporarily. Run game. Verify:
- Snow caps appear on exposed surface blocks
- Mining a block removes its snow cap
- Caps appear gradually during snowfall

Revert temporary change.

- [ ] **Step 5: Commit**

```bash
git add src/weather/
git commit -m "feat(weather): add snow overlay caps with chunk parenting and dirty-chunk integration"
```

---

## Chunk 4: Tundra Biome + Polish

### Task 6: Tundra Biome Definition

**Files:**
- Create: `assets/content/biomes/tundra/tundra.biome.ron`
- Create: `assets/content/biomes/tundra/tundra.parallax.ron`
- Create: new tile definitions for `snow_dirt` and `frozen_dirt`
- Modify: planet type RON to include tundra as secondary biome

- [ ] **Step 1: Check tile definition format**

Read `assets/content/tiles/` directory and examine an existing tile definition (e.g., grass or dirt) for the RON schema.

- [ ] **Step 2: Create `snow_dirt` and `frozen_dirt` tile definitions**

Create tile RON files following the existing pattern. Use placeholder autotile textures (solid-color 16x16 images):
- `snow_dirt`: white/light grey
- `frozen_dirt`: blue-grey

- [ ] **Step 3: Create `assets/content/biomes/tundra/tundra.biome.ron`**

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

- [ ] **Step 4: Create tundra parallax config**

Create `assets/content/biomes/tundra/tundra.parallax.ron` — reuse existing biome parallax images as placeholders:

```ron
(
    layers: [
        (
            name: "sky",
            image: "content/biomes/meadow/sky.png",
            speed_x: 0.0,
            speed_y: 0.0,
            repeat_x: false,
            repeat_y: false,
            z_order: -100.0,
        ),
        (
            name: "far_hills",
            image: "content/biomes/rocky/far_rocks.png",
            speed_x: 0.1,
            speed_y: 0.05,
            repeat_x: true,
            repeat_y: false,
            z_order: -90.0,
        ),
        (
            name: "near_hills",
            image: "content/biomes/rocky/near_rocks.png",
            speed_x: 0.3,
            speed_y: 0.15,
            repeat_x: true,
            repeat_y: false,
            z_order: -80.0,
        ),
    ],
)
```

- [ ] **Step 5: Add tundra as a secondary biome to a planet type**

Find the planet type RON that lists biomes (search `assets/` for `.planet.ron` files). Add `"tundra"` to the `secondary_biomes` list.

- [ ] **Step 6: Verify compilation and test**

Run: `cargo build 2>&1 | tail -5`
Run game, explore to find tundra biome. Verify:
- Tundra blocks render (placeholder colors OK)
- Snow falls permanently in tundra
- Snow caps persist on tundra blocks

- [ ] **Step 7: Commit**

```bash
git add assets/content/biomes/tundra/ assets/content/tiles/
git commit -m "feat(weather): add tundra biome with snow_dirt/frozen_dirt tiles"
```

---

### Task 7: Polish — Biome Boundary Transitions and Tuning

**Files:**
- Modify: `src/weather/snow_overlay.rs`
- Modify: `src/weather/snow_particles.rs` (tuning constants)
- Modify: `src/weather/wind.rs` (tuning constants)

- [ ] **Step 1: Add biome boundary transition for overlays**

In `snow_overlay.rs`, in the overlay placement loop, after getting the biome, add a falloff check:

```rust
// Biome boundary falloff (4-tile zone)
if biome.snow_permanent {
    let region_idx = biome_map.region_index_at(wrapped_x as u32);
    let region = &biome_map.regions[region_idx];
    let dist_from_start = wrapped_x as u32 - region.start_x;
    let dist_from_end = (region.start_x + region.width) - wrapped_x as u32;
    let min_dist = dist_from_start.min(dist_from_end);
    if min_dist < 4 {
        let falloff = min_dist as f32 / 4.0;
        if rng.gen::<f32>() > falloff {
            continue;
        }
    }
}
```

- [ ] **Step 2: Tune constants by play-testing**

Adjust these values based on visual feel:
- `BASE_SPAWN_RATE` in `snow_particles.rs` — particle density
- `MAX_WIND_SPEED` in `wind.rs` — how far wind pushes snow
- `wobble_amplitude` range — how much particles sway
- Fall speed range — how fast/slow snow falls
- `MELT_TEMP_THRESHOLD` — when snow melts

- [ ] **Step 3: Run full test suite**

Run: `cargo test --lib 2>&1 | tail -20`
Expected: all tests pass.

- [ ] **Step 4: Final commit**

```bash
git add src/weather/
git commit -m "feat(weather): add biome boundary transitions and tune snow parameters"
```
