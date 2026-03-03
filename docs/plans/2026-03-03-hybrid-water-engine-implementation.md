# Hybrid Water Engine Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add realistic water effects to the existing CA fluid system: multi-octave shader ripples, dynamic wave propagation, particle splashes with metaball rendering, and mass-conserving CA ↔ particle transitions.

**Architecture:** Hybrid engine — CA handles bulk water (oceans, lakes), particles spawn only during interactions (splashes, drops). A wave propagation buffer per chunk simulates dynamic waves. Events connect gameplay triggers to wave impulses and particle spawns. Metaball shader merges nearby particles visually.

**Tech Stack:** Rust, Bevy 0.18 (Message system, Material2d, render-to-texture), WGSL shaders, existing FluidPlugin infrastructure.

**Key Bevy 0.18 notes:**
- Events use `Message` API: `app.add_message::<T>()`, `MessageWriter<T>`, `MessageReader<T>`
- Physics components: `Velocity { x, y }`, `Gravity(f32)`, `Grounded(bool)`, `TileCollider { width, height }`
- Player entity has: `Player`, `Velocity`, `Gravity`, `Grounded`, `TileCollider { 24.0, 40.0 }`
- DroppedItem entities also have `Velocity`, `Gravity`, `TileCollider`
- GameSet chain: `Input -> Physics -> WorldUpdate -> Camera -> Parallax -> Ui`
- Fluid systems run in `GameSet::WorldUpdate`, gated by `AppState::InGame`

---

### Task 1: WaterImpactEvent message type

Foundation for all inter-system communication.

**Files:**
- Create: `src/fluid/events.rs`
- Modify: `src/fluid/mod.rs` (add module + message registration)

**Step 1: Create event types**

Create `src/fluid/events.rs`:

```rust
use bevy::prelude::*;

use crate::fluid::cell::FluidId;

/// Kind of water interaction that occurred.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImpactKind {
    /// Entity entered/exited water (player, NPC, item).
    Splash,
    /// Entity moving through water.
    Wake,
    /// Fluid stream falling onto standing fluid surface.
    Pour,
}

/// Fired when something interacts with a fluid surface.
///
/// Consumed by:
///   - Wave system: writes impulse into wave_velocity buffer
///   - Particle system: spawns splash/wake particles
#[derive(Message, Debug, Clone)]
pub struct WaterImpactEvent {
    /// World-space position of the impact.
    pub position: Vec2,
    /// Velocity of the impacting object.
    pub velocity: Vec2,
    /// Type of interaction.
    pub kind: ImpactKind,
    /// Which fluid was impacted.
    pub fluid_id: FluidId,
    /// Mass of the impacting object (affects splash strength).
    pub mass: f32,
}
```

**Step 2: Wire into FluidPlugin**

In `src/fluid/mod.rs`, add:
```rust
pub mod events;
```

In `FluidPlugin::build()`:
```rust
app.add_message::<events::WaterImpactEvent>();
```

Add to re-exports:
```rust
pub use events::{ImpactKind, WaterImpactEvent};
```

**Step 3: Write test**

In `src/fluid/events.rs`, add test:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impact_event_fields() {
        let evt = WaterImpactEvent {
            position: Vec2::new(10.0, 20.0),
            velocity: Vec2::new(0.0, -50.0),
            kind: ImpactKind::Splash,
            fluid_id: FluidId(1),
            mass: 5.0,
        };
        assert_eq!(evt.kind, ImpactKind::Splash);
        assert_eq!(evt.fluid_id, FluidId(1));
        assert!((evt.mass - 5.0).abs() < f32::EPSILON);
    }
}
```

**Step 4: Run tests**

Run: `cargo test fluid::events`
Expected: PASS

**Step 5: Commit**

`feat(fluid): add WaterImpactEvent message type`

---

### Task 2: Multi-octave shader ripples

Replace single sin() with 3 octaves. Shader-only change, no CPU cost.

**Files:**
- Modify: `assets/engine/shaders/fluid.wgsl`

**Step 1: Update vertex shader wave calculation**

Replace the current wave block in `fluid.wgsl` (line 41-43):

```wgsl
    if is_wave {
        // Multi-octave ripple: base (slow, large) + mid + detail (fast, small)
        let base   = sin(world_pos.x * 1.5 + uniforms.time * 1.0) * 1.2;
        let mid    = sin(world_pos.x * 4.0 + world_pos.y * 0.5 + uniforms.time * 1.8) * 0.5;
        let detail = sin(world_pos.x * 9.0 - world_pos.y * 1.2 + uniforms.time * 3.0) * 0.2;
        world_pos.y += base + mid + detail;
    }
```

**Step 2: Run build to verify shader compiles**

Run: `cargo build`
Expected: compiles (WGSL errors only show at runtime, but syntax should be fine)

**Step 3: Visual test**

Launch game, press F5 to place water, verify:
- Surface has layered ripple motion (not a single uniform wave)
- Three distinct frequencies visible

**Step 4: Commit**

`feat(fluid): multi-octave shader ripples (3 sin layers)`

---

### Task 3: Wave propagation buffer and simulation

2D wave height buffer per chunk with wave equation.

**Files:**
- Create: `src/fluid/wave.rs`
- Modify: `src/fluid/mod.rs` (add module)

**Step 1: Create WaveBuffer struct and simulation**

Create `src/fluid/wave.rs`:

```rust
use bevy::prelude::*;

use std::collections::{HashMap, HashSet};

use crate::fluid::cell::FluidCell;

/// Per-chunk wave state for dynamic surface waves.
///
/// Stores height displacement and velocity for each cell in a chunk.
/// Only cells that contain fluid participate in wave propagation.
#[derive(Debug, Clone)]
pub struct WaveBuffer {
    pub height: Vec<f32>,
    pub velocity: Vec<f32>,
    pub chunk_size: u32,
}

/// Configuration for wave propagation.
#[derive(Resource, Debug, Clone)]
pub struct WaveConfig {
    /// Speed of wave propagation (higher = faster spreading).
    pub speed: f32,
    /// Damping factor per tick (0.97 = waves decay over ~2-3 seconds).
    pub damping: f32,
    /// Waves below this amplitude are zeroed.
    pub epsilon: f32,
    /// Maximum wave height (clamped to prevent visual artifacts).
    pub max_height: f32,
}

impl Default for WaveConfig {
    fn default() -> Self {
        Self {
            speed: 0.3,
            damping: 0.97,
            epsilon: 0.001,
            max_height: 3.0,
        }
    }
}

/// Holds wave buffers for all active chunks.
#[derive(Resource, Default)]
pub struct WaveState {
    pub buffers: HashMap<(i32, i32), WaveBuffer>,
}

impl WaveBuffer {
    pub fn new(chunk_size: u32) -> Self {
        let len = (chunk_size * chunk_size) as usize;
        Self {
            height: vec![0.0; len],
            velocity: vec![0.0; len],
            chunk_size,
        }
    }

    /// Returns true if all heights and velocities are near zero.
    pub fn is_calm(&self, epsilon: f32) -> bool {
        self.height.iter().all(|h| h.abs() < epsilon)
            && self.velocity.iter().all(|v| v.abs() < epsilon)
    }

    /// Apply an impulse at (local_x, local_y).
    pub fn apply_impulse(&mut self, local_x: u32, local_y: u32, impulse: f32) {
        if local_x < self.chunk_size && local_y < self.chunk_size {
            let idx = (local_y * self.chunk_size + local_x) as usize;
            self.velocity[idx] += impulse;
        }
    }

    /// Step the wave equation for one iteration.
    ///
    /// Only propagates through cells that contain fluid (non-empty).
    /// Solid/empty cells act as boundaries (wave reflects).
    pub fn step(&mut self, fluids: &[FluidCell], config: &WaveConfig) {
        let size = self.chunk_size;
        let len = (size * size) as usize;
        debug_assert_eq!(fluids.len(), len);

        // Compute new velocities based on neighbor height average
        let old_height = self.height.clone();

        for y in 0..size {
            for x in 0..size {
                let idx = (y * size + x) as usize;
                if fluids[idx].is_empty() {
                    self.height[idx] = 0.0;
                    self.velocity[idx] = 0.0;
                    continue;
                }

                let mut sum = 0.0;
                let mut count = 0u32;

                // Left
                if x > 0 {
                    let ni = (y * size + x - 1) as usize;
                    if !fluids[ni].is_empty() {
                        sum += old_height[ni];
                        count += 1;
                    }
                }
                // Right
                if x + 1 < size {
                    let ni = (y * size + x + 1) as usize;
                    if !fluids[ni].is_empty() {
                        sum += old_height[ni];
                        count += 1;
                    }
                }
                // Down
                if y > 0 {
                    let ni = ((y - 1) * size + x) as usize;
                    if !fluids[ni].is_empty() {
                        sum += old_height[ni];
                        count += 1;
                    }
                }
                // Up
                if y + 1 < size {
                    let ni = ((y + 1) * size + x) as usize;
                    if !fluids[ni].is_empty() {
                        sum += old_height[ni];
                        count += 1;
                    }
                }

                if count > 0 {
                    let avg = sum / count as f32;
                    self.velocity[idx] += (avg - old_height[idx]) * config.speed;
                }

                self.velocity[idx] *= config.damping;
                self.height[idx] += self.velocity[idx];
                self.height[idx] = self.height[idx].clamp(-config.max_height, config.max_height);

                if self.height[idx].abs() < config.epsilon
                    && self.velocity[idx].abs() < config.epsilon
                {
                    self.height[idx] = 0.0;
                    self.velocity[idx] = 0.0;
                }
            }
        }
    }
}

/// Reconcile wave heights at chunk boundaries (horizontal only, matching fluid reconcile).
pub fn reconcile_wave_boundaries(
    wave_state: &mut WaveState,
    active_chunks: &HashSet<(i32, i32)>,
    chunk_size: u32,
    width_chunks: i32,
) {
    let pairs: Vec<((i32, i32), (i32, i32))> = active_chunks
        .iter()
        .filter_map(|&(cx, cy)| {
            let right = ((cx + 1).rem_euclid(width_chunks), cy);
            if active_chunks.contains(&right) {
                Some(((cx, cy), right))
            } else {
                None
            }
        })
        .collect();

    for (left_key, right_key) in pairs {
        // Average wave heights at the shared boundary column
        let last_col = chunk_size - 1;

        // Read heights from both sides
        let mut left_heights = vec![0.0f32; chunk_size as usize];
        let mut right_heights = vec![0.0f32; chunk_size as usize];

        if let Some(left_buf) = wave_state.buffers.get(&left_key) {
            for y in 0..chunk_size {
                left_heights[y as usize] = left_buf.height[(y * chunk_size + last_col) as usize];
            }
        }
        if let Some(right_buf) = wave_state.buffers.get(&right_key) {
            for y in 0..chunk_size {
                right_heights[y as usize] = right_buf.height[(y * chunk_size) as usize];
            }
        }

        // Average and write back
        if let Some(left_buf) = wave_state.buffers.get_mut(&left_key) {
            for y in 0..chunk_size {
                let avg = (left_heights[y as usize] + right_heights[y as usize]) * 0.5;
                left_buf.height[(y * chunk_size + last_col) as usize] = avg;
            }
        }
        if let Some(right_buf) = wave_state.buffers.get_mut(&right_key) {
            for y in 0..chunk_size {
                let avg = (left_heights[y as usize] + right_heights[y as usize]) * 0.5;
                right_buf.height[(y * chunk_size) as usize] = avg;
            }
        }
    }
}
```

**Step 2: Write tests**

Add to `src/fluid/wave.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::cell::{FluidCell, FluidId};

    fn all_water(chunk_size: u32) -> Vec<FluidCell> {
        vec![FluidCell::new(FluidId(1), 1.0); (chunk_size * chunk_size) as usize]
    }

    #[test]
    fn new_buffer_is_calm() {
        let buf = WaveBuffer::new(4);
        assert!(buf.is_calm(0.001));
    }

    #[test]
    fn impulse_creates_wave() {
        let mut buf = WaveBuffer::new(4);
        buf.apply_impulse(2, 2, 1.0);
        assert!(!buf.is_calm(0.001));
        assert!((buf.velocity[(2 * 4 + 2) as usize] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn wave_propagates_to_neighbors() {
        let mut buf = WaveBuffer::new(4);
        let fluids = all_water(4);
        let config = WaveConfig::default();

        buf.apply_impulse(2, 2, 2.0);

        // Step once: center height increases, neighbors still 0
        buf.step(&fluids, &config);
        let center = buf.height[(2 * 4 + 2) as usize];
        assert!(center > 0.0, "center should have positive height after impulse");

        // Step a few more times: neighbors should get some height
        for _ in 0..5 {
            buf.step(&fluids, &config);
        }

        let left = buf.height[(2 * 4 + 1) as usize];
        let right = buf.height[(2 * 4 + 3) as usize];
        assert!(left.abs() > 0.001, "left neighbor should have wave");
        assert!(right.abs() > 0.001, "right neighbor should have wave");
    }

    #[test]
    fn wave_does_not_propagate_through_empty() {
        let mut buf = WaveBuffer::new(4);
        let mut fluids = all_water(4);
        // Empty cell at (1, 2) blocks propagation to the left
        fluids[(2 * 4 + 1) as usize] = FluidCell::EMPTY;
        let config = WaveConfig::default();

        buf.apply_impulse(2, 2, 2.0);
        for _ in 0..10 {
            buf.step(&fluids, &config);
        }

        let blocked = buf.height[(2 * 4 + 0) as usize];
        // Cell (0,2) should have minimal wave because (1,2) is empty
        assert!(
            blocked.abs() < 0.1,
            "wave should not significantly propagate through empty cell, got {blocked}"
        );
    }

    #[test]
    fn wave_decays_to_calm() {
        let mut buf = WaveBuffer::new(4);
        let fluids = all_water(4);
        let config = WaveConfig::default();

        buf.apply_impulse(2, 2, 1.0);

        // Run many steps
        for _ in 0..500 {
            buf.step(&fluids, &config);
        }

        assert!(buf.is_calm(0.01), "wave should decay to calm after many steps");
    }

    #[test]
    fn max_height_clamped() {
        let mut buf = WaveBuffer::new(4);
        let fluids = all_water(4);
        let config = WaveConfig {
            max_height: 2.0,
            ..Default::default()
        };

        buf.apply_impulse(2, 2, 100.0);
        buf.step(&fluids, &config);

        let h = buf.height[(2 * 4 + 2) as usize];
        assert!(h <= config.max_height, "height {h} should be clamped to {}", config.max_height);
    }
}
```

**Step 3: Wire module**

In `src/fluid/mod.rs`, add:
```rust
pub mod wave;
```

**Step 4: Run tests**

Run: `cargo test fluid::wave`
Expected: all PASS

**Step 5: Commit**

`feat(fluid): wave propagation buffer with 2D wave equation`

---

### Task 4: Wave propagation ECS system + event consumer

Connect wave buffers to ECS, consume WaterImpactEvents, run simulation.

**Files:**
- Modify: `src/fluid/systems.rs` (add wave systems)
- Modify: `src/fluid/mod.rs` (register wave systems and resources)

**Step 1: Add wave update system**

In `src/fluid/systems.rs`, add:

```rust
use crate::fluid::events::{ImpactKind, WaterImpactEvent};
use crate::fluid::wave::{reconcile_wave_boundaries, WaveBuffer, WaveConfig, WaveState};

/// Consume WaterImpactEvents and apply impulses to wave buffers.
pub fn wave_consume_events(
    mut events: MessageReader<WaterImpactEvent>,
    mut wave_state: ResMut<WaveState>,
    active_world: Res<ActiveWorld>,
) {
    let chunk_size = active_world.chunk_size;
    let tile_size = active_world.tile_size;

    for event in events.read() {
        // Convert world position to chunk + local coords
        let tile_x = (event.position.x / tile_size).floor() as i32;
        let tile_y = (event.position.y / tile_size).floor() as i32;
        let cx = tile_x.div_euclid(chunk_size as i32);
        let cy = tile_y.div_euclid(chunk_size as i32);
        let data_cx = active_world.wrap_chunk_x(cx);
        let local_x = tile_x.rem_euclid(chunk_size as i32) as u32;
        let local_y = tile_y.rem_euclid(chunk_size as i32) as u32;

        let impulse = match event.kind {
            ImpactKind::Splash => event.velocity.y.abs() * 0.02 * event.mass.sqrt(),
            ImpactKind::Wake => event.velocity.length() * 0.005,
            ImpactKind::Pour => event.velocity.y.abs() * 0.01,
        };

        let buf = wave_state
            .buffers
            .entry((data_cx, cy))
            .or_insert_with(|| WaveBuffer::new(chunk_size));
        buf.apply_impulse(local_x, local_y, impulse);

        // Spread impulse to neighbors for wider splash
        if matches!(event.kind, ImpactKind::Splash) {
            let spread = impulse * 0.5;
            if local_x > 0 {
                buf.apply_impulse(local_x - 1, local_y, spread);
            }
            if local_x + 1 < chunk_size {
                buf.apply_impulse(local_x + 1, local_y, spread);
            }
        }
    }
}

/// Step wave simulation for all active wave buffers.
pub fn wave_simulation(
    world_map: Res<WorldMap>,
    active_world: Res<ActiveWorld>,
    active_fluids: Res<ActiveFluidChunks>,
    mut wave_state: ResMut<WaveState>,
    wave_config: Res<WaveConfig>,
) {
    let chunk_size = active_world.chunk_size;
    let width_chunks = active_world.width_chunks();

    // Step each buffer
    for &(cx, cy) in &active_fluids.chunks {
        if let Some(buf) = wave_state.buffers.get_mut(&(cx, cy)) {
            if let Some(chunk) = world_map.chunks.get(&(cx, cy)) {
                buf.step(&chunk.fluids, &wave_config);
            }
        }
    }

    // Reconcile boundaries
    reconcile_wave_boundaries(&mut wave_state, &active_fluids.chunks, chunk_size, width_chunks);

    // Prune calm buffers
    wave_state
        .buffers
        .retain(|_, buf| !buf.is_calm(wave_config.epsilon));
}
```

**Step 2: Register in FluidPlugin**

In `src/fluid/mod.rs`, add resources and systems:
```rust
.init_resource::<wave::WaveConfig>()
.init_resource::<wave::WaveState>()
```

Add systems to the `WorldUpdate` system set (after fluid_simulation, before fluid_rebuild_meshes):
```rust
(
    systems::fluid_simulation,
    systems::wave_consume_events,
    systems::wave_simulation,
    systems::fluid_rebuild_meshes,
)
    .chain()
```

**Step 3: Run build**

Run: `cargo build`
Expected: compiles

**Step 4: Commit**

`feat(fluid): wave propagation ECS systems + event consumer`

---

### Task 5: Wave height into render pipeline

Pass wave_height from WaveBuffer into vertex attributes so shader can use it.

**Files:**
- Modify: `src/fluid/render.rs` (extend build_fluid_mesh to accept wave data)
- Modify: `src/fluid/systems.rs` (pass wave data to mesh builder)
- Modify: `assets/engine/shaders/fluid.wgsl` (add dynamic_wave to vertex offset)

**Step 1: Extend ATTRIBUTE_FLUID_DATA to carry wave_height**

Current layout: `[emission_r, emission_g, emission_b, flags]`

Change approach: flags currently encode `is_wave_vertex * 1.0 + is_gas * 2.0`. We can pack wave_height into an expanded attribute. But simpler: add a 5th vertex attribute.

Create new attribute in `src/fluid/render.rs`:

```rust
/// Per-vertex dynamic wave height from wave propagation simulation.
pub const ATTRIBUTE_WAVE_HEIGHT: MeshVertexAttribute =
    MeshVertexAttribute::new("WaveHeight", 982301568, VertexFormat::Float32);
```

Update `build_fluid_mesh` signature to accept optional wave heights:

```rust
pub fn build_fluid_mesh(
    fluids: &[FluidCell],
    chunk_x: i32,
    chunk_y: i32,
    chunk_size: u32,
    tile_size: f32,
    fluid_registry: &FluidRegistry,
    wave_heights: Option<&[f32]>,
) -> Option<Mesh> {
```

Inside the function, for each cell, read wave height:

```rust
let wave_h = wave_heights
    .map(|wh| wh[idx])
    .unwrap_or(0.0);
```

Build `wave_data: Vec<f32>` with `[wave_h, wave_h, wave_h, wave_h]` per quad (4 vertices same value, surface vertices will use it in shader).

Add to mesh:
```rust
mesh.insert_attribute(ATTRIBUTE_WAVE_HEIGHT, wave_data);
```

**Step 2: Update FluidMaterial specialize()**

In `src/fluid/systems.rs`, add the new attribute to the vertex layout:

```rust
fn specialize(...) {
    let vertex_layout = layout.0.get_layout(&[
        Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
        Mesh::ATTRIBUTE_COLOR.at_shader_location(1),
        Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
        ATTRIBUTE_FLUID_DATA.at_shader_location(3),
        ATTRIBUTE_WAVE_HEIGHT.at_shader_location(4),
    ])?;
    ...
}
```

**Step 3: Update shader**

In `fluid.wgsl`, add to VertexInput:
```wgsl
@location(4) wave_height: f32,
```

In vertex function, combine shader ripple with dynamic wave:
```wgsl
    if is_wave {
        let base   = sin(world_pos.x * 1.5 + uniforms.time * 1.0) * 1.2;
        let mid    = sin(world_pos.x * 4.0 + world_pos.y * 0.5 + uniforms.time * 1.8) * 0.5;
        let detail = sin(world_pos.x * 9.0 - world_pos.y * 1.2 + uniforms.time * 3.0) * 0.2;
        world_pos.y += base + mid + detail + in.wave_height;
    }
```

**Step 4: Update fluid_rebuild_meshes to pass wave data**

In `src/fluid/systems.rs`, add `wave_state: Res<WaveState>` parameter to `fluid_rebuild_meshes`, and pass wave heights:

```rust
let wave_heights = wave_state
    .buffers
    .get(&(data_cx, cy))
    .map(|buf| buf.height.as_slice());

let Some(mesh) = build_fluid_mesh(
    &chunk.fluids,
    display_cx, cy,
    chunk_size, tile_size,
    &fluid_registry,
    wave_heights,
) else { continue; };
```

**Step 5: Update existing tests**

All existing calls to `build_fluid_mesh` now need `None` as the last argument (no wave data). Update all test call sites.

**Step 6: Write new test**

```rust
#[test]
fn wave_height_attribute_present() {
    let reg = test_fluid_registry();
    let mut fluids = vec![FluidCell::EMPTY; 4];
    fluids[0] = FluidCell::new(FluidId(1), 1.0);
    let wave = vec![0.5; 4];

    let mesh = build_fluid_mesh(&fluids, 0, 0, 2, 8.0, &reg, Some(&wave))
        .expect("should produce a mesh");

    assert!(
        mesh.attribute(ATTRIBUTE_WAVE_HEIGHT).is_some(),
        "mesh should have WAVE_HEIGHT attribute"
    );
}
```

**Step 7: Run tests**

Run: `cargo test fluid`
Expected: all PASS

**Step 8: Commit**

`feat(fluid): wave_height vertex attribute + shader integration`

---

### Task 6: Particle system (generic, reusable)

New module for a general-purpose particle system.

**Files:**
- Create: `src/particles/mod.rs`
- Create: `src/particles/particle.rs`
- Create: `src/particles/pool.rs`
- Create: `src/particles/physics.rs`
- Modify: `src/main.rs` (add particles module + ParticlePlugin)

**Step 1: Particle struct**

Create `src/particles/particle.rs`:

```rust
use bevy::prelude::*;

use crate::fluid::cell::FluidId;

/// A single particle in the simulation.
#[derive(Debug, Clone)]
pub struct Particle {
    pub position: Vec2,
    pub velocity: Vec2,
    /// Fluid mass carried (for CA reabsorption). 0 for visual-only particles.
    pub mass: f32,
    /// Which fluid this particle represents. FluidId::NONE for non-fluid particles.
    pub fluid_id: FluidId,
    /// Maximum lifetime in seconds.
    pub lifetime: f32,
    /// Current age in seconds.
    pub age: f32,
    /// Visual radius (world units) for metaball rendering.
    pub size: f32,
    /// RGBA color.
    pub color: [f32; 4],
    /// Whether this particle is alive.
    pub alive: bool,
}

impl Particle {
    pub fn is_dead(&self) -> bool {
        !self.alive || self.age >= self.lifetime
    }

    /// Normalized age: 0.0 = just born, 1.0 = about to die.
    pub fn age_ratio(&self) -> f32 {
        (self.age / self.lifetime).min(1.0)
    }
}
```

**Step 2: Particle pool (ring buffer)**

Create `src/particles/pool.rs`:

```rust
use bevy::prelude::*;

use super::particle::Particle;
use crate::fluid::cell::FluidId;

/// Configuration for the particle system.
#[derive(Resource, Debug, Clone)]
pub struct ParticleConfig {
    /// Maximum simultaneous particles.
    pub max_particles: usize,
    /// Gravity applied to particles (px/s²).
    pub gravity: f32,
}

impl Default for ParticleConfig {
    fn default() -> Self {
        Self {
            max_particles: 3000,
            gravity: 980.0,
        }
    }
}

/// Pre-allocated pool of particles.
#[derive(Resource)]
pub struct ParticlePool {
    pub particles: Vec<Particle>,
    /// Index to start searching for free slot.
    next_free: usize,
}

impl ParticlePool {
    pub fn new(capacity: usize) -> Self {
        Self {
            particles: Vec::with_capacity(capacity),
            next_free: 0,
        }
    }

    /// Spawn a particle, returning its index. Recycles dead particles.
    /// Returns None if pool is at capacity and all alive.
    pub fn spawn(
        &mut self,
        position: Vec2,
        velocity: Vec2,
        mass: f32,
        fluid_id: FluidId,
        lifetime: f32,
        size: f32,
        color: [f32; 4],
    ) -> Option<usize> {
        let cap = self.particles.capacity();

        // Search for a dead slot starting from next_free
        for i in 0..self.particles.len() {
            let idx = (self.next_free + i) % self.particles.len();
            if self.particles[idx].is_dead() {
                self.particles[idx] = Particle {
                    position, velocity, mass, fluid_id,
                    lifetime, age: 0.0, size, color, alive: true,
                };
                self.next_free = (idx + 1) % self.particles.len().max(1);
                return Some(idx);
            }
        }

        // No dead slot found — grow if under capacity
        if self.particles.len() < cap {
            let idx = self.particles.len();
            self.particles.push(Particle {
                position, velocity, mass, fluid_id,
                lifetime, age: 0.0, size, color, alive: true,
            });
            self.next_free = (idx + 1) % (self.particles.len().max(1));
            return Some(idx);
        }

        // Pool full — force-kill oldest
        let oldest = self.particles
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.age.partial_cmp(&b.age).unwrap())
            .map(|(i, _)| i);

        if let Some(idx) = oldest {
            // TODO: reabsorb mass into CA before killing
            self.particles[idx] = Particle {
                position, velocity, mass, fluid_id,
                lifetime, age: 0.0, size, color, alive: true,
            };
            self.next_free = (idx + 1) % self.particles.len().max(1);
            Some(idx)
        } else {
            None
        }
    }

    /// Count of currently alive particles.
    pub fn alive_count(&self) -> usize {
        self.particles.iter().filter(|p| !p.is_dead()).count()
    }
}
```

**Step 3: Particle physics update**

Create `src/particles/physics.rs`:

```rust
use bevy::prelude::*;

use super::pool::{ParticleConfig, ParticlePool};
use crate::registry::tile::TileRegistry;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::WorldMap;

/// Update particle positions, apply gravity, handle tile collisions, age particles.
pub fn update_particles(
    mut pool: ResMut<ParticlePool>,
    config: Res<ParticleConfig>,
    time: Res<Time>,
    world_map: Res<WorldMap>,
    tile_registry: Res<TileRegistry>,
    active_world: Res<ActiveWorld>,
) {
    let dt = time.delta_secs().min(1.0 / 20.0);
    let tile_size = active_world.tile_size;
    let chunk_size = active_world.chunk_size;

    for particle in pool.particles.iter_mut() {
        if particle.is_dead() {
            continue;
        }

        // Age
        particle.age += dt;
        if particle.age >= particle.lifetime {
            particle.alive = false;
            continue;
        }

        // Gravity
        particle.velocity.y -= config.gravity * dt;

        // Move
        let new_pos = particle.position + particle.velocity * dt;

        // Tile collision check
        let tile_x = (new_pos.x / tile_size).floor() as i32;
        let tile_y = (new_pos.y / tile_size).floor() as i32;

        if world_map.is_solid(tile_x, tile_y, chunk_size, &active_world) {
            // Bounce off solid tile
            // Simple: reverse velocity component, reduce energy
            if world_map.is_solid(
                (particle.position.x / tile_size).floor() as i32,
                tile_y,
                chunk_size,
                &active_world,
            ) {
                particle.velocity.y *= -0.3;
            } else {
                particle.velocity.x *= -0.3;
            }
            // Don't update position — stay at old pos
        } else {
            particle.position = new_pos;
        }

        // Fade alpha based on age
        let age_ratio = particle.age_ratio();
        if age_ratio > 0.7 {
            let fade = 1.0 - (age_ratio - 0.7) / 0.3;
            particle.color[3] *= fade.max(0.0);
        }
    }
}
```

**Step 4: Module setup**

Create `src/particles/mod.rs`:

```rust
pub mod particle;
pub mod physics;
pub mod pool;

use bevy::prelude::*;

use crate::registry::world::ActiveWorld;
use crate::sets::GameSet;
use crate::AppState;

pub use particle::Particle;
pub use pool::{ParticleConfig, ParticlePool};

pub struct ParticlePlugin;

impl Plugin for ParticlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ParticleConfig>()
            .insert_resource(ParticlePool::new(3000))
            .add_systems(
                Update,
                physics::update_particles
                    .in_set(GameSet::Physics)
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<ActiveWorld>),
            );
    }
}
```

In `src/main.rs`, add:
```rust
pub mod particles;
// In plugin chain:
.add_plugins(particles::ParticlePlugin)
```

**Step 5: Write tests**

In `src/particles/pool.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::cell::FluidId;

    #[test]
    fn spawn_and_count() {
        let mut pool = ParticlePool::new(10);
        pool.spawn(Vec2::ZERO, Vec2::ZERO, 0.1, FluidId(1), 1.0, 2.0, [0.0; 4]);
        assert_eq!(pool.alive_count(), 1);
    }

    #[test]
    fn dead_particles_recycled() {
        let mut pool = ParticlePool::new(2);
        let idx0 = pool.spawn(Vec2::ZERO, Vec2::ZERO, 0.1, FluidId(1), 1.0, 2.0, [0.0; 4]).unwrap();
        pool.particles[idx0].alive = false;

        let idx1 = pool.spawn(Vec2::ONE, Vec2::ONE, 0.2, FluidId(1), 2.0, 3.0, [1.0; 4]).unwrap();
        assert_eq!(idx1, idx0, "should reuse dead slot");
        assert_eq!(pool.alive_count(), 1);
    }

    #[test]
    fn pool_capacity_respected() {
        let mut pool = ParticlePool::new(3);
        for _ in 0..3 {
            pool.spawn(Vec2::ZERO, Vec2::ZERO, 0.1, FluidId(1), 1.0, 2.0, [0.0; 4]);
        }
        assert_eq!(pool.alive_count(), 3);
        // 4th spawn should force-recycle oldest
        let idx = pool.spawn(Vec2::ZERO, Vec2::ZERO, 0.1, FluidId(1), 1.0, 2.0, [0.0; 4]);
        assert!(idx.is_some());
        assert_eq!(pool.alive_count(), 3);
    }

    #[test]
    fn particle_age_ratio() {
        let p = Particle {
            position: Vec2::ZERO, velocity: Vec2::ZERO,
            mass: 0.0, fluid_id: FluidId::NONE,
            lifetime: 2.0, age: 1.0, size: 1.0,
            color: [1.0; 4], alive: true,
        };
        assert!((p.age_ratio() - 0.5).abs() < f32::EPSILON);
    }
}
```

**Step 6: Run tests**

Run: `cargo test particles`
Expected: all PASS

**Step 7: Commit**

`feat(particles): generic particle system with pool, physics, and tile collision`

---

### Task 7: CA ↔ Particle transitions

Mass displacement from CA → particles on impact, reabsorption back.

**Files:**
- Create: `src/fluid/splash.rs`
- Modify: `src/fluid/mod.rs` (add module)
- Modify: `src/fluid/systems.rs` (add splash systems)

**Step 1: Create splash logic**

Create `src/fluid/splash.rs`:

```rust
use bevy::prelude::*;

use crate::fluid::cell::FluidId;
use crate::fluid::events::{ImpactKind, WaterImpactEvent};
use crate::fluid::registry::FluidRegistry;
use crate::fluid::wave::{WaveBuffer, WaveState};
use crate::particles::pool::ParticlePool;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::WorldMap;

/// Configuration for splash generation.
#[derive(Resource, Debug, Clone)]
pub struct SplashConfig {
    /// Fraction of cell mass displaced on Splash impact.
    pub splash_displacement: f32,
    /// Number of particles per unit of displaced mass.
    pub particles_per_mass: f32,
    /// Base lifetime for splash particles (seconds).
    pub particle_lifetime: f32,
    /// Base particle size (world units).
    pub particle_size: f32,
    /// Min velocity.y to trigger splash (ignore tiny movements).
    pub min_splash_velocity: f32,
}

impl Default for SplashConfig {
    fn default() -> Self {
        Self {
            splash_displacement: 0.3,
            particles_per_mass: 15.0,
            particle_lifetime: 1.5,
            particle_size: 2.5,
            min_splash_velocity: 20.0,
        }
    }
}

/// Consume WaterImpactEvents and spawn particles (displacing CA mass).
pub fn spawn_splash_particles(
    mut events: MessageReader<WaterImpactEvent>,
    mut pool: ResMut<ParticlePool>,
    mut world_map: ResMut<WorldMap>,
    fluid_registry: Res<FluidRegistry>,
    active_world: Res<ActiveWorld>,
    splash_config: Res<SplashConfig>,
) {
    let chunk_size = active_world.chunk_size;
    let tile_size = active_world.tile_size;

    for event in events.read() {
        let speed = event.velocity.length();
        if speed < splash_config.min_splash_velocity && event.kind == ImpactKind::Splash {
            continue;
        }

        // Find the CA cell at impact position
        let tile_x = (event.position.x / tile_size).floor() as i32;
        let tile_y = (event.position.y / tile_size).floor() as i32;
        let data_cx = active_world.wrap_chunk_x(tile_x.div_euclid(chunk_size as i32));
        let cy = tile_y.div_euclid(chunk_size as i32);
        let local_x = tile_x.rem_euclid(chunk_size as i32) as u32;
        let local_y = tile_y.rem_euclid(chunk_size as i32) as u32;
        let idx = (local_y * chunk_size + local_x) as usize;

        // Get cell info
        let (cell_mass, cell_fluid_id) = {
            let Some(chunk) = world_map.chunks.get(&(data_cx, cy)) else { continue };
            let cell = &chunk.fluids[idx];
            if cell.is_empty() { continue; }
            (cell.mass, cell.fluid_id)
        };

        if cell_fluid_id != event.fluid_id { continue; }

        let def = fluid_registry.get(cell_fluid_id);

        // Calculate displacement
        let (displaced, particle_count, vel_scale) = match event.kind {
            ImpactKind::Splash => {
                let displaced = (cell_mass * splash_config.splash_displacement)
                    .min(cell_mass - 0.01);
                let count = (displaced * splash_config.particles_per_mass)
                    .ceil() as u32;
                (displaced.max(0.0), count.clamp(4, 20), 0.8)
            }
            ImpactKind::Wake => {
                let displaced = cell_mass * 0.02;
                let count = 2u32;
                (displaced.max(0.0), count, 0.3)
            }
            ImpactKind::Pour => {
                let displaced = cell_mass * 0.05;
                let count = (displaced * splash_config.particles_per_mass * 0.5)
                    .ceil() as u32;
                (displaced.max(0.0), count.clamp(1, 5), 0.5)
            }
        };

        if displaced < 0.001 || particle_count == 0 { continue; }

        // Remove mass from CA cell
        if let Some(chunk) = world_map.chunks.get_mut(&(data_cx, cy)) {
            chunk.fluids[idx].mass -= displaced;
            if chunk.fluids[idx].mass < 0.001 {
                chunk.fluids[idx] = crate::fluid::cell::FluidCell::EMPTY;
            }
        }

        // Spawn particles with fan-shaped velocity
        let mass_per_particle = displaced / particle_count as f32;
        let base_speed = speed * vel_scale;
        let color = [
            def.color[0] as f32 / 255.0,
            def.color[1] as f32 / 255.0,
            def.color[2] as f32 / 255.0,
            def.color[3] as f32 / 255.0,
        ];

        for i in 0..particle_count {
            let angle = std::f32::consts::PI * 0.15
                + (i as f32 / particle_count as f32) * std::f32::consts::PI * 0.7;
            let dir = Vec2::new(angle.cos(), angle.sin());
            let spread = 0.7 + (i as f32 * 0.37).fract() * 0.6; // pseudo-random spread
            let vel = dir * base_speed * spread;

            pool.spawn(
                event.position,
                vel,
                mass_per_particle,
                cell_fluid_id,
                splash_config.particle_lifetime * (0.8 + spread * 0.4),
                splash_config.particle_size * (0.6 + spread * 0.8),
                color,
            );
        }
    }
}

/// Reabsorb particles that fall back into fluid.
pub fn reabsorb_particles(
    mut pool: ResMut<ParticlePool>,
    mut world_map: ResMut<WorldMap>,
    mut wave_state: ResMut<WaveState>,
    active_world: Res<ActiveWorld>,
) {
    let chunk_size = active_world.chunk_size;
    let tile_size = active_world.tile_size;

    for particle in pool.particles.iter_mut() {
        if particle.is_dead() || particle.fluid_id == FluidId::NONE {
            continue;
        }

        let tile_x = (particle.position.x / tile_size).floor() as i32;
        let tile_y = (particle.position.y / tile_size).floor() as i32;
        let data_cx = active_world.wrap_chunk_x(tile_x.div_euclid(chunk_size as i32));
        let cy = tile_y.div_euclid(chunk_size as i32);
        let local_x = tile_x.rem_euclid(chunk_size as i32) as u32;
        let local_y = tile_y.rem_euclid(chunk_size as i32) as u32;
        let idx = (local_y * chunk_size + local_x) as usize;

        let Some(chunk) = world_map.chunks.get(&(data_cx, cy)) else {
            continue;
        };

        // Check if particle is inside fluid of same type
        if idx < chunk.fluids.len()
            && !chunk.fluids[idx].is_empty()
            && chunk.fluids[idx].fluid_id == particle.fluid_id
        {
            // Reabsorb: return mass to CA
            if let Some(chunk) = world_map.chunks.get_mut(&(data_cx, cy)) {
                chunk.fluids[idx].mass += particle.mass;
            }

            // Create wave impulse from impact
            if let Some(buf) = wave_state.buffers.get_mut(&(data_cx, cy)) {
                let impulse = particle.velocity.y.abs() * 0.01;
                buf.apply_impulse(local_x, local_y, impulse);
            }

            particle.alive = false;
        }
    }
}
```

**Step 2: Wire into FluidPlugin**

In `src/fluid/mod.rs`:
```rust
pub mod splash;
```

Register resource:
```rust
.init_resource::<splash::SplashConfig>()
```

Add systems (after wave_simulation, before fluid_rebuild_meshes):
```rust
(
    systems::fluid_simulation,
    systems::wave_consume_events,
    systems::wave_simulation,
    splash::spawn_splash_particles,
    splash::reabsorb_particles,
    systems::fluid_rebuild_meshes,
)
    .chain()
```

**Step 3: Write tests**

In `src/fluid/splash.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splash_config_defaults() {
        let cfg = SplashConfig::default();
        assert!(cfg.splash_displacement > 0.0);
        assert!(cfg.particle_lifetime > 0.0);
    }
}
```

**Step 4: Run tests**

Run: `cargo test fluid::splash`
Expected: PASS

**Step 5: Commit**

`feat(fluid): CA ↔ particle mass transitions with splash spawning and reabsorption`

---

### Task 8: Event detectors

Systems that detect gameplay interactions and emit WaterImpactEvents.

**Files:**
- Create: `src/fluid/detectors.rs`
- Modify: `src/fluid/mod.rs` (add module, register systems)

**Step 1: Create detectors**

Create `src/fluid/detectors.rs`:

```rust
use bevy::prelude::*;

use crate::fluid::cell::FluidId;
use crate::fluid::events::{ImpactKind, WaterImpactEvent};
use crate::fluid::registry::FluidRegistry;
use crate::fluid::systems::ActiveFluidChunks;
use crate::physics::Velocity;
use crate::player::Player;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::WorldMap;

/// Tracks whether an entity was in fluid last frame.
#[derive(Component, Default)]
pub struct FluidContactState {
    /// FluidId the entity was touching last frame (NONE if in air).
    pub last_fluid: FluidId,
}

/// Detect player/entity entering or exiting water → Splash event.
pub fn detect_entity_water_entry(
    mut events: MessageWriter<WaterImpactEvent>,
    mut query: Query<(&Transform, &Velocity, &mut FluidContactState)>,
    world_map: Res<WorldMap>,
    active_world: Res<ActiveWorld>,
    fluid_registry: Res<FluidRegistry>,
) {
    let chunk_size = active_world.chunk_size;
    let tile_size = active_world.tile_size;

    for (transform, velocity, mut contact) in query.iter_mut() {
        let pos = transform.translation.truncate();
        let tile_x = (pos.x / tile_size).floor() as i32;
        let tile_y = (pos.y / tile_size).floor() as i32;
        let data_cx = active_world.wrap_chunk_x(tile_x.div_euclid(chunk_size as i32));
        let cy = tile_y.div_euclid(chunk_size as i32);
        let local_x = tile_x.rem_euclid(chunk_size as i32) as u32;
        let local_y = tile_y.rem_euclid(chunk_size as i32) as u32;

        let current_fluid = world_map
            .chunks
            .get(&(data_cx, cy))
            .and_then(|chunk| {
                let idx = (local_y * chunk_size + local_x) as usize;
                if idx < chunk.fluids.len() && !chunk.fluids[idx].is_empty() {
                    Some(chunk.fluids[idx].fluid_id)
                } else {
                    None
                }
            })
            .unwrap_or(FluidId::NONE);

        let was_in_fluid = contact.last_fluid != FluidId::NONE;
        let is_in_fluid = current_fluid != FluidId::NONE;

        // Entry or exit
        if was_in_fluid != is_in_fluid {
            let fluid_id = if is_in_fluid { current_fluid } else { contact.last_fluid };
            events.write(WaterImpactEvent {
                position: pos,
                velocity: Vec2::new(velocity.x, velocity.y),
                kind: ImpactKind::Splash,
                fluid_id,
                mass: 5.0, // TODO: derive from entity mass component
            });
        }

        contact.last_fluid = current_fluid;
    }
}

/// Detect entity swimming (moving inside fluid) → Wake events.
pub fn detect_entity_swimming(
    mut events: MessageWriter<WaterImpactEvent>,
    query: Query<(&Transform, &Velocity, &FluidContactState)>,
    time: Res<Time>,
) {
    // Throttle: emit Wake every 0.15s
    // We use elapsed time modulo to avoid per-entity timers
    let t = time.elapsed_secs();
    let phase = (t / 0.15).floor();
    let frac = t / 0.15 - phase;
    if frac > 0.1 {
        return; // Only emit near the start of each 0.15s window
    }

    for (transform, velocity, contact) in query.iter() {
        if contact.last_fluid == FluidId::NONE {
            continue;
        }

        let speed = (velocity.x * velocity.x + velocity.y * velocity.y).sqrt();
        if speed < 10.0 {
            continue; // Not moving fast enough
        }

        events.write(WaterImpactEvent {
            position: transform.translation.truncate(),
            velocity: Vec2::new(velocity.x, velocity.y),
            kind: ImpactKind::Wake,
            fluid_id: contact.last_fluid,
            mass: 1.0,
        });
    }
}
```

**Step 2: Add FluidContactState to player spawn**

In `src/player/mod.rs`, add `FluidContactState::default()` to the player entity spawn bundle.

For DroppedItem entities in `src/item/dropped_item.rs` or `src/inventory/systems.rs`, also add `FluidContactState::default()`.

**Step 3: Register detectors in FluidPlugin**

In `src/fluid/mod.rs`:
```rust
pub mod detectors;
```

Add systems in `GameSet::WorldUpdate` (before wave_consume_events):
```rust
(
    detectors::detect_entity_water_entry,
    detectors::detect_entity_swimming,
    systems::fluid_simulation,
    systems::wave_consume_events,
    systems::wave_simulation,
    splash::spawn_splash_particles,
    splash::reabsorb_particles,
    systems::fluid_rebuild_meshes,
)
    .chain()
```

**Step 4: Run build**

Run: `cargo build`
Expected: compiles

**Step 5: Commit**

`feat(fluid): water interaction detectors (entity entry, swimming)`

---

### Task 9: Metaball particle rendering

Two-pass rendering: accumulation → threshold.

**Files:**
- Create: `src/particles/render.rs`
- Create: `assets/engine/shaders/particle_accum.wgsl` (accumulation pass)
- Create: `assets/engine/shaders/particle_composite.wgsl` (threshold + color pass)
- Modify: `src/particles/mod.rs` (add render module and systems)

**Step 1: Accumulation shader**

Create `assets/engine/shaders/particle_accum.wgsl`:

```wgsl
struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,  // quad corner offset
    @location(1) center: vec2<f32>,    // particle world position (instanced)
    @location(2) params: vec2<f32>,    // [size, alpha] (instanced)
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local_uv: vec2<f32>,  // -1..1 from center
    @location(1) alpha: f32,
}

@group(0) @binding(0) var<uniform> view_proj: mat4x4<f32>;

@vertex
fn vertex(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = in.center + in.position.xy * in.params.x;
    out.clip_position = view_proj * vec4<f32>(world_pos, 0.0, 1.0);
    out.local_uv = in.position.xy;
    out.alpha = in.params.y;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let dist_sq = dot(in.local_uv, in.local_uv);
    // Gaussian falloff
    let intensity = exp(-dist_sq * 4.0) * in.alpha;
    return vec4<f32>(intensity, 0.0, 0.0, 1.0);
}
```

**Step 2: Composite shader**

Create `assets/engine/shaders/particle_composite.wgsl`:

```wgsl
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

struct CompositeUniforms {
    fluid_color: vec4<f32>,
    threshold: f32,
    softness: f32,
}

@group(0) @binding(0) var accum_texture: texture_2d<f32>;
@group(0) @binding(1) var accum_sampler: sampler;
@group(0) @binding(2) var<uniform> uniforms: CompositeUniforms;

@vertex
fn vertex(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var out: VertexOutput;
    // Fullscreen triangle
    let x = f32((vi & 1u) << 1u) - 1.0;
    let y = f32((vi & 2u)) - 1.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, 0.5 - y * 0.5);
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let intensity = textureSample(accum_texture, accum_sampler, in.uv).r;
    if intensity < uniforms.threshold - uniforms.softness {
        discard;
    }
    let edge = smoothstep(
        uniforms.threshold - uniforms.softness,
        uniforms.threshold + uniforms.softness,
        intensity
    );
    return vec4<f32>(uniforms.fluid_color.rgb, uniforms.fluid_color.a * edge);
}
```

**Step 3: Render system (CPU side)**

Create `src/particles/render.rs`:

Build a batched mesh from all alive particles each frame. Each particle becomes a quad (4 verts, 6 indices) with instanced center + size data.

This is the most complex rendering task. The full implementation involves:
1. Creating an offscreen render target (half-resolution `R16Float`)
2. Rendering particle quads with gaussian shader into it
3. Compositing the result over the main scene with threshold shader

For the initial implementation, start with simple sprite rendering (one quad per particle, same FluidMaterial shader but simpler). Metaball can be added as an upgrade pass after basic particles work.

```rust
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use super::pool::ParticlePool;

/// Marker for the particle mesh entity.
#[derive(Component)]
pub struct ParticleMeshEntity;

/// Rebuild the particle mesh each frame from alive particles.
pub fn rebuild_particle_mesh(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    pool: Res<ParticlePool>,
    existing: Query<Entity, With<ParticleMeshEntity>>,
) {
    // Collect alive particles
    let alive: Vec<_> = pool.particles.iter().filter(|p| !p.is_dead()).collect();

    // Remove old mesh
    for entity in existing.iter() {
        commands.entity(entity).despawn();
    }

    if alive.is_empty() {
        return;
    }

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(alive.len() * 4);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(alive.len() * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(alive.len() * 6);

    for particle in &alive {
        let vi = positions.len() as u32;
        let s = particle.size;
        let x = particle.position.x;
        let y = particle.position.y;

        positions.extend_from_slice(&[
            [x - s, y - s, 0.6],
            [x + s, y - s, 0.6],
            [x + s, y + s, 0.6],
            [x - s, y + s, 0.6],
        ]);

        let c = particle.color;
        colors.extend_from_slice(&[c, c, c, c]);

        indices.extend_from_slice(&[vi, vi + 1, vi + 2, vi, vi + 2, vi + 3]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));

    // For initial version: use simple ColorMaterial or vertex-color rendering.
    // Metaball pass upgrade comes in Task 10.
    commands.spawn((
        ParticleMeshEntity,
        Mesh2d(meshes.add(mesh)),
        // TODO: Use metaball material in Task 10. For now, basic ColorMaterial.
        Transform::default(),
        Visibility::default(),
    ));
}
```

**Step 4: Register render system**

In `src/particles/mod.rs`, add:
```rust
pub mod render;
```

Add system:
```rust
.add_systems(
    Update,
    render::rebuild_particle_mesh
        .in_set(GameSet::WorldUpdate)
        .run_if(in_state(AppState::InGame)),
)
```

**Step 5: Run build**

Run: `cargo build`
Expected: compiles

**Step 6: Commit**

`feat(particles): basic sprite particle rendering (pre-metaball)`

---

### Task 10: Metaball rendering upgrade

Replace simple sprite rendering with two-pass metaball.

**Files:**
- Modify: `src/particles/render.rs` (offscreen render target + composite)
- The shaders from Task 9 Step 1-2 are already created

This task involves Bevy 0.18 render graph customization (creating custom render passes, offscreen textures, and fullscreen quad compositing). Due to complexity, implementation should:

1. Create an offscreen `Image` resource (half viewport size, `R16Float`)
2. Render particle quads into it using `particle_accum.wgsl`
3. Composite over main scene using `particle_composite.wgsl` as a fullscreen pass

**This is an advanced rendering task.** The exact Bevy 0.18 API for custom render passes should be verified against current docs before implementation. The basic sprite rendering from Task 9 provides a working fallback.

**Step 1:** Create `ParticleAccumMaterial` (Material2d for accumulation pass)
**Step 2:** Create offscreen render target resource
**Step 3:** System to render particles into offscreen target
**Step 4:** Create `ParticleCompositeMaterial` for fullscreen threshold pass
**Step 5:** Composite system that draws fullscreen quad
**Step 6:** Visual test — particles near each other should merge
**Step 7:** Commit

`feat(particles): metaball rendering with accumulation + threshold passes`

---

### Task 11: FluidDef wave parameters + RON update

Add per-fluid wave tuning to FluidDef.

**Files:**
- Modify: `src/fluid/registry.rs` (add wave fields to FluidDef)
- Modify: `assets/content/fluids/fluids.fluid.ron` (add wave params)
- Modify: `src/fluid/render.rs` (pass wave params to shader)

**Step 1: Extend FluidDef**

Add to `FluidDef`:
```rust
/// Wave amplitude multiplier (1.0 = default). Lava ~0.3, gas ~1.5.
#[serde(default = "default_wave_amplitude")]
pub wave_amplitude: f32,
/// Wave speed multiplier (1.0 = default). Lava ~0.3, gas ~2.0.
#[serde(default = "default_wave_speed")]
pub wave_speed: f32,

fn default_wave_amplitude() -> f32 { 1.0 }
fn default_wave_speed() -> f32 { 1.0 }
```

**Step 2: Update RON**

In `fluids.fluid.ron`, add per-fluid tuning:
- water: `wave_amplitude: 1.0, wave_speed: 1.0`
- lava: `wave_amplitude: 0.4, wave_speed: 0.3`
- steam: `wave_amplitude: 1.5, wave_speed: 2.0`
- toxic_gas: `wave_amplitude: 1.2, wave_speed: 1.5`
- smoke: `wave_amplitude: 0.8, wave_speed: 1.8`

**Step 3: Pass wave params through vertex attribute**

Extend ATTRIBUTE_FLUID_DATA to encode wave_amplitude and wave_speed (or use additional attributes). The shader uses these to scale the multi-octave ripple per fluid type.

**Step 4: Run tests**

Run: `cargo test fluid`
Expected: all PASS (serde defaults make old tests work)

**Step 5: Commit**

`feat(fluid): per-fluid wave amplitude and speed in FluidDef`

---

### Task 12: Final integration + visual testing

Wire everything together and verify.

**Files:**
- Modify: `src/fluid/mod.rs` (final system ordering)
- Modify: `src/fluid/debug.rs` (add F7 for lava, test splash manually)

**Step 1: Verify full system chain**

Final system ordering in FluidPlugin:
```
GameSet::WorldUpdate:
  detect_entity_water_entry
  detect_entity_swimming
  fluid_simulation
  wave_consume_events
  wave_simulation
  spawn_splash_particles
  reabsorb_particles
  fluid_rebuild_meshes
  rebuild_particle_mesh
  update_fluid_time
```

**Step 2: Visual test checklist**

- [ ] Place water (F5), observe multi-octave ripples
- [ ] Jump into water → splash particles fly upward
- [ ] Swim → small wake particles
- [ ] Particles fall back → reabsorbed into CA (water level unchanged over time)
- [ ] Place lava (F7) → slower, heavier waves
- [ ] Wave propagates from splash point outward
- [ ] No visual seams at chunk boundaries
- [ ] Performance: FPS stays above 30 with large water body

**Step 3: Commit**

`feat(fluid): hybrid water engine integration complete`

---

## Task Dependency Graph

```
Task 1 (Events) ──────┬──► Task 4 (Wave ECS) ──► Task 5 (Wave→Shader)
                       │
Task 2 (Shader ripple) ┘
                                                   Task 7 (CA↔Particle)
Task 3 (Wave buffer) ──► Task 4                        │
                                                       ▼
Task 6 (Particles) ────► Task 7 ──► Task 9 (Sprite render) ──► Task 10 (Metaball)
                                        │
Task 8 (Detectors) ────────────────────►│
                                        ▼
Task 11 (FluidDef) ──► Task 12 (Integration)
```

**Parallel-safe groups:**
- Tasks 1, 2, 3, 6 can be developed independently
- Task 4 needs 1 + 3
- Task 5 needs 4
- Task 7 needs 1 + 6
- Task 8 needs 1
- Task 9 needs 6
- Task 10 needs 9
- Task 11 independent
- Task 12 needs everything
