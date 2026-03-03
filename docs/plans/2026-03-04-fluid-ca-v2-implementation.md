# Fluid CA v2 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement stable CA-based fluid simulation with visual waves, instant reactions, smooth shader rendering, and splash particles.

**Architecture:** CA flow on global active chunk space, bilinear-filtered fluid data texture per chunk, visual-only spring waves, fixed tick rate.

**Tech Stack:** Rust, Bevy 0.15, WGSL shaders, Material2d

**Design doc:** `docs/plans/2026-03-04-fluid-ca-v2-design.md`

---

## Key Codebase References

| What | Where | API |
|------|-------|-----|
| FluidCell, FluidId | `src/fluid/cell.rs` | `FluidCell { fluid_id, mass }`, `FluidId(pub u8)` |
| FluidRegistry | `src/fluid/registry.rs` | `get(id)`, `by_name(s)`, `FluidDef { density, viscosity, color, ... }` |
| FluidReactionRegistry | `src/fluid/reactions.rs` | `reactions: Vec<CompiledReaction>` |
| WorldMap | `src/world/chunk.rs:109` | `chunk(cx,cy)`, `chunk_mut(cx,cy)`, `get_fluid(tx,ty,ctx)`, `set_fluid(tx,ty,cell,ctx)` |
| ActiveWorld | `src/registry/world.rs` | `chunk_size: u32`, `tile_size: f32`, `wrap_tile_x()`, `width_tiles`, `height_tiles` |
| Coord helpers | `src/world/chunk.rs:311-330` | `tile_to_chunk()`, `tile_to_local()`, `world_to_tile()` |
| ChunkData.fluids | `src/world/chunk.rs:86` | `pub fluids: Vec<FluidCell>` (size = chunk_size²) |
| Particle spawn | `src/particles/pool.rs:46` | `pool.spawn(pos, vel, mass, fluid_id, lifetime, size, color, grav, fade)` |
| Material2d pattern | `src/world/tile_renderer.rs` | `AsBindGroup`, `Material2d`, `ShaderRef` |
| Mesh building | `src/world/mesh_builder.rs` | quad per tile, positions+uvs+indices |
| Events | Bevy Messages | `#[derive(Message)]`, `MessageWriter<T>`, `MessageReader<T>` |
| Input | `Res<ButtonInput<KeyCode>>` | `.just_pressed(KeyCode::F5)` etc. |
| System set | `GameSet::WorldUpdate` | `.run_if(in_state(AppState::InGame))` |
| Z-order | bg=-1.0, fg=0.0, objects=0.5, particles=1.0 | Fluid should be z=0.25 (between fg tiles and objects) |

---

### Task 1: Simulation Resources & Tick Accumulator

**Files:**
- Create: `src/fluid/simulation.rs`
- Modify: `src/fluid/mod.rs`

**Step 1: Write simulation.rs with resources and tick system**

```rust
// src/fluid/simulation.rs
use bevy::prelude::*;
use std::collections::HashSet;

/// Configuration for fluid simulation.
#[derive(Resource, Debug, Clone)]
pub struct FluidSimConfig {
    /// Simulation ticks per second.
    pub tick_rate: f32,
    /// Minimum flow amount; below this, flow is skipped.
    pub min_flow: f32,
    /// Maximum mass a cell can hold before pressure builds.
    pub max_mass: f32,
}

impl Default for FluidSimConfig {
    fn default() -> Self {
        Self {
            tick_rate: 20.0,
            min_flow: 0.01,
            max_mass: 1.0,
        }
    }
}

/// Accumulates delta time and fires simulation ticks at fixed rate.
#[derive(Resource, Debug, Default)]
pub struct FluidTickAccumulator {
    pub accumulated: f32,
}

/// Set of chunk coordinates that contain fluid or neighbor fluid chunks.
/// Only these chunks are processed each tick.
#[derive(Resource, Debug, Default)]
pub struct ActiveFluidChunks {
    pub chunks: HashSet<(i32, i32)>,
}

/// Per-cell flag buffer reused each tick to mark cells as "settled"
/// (already swapped via density displacement, skip further processing).
#[derive(Resource, Debug, Default)]
pub struct SettledBuffer {
    pub settled: HashSet<(i32, i32)>,  // global tile coords
}

/// System: accumulates dt, returns whether a tick should fire this frame.
pub fn fluid_tick_accumulator(
    time: Res<Time>,
    config: Res<FluidSimConfig>,
    mut accum: ResMut<FluidTickAccumulator>,
) {
    accum.accumulated += time.delta_secs();
    // Clamped to max 3 ticks per frame to avoid spiral of death
    let tick_interval = 1.0 / config.tick_rate;
    accum.accumulated = accum.accumulated.min(tick_interval * 3.0);
}
```

**Step 2: Wire up in mod.rs**

```rust
// src/fluid/mod.rs — add:
pub mod simulation;

pub use simulation::{FluidSimConfig, FluidTickAccumulator, ActiveFluidChunks};
```

In `FluidPlugin::build`:
```rust
fn build(&self, app: &mut App) {
    app.init_resource::<FluidSimConfig>()
        .init_resource::<FluidTickAccumulator>()
        .init_resource::<ActiveFluidChunks>()
        .init_resource::<simulation::SettledBuffer>();
}
```

**Step 3: Test tick accumulator**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_accumulator_caps_at_3_ticks() {
        let config = FluidSimConfig::default(); // 20 Hz = 50ms interval
        let mut accum = FluidTickAccumulator { accumulated: 0.0 };
        // Simulate a 500ms frame (10 ticks worth)
        accum.accumulated += 0.5;
        let tick_interval = 1.0 / config.tick_rate;
        accum.accumulated = accum.accumulated.min(tick_interval * 3.0);
        assert!((accum.accumulated - 0.15).abs() < 0.001); // 3 * 50ms = 150ms
    }
}
```

**Step 4: `cargo check --tests`**

Run: `cargo check --tests`
Expected: PASS

**Step 5: Commit**

```
git add src/fluid/simulation.rs src/fluid/mod.rs
git commit -m "feat(fluid): add simulation resources and tick accumulator"
```

---

### Task 2: CA Flow Core — Single-Fluid Simulation

**Files:**
- Modify: `src/fluid/simulation.rs`

**Step 1: Write the core simulation function**

This is a pure function that takes world data and mutates it. No Bevy dependencies for testability.

```rust
use crate::fluid::cell::{FluidCell, FluidId};
use crate::fluid::registry::FluidRegistry;
use crate::world::chunk::{WorldMap, tile_to_chunk, tile_to_local};
use crate::registry::world::ActiveWorld;

/// Calculate how much fluid should be in the bottom cell of a vertical pair.
/// Returns the stable mass for the bottom cell.
fn stable_state_below(total_mass: f32, max_mass: f32, max_compress: f32) -> f32 {
    if total_mass <= max_mass {
        total_mass
    } else if total_mass < 2.0 * max_mass + max_compress {
        (max_mass * max_mass + total_mass * max_compress) / (max_mass + max_compress)
    } else {
        (total_mass + max_compress) / 2.0
    }
}

/// Run one tick of CA fluid simulation across all active chunks.
/// `even_tick` alternates L→R / R→L iteration.
pub fn simulate_fluids_tick(
    world_map: &mut WorldMap,
    active_world: &ActiveWorld,
    active_chunks: &ActiveFluidChunks,
    config: &FluidSimConfig,
    fluid_registry: &FluidRegistry,
    settled: &mut SettledBuffer,
    even_tick: bool,
) {
    settled.settled.clear();

    if active_chunks.chunks.is_empty() {
        return;
    }

    let cs = active_world.chunk_size as i32;

    // Compute bounding box of all active chunks in tile space
    let mut min_tx = i32::MAX;
    let mut max_tx = i32::MIN;
    let mut min_ty = i32::MAX;
    let mut max_ty = i32::MIN;
    for &(cx, cy) in &active_chunks.chunks {
        min_tx = min_tx.min(cx * cs);
        max_tx = max_tx.max((cx + 1) * cs - 1);
        min_ty = min_ty.min(cy * cs);
        max_ty = max_ty.max((cy + 1) * cs - 1);
    }

    // Build a WorldCtxRef-like accessor (simplified — uses ActiveWorld directly)
    // Iterate bottom-to-top for liquids
    for ty in min_ty..=max_ty {
        let x_range: Box<dyn Iterator<Item = i32>> = if even_tick {
            Box::new(min_tx..=max_tx)
        } else {
            Box::new((min_tx..=max_tx).rev())
        };

        for tx in x_range {
            // Skip cells outside active chunks
            let (cx, cy) = tile_to_chunk(
                active_world.wrap_tile_x(tx), ty, active_world.chunk_size
            );
            if !active_chunks.chunks.contains(&(cx, cy)) {
                continue;
            }
            if settled.settled.contains(&(tx, ty)) {
                continue;
            }

            let cell = read_fluid(world_map, tx, ty, active_world);
            if cell.is_empty() {
                continue;
            }

            let def = fluid_registry.get(cell.fluid_id);
            if def.is_gas {
                continue; // gases handled in separate pass
            }

            let mut remaining = cell.mass;

            // --- 1. Flow DOWN ---
            remaining = try_flow_vertical(
                world_map, active_world, config, fluid_registry, settled,
                tx, ty, tx, ty - 1, remaining, cell.fluid_id, def.max_compress, true,
            );

            // --- 2. Flow LEFT/RIGHT ---
            if remaining > config.min_flow {
                let (first_dx, second_dx) = if even_tick { (-1, 1) } else { (1, -1) };
                remaining = try_flow_horizontal(
                    world_map, active_world, config, fluid_registry, settled,
                    tx, ty, tx + first_dx, ty, remaining, cell.fluid_id,
                );
                if remaining > config.min_flow {
                    remaining = try_flow_horizontal(
                        world_map, active_world, config, fluid_registry, settled,
                        tx, ty, tx + second_dx, ty, remaining, cell.fluid_id,
                    );
                }
            }

            // --- 3. Flow UP (pressure) ---
            if remaining > config.max_mass {
                remaining = try_flow_vertical(
                    world_map, active_world, config, fluid_registry, settled,
                    tx, ty, tx, ty + 1, remaining, cell.fluid_id, def.max_compress, false,
                );
            }

            // Write remaining mass back
            write_fluid_mass(world_map, tx, ty, active_world, cell.fluid_id, remaining);
        }
    }
}
```

Helper functions `read_fluid`, `write_fluid_mass`, `try_flow_vertical`, `try_flow_horizontal` implement the flow rules from the design doc:

- `try_flow_vertical`: stable state formula for down, simple overflow for up. If destination has different fluid_id → density swap or reaction.
- `try_flow_horizontal`: equalization `(self - neighbor) / 4`. If different fluid_id → density swap or reaction.

**Step 2: Write tests for stable_state_below**

```rust
#[test]
fn stable_state_all_fits_below() {
    // Total 0.5, max 1.0 → all goes below
    assert!((stable_state_below(0.5, 1.0, 0.02) - 0.5).abs() < 0.001);
}

#[test]
fn stable_state_overflow_creates_pressure() {
    // Total 1.5, max 1.0 → below gets more than 1.0
    let result = stable_state_below(1.5, 1.0, 0.02);
    assert!(result > 1.0);
    assert!(result < 1.5);
}

#[test]
fn stable_state_large_total() {
    // Total 3.0, max 1.0 → roughly even split with compression
    let result = stable_state_below(3.0, 1.0, 0.02);
    assert!(result > 1.0);
    assert!(result < 2.0);
}
```

**Step 3: `cargo check --tests` then commit**

```
git commit -m "feat(fluid): implement CA flow core with stable state formula"
```

---

### Task 3: Density Displacement & Reactions

**Files:**
- Modify: `src/fluid/simulation.rs`

**Step 1: Implement density displacement in flow helpers**

Inside `try_flow_vertical` and `try_flow_horizontal`, when destination cell has different fluid_id:

```rust
// Pseudo-code for try_flow_vertical (down):
let dest = read_fluid(world_map, dest_tx, dest_ty, active_world);
if !dest.is_empty() && dest.fluid_id != src_fluid_id {
    // Check for reaction first
    if let Some(reaction) = find_reaction(fluid_registry, reaction_registry, src_fluid_id, dest.fluid_id) {
        execute_reaction(world_map, active_world, reaction, src_tx, src_ty, dest_tx, dest_ty);
        // reaction consumed mass, return updated remaining
        return read_fluid(world_map, src_tx, src_ty, active_world).mass;
    }

    // No reaction → density displacement
    let src_def = fluid_registry.get(src_fluid_id);
    let dest_def = fluid_registry.get(dest.fluid_id);
    if is_downward && src_def.density > dest_def.density {
        // Heavy sinks: full swap
        let src_cell = read_fluid(world_map, src_tx, src_ty, active_world);
        write_fluid(world_map, src_tx, src_ty, active_world, dest);
        write_fluid(world_map, dest_tx, dest_ty, active_world, src_cell);
        settled.settled.insert((src_tx, src_ty));
        settled.settled.insert((dest_tx, dest_ty));
        return dest.mass; // src now contains what was in dest
    }

    return remaining; // can't flow here
}
```

**Step 2: Implement reaction execution**

```rust
fn find_reaction<'a>(
    reaction_registry: &'a FluidReactionRegistry,
    fluid_a: FluidId,
    fluid_b: FluidId,
) -> Option<&'a CompiledReaction> {
    reaction_registry.reactions.iter().find(|r| {
        (r.fluid_a == fluid_a && r.fluid_b == fluid_b)
            || (r.fluid_a == fluid_b && r.fluid_b == fluid_a)
    })
}

fn execute_reaction(
    world_map: &mut WorldMap,
    active_world: &ActiveWorld,
    reaction: &CompiledReaction,
    tx_a: i32, ty_a: i32,
    tx_b: i32, ty_b: i32,
    tile_registry: &TileRegistry,
) {
    let mut cell_a = read_fluid(world_map, tx_a, ty_a, active_world);
    let mut cell_b = read_fluid(world_map, tx_b, ty_b, active_world);

    if cell_a.mass < reaction.min_mass_a || cell_b.mass < reaction.min_mass_b {
        return;
    }

    cell_a.mass -= reaction.consume_a;
    cell_b.mass -= reaction.consume_b;

    // Place result tile at B's position
    if let Some(tile_id) = reaction.result_tile {
        // set_tile at (tx_b, ty_b) — clears fluid in that cell
        cell_b = FluidCell::EMPTY;
        // world_map.set_tile(tx_b, ty_b, Layer::Fg, tile_id, ctx);
    }

    // Result fluid at B
    if let Some(fluid_id) = reaction.result_fluid {
        cell_b = FluidCell::new(fluid_id, cell_b.mass.max(0.1));
    }

    // Byproduct at A
    if let Some(bp_fluid) = reaction.byproduct_fluid {
        cell_a = FluidCell::new(bp_fluid, reaction.byproduct_mass);
    }

    // Clean up near-zero cells
    if cell_a.mass < 0.001 { cell_a = FluidCell::EMPTY; }
    if cell_b.mass < 0.001 { cell_b = FluidCell::EMPTY; }

    write_fluid(world_map, tx_a, ty_a, active_world, cell_a);
    write_fluid(world_map, tx_b, ty_b, active_world, cell_b);
}
```

**Step 3: Uncomment water+lava reaction in RON**

In `assets/content/fluids/fluids.fluid.ron`, uncomment the reaction block.

**Step 4: Test and commit**

```
git commit -m "feat(fluid): add density displacement and reaction execution"
```

---

### Task 4: Gas Flow (Inverted Y)

**Files:**
- Modify: `src/fluid/simulation.rs`

**Step 1: Add gas simulation pass**

After the liquid pass, run a separate pass for gases. Same algorithm but:
- Iterate **top-to-bottom** (reversed Y)
- "Down" = **up** (ty + 1)
- "Up (pressure)" = **down** (ty - 1)
- Skip non-gas fluids

```rust
pub fn simulate_gases_tick(
    world_map: &mut WorldMap,
    active_world: &ActiveWorld,
    active_chunks: &ActiveFluidChunks,
    config: &FluidSimConfig,
    fluid_registry: &FluidRegistry,
    settled: &mut SettledBuffer,
    even_tick: bool,
) {
    // Same bounding box as liquids
    // Iterate top-to-bottom
    for ty in (min_ty..=max_ty).rev() {
        // ... same horizontal iteration with alternation ...
        // For each gas cell:
        //   1. Flow UP (ty+1) using stable_state
        //   2. Flow LEFT/RIGHT using equalization
        //   3. Pressure DOWN (ty-1) if mass > max
    }
}
```

**Step 2: Test gas rises above liquid and commit**

```
git commit -m "feat(fluid): add gas simulation with inverted gravity"
```

---

### Task 5: Active Chunks Management & Bevy System

**Files:**
- Modify: `src/fluid/simulation.rs`

**Step 1: Write the main Bevy system that ties it all together**

```rust
pub fn fluid_simulation_system(
    time: Res<Time>,
    config: Res<FluidSimConfig>,
    mut accum: ResMut<FluidTickAccumulator>,
    mut active_chunks: ResMut<ActiveFluidChunks>,
    mut settled: ResMut<SettledBuffer>,
    mut world_map: ResMut<WorldMap>,
    active_world: Res<ActiveWorld>,
    fluid_registry: Option<Res<FluidRegistry>>,
    reaction_registry: Option<Res<FluidReactionRegistry>>,
    // TODO: tile_registry for reactions that place tiles
) {
    let Some(fluid_reg) = fluid_registry else { return; };

    let tick_interval = 1.0 / config.tick_rate;
    let mut tick_count = 0u32;

    while accum.accumulated >= tick_interval && tick_count < 3 {
        accum.accumulated -= tick_interval;
        tick_count += 1;

        let even = tick_count % 2 == 0;

        simulate_fluids_tick(
            &mut world_map, &active_world, &active_chunks,
            &config, &fluid_reg, &mut settled, even,
        );
        simulate_gases_tick(
            &mut world_map, &active_world, &active_chunks,
            &config, &fluid_reg, &mut settled, even,
        );
    }

    // Rebuild active chunks set: scan loaded chunks for fluid
    rebuild_active_chunks(&world_map, &active_world, &mut active_chunks);
}

fn rebuild_active_chunks(
    world_map: &WorldMap,
    active_world: &ActiveWorld,
    active: &mut ActiveFluidChunks,
) {
    active.chunks.clear();
    let cs = active_world.chunk_size;
    for (&(cx, cy), chunk) in &world_map.chunks {
        let has_fluid = chunk.fluids.iter().any(|c| !c.is_empty());
        if has_fluid {
            active.chunks.insert((cx, cy));
            // Also activate neighbors to receive flow
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let nx = active_world.wrap_chunk_x(cx + dx);
                    let ny = cy + dy;
                    if ny >= 0 && ny < active_world.height_chunks() {
                        active.chunks.insert((nx, ny));
                    }
                }
            }
        }
    }
}
```

**Step 2: Register in FluidPlugin**

```rust
app.add_systems(
    Update,
    (
        fluid_tick_accumulator,
        fluid_simulation_system,
    )
        .chain()
        .in_set(GameSet::WorldUpdate)
        .run_if(in_state(AppState::InGame)),
);
```

**Step 3: Test and commit**

```
git commit -m "feat(fluid): wire simulation system with active chunk management"
```

---

### Task 6: Wave System (Visual-Only Springs)

**Files:**
- Create: `src/fluid/wave.rs`
- Modify: `src/fluid/mod.rs`

**Step 1: Write wave.rs**

```rust
use bevy::prelude::*;
use std::collections::HashMap;

use crate::fluid::cell::FluidId;
use crate::fluid::registry::FluidRegistry;

#[derive(Debug, Clone, Default)]
pub struct WaveColumn {
    pub height: f32,     // displacement in tile fractions (-0.5..+0.5)
    pub velocity: f32,
}

#[derive(Resource, Debug, Default)]
pub struct WaveBuffer {
    pub columns: HashMap<(i32, i32), WaveColumn>,
}

const SPRING_K: f32 = 40.0;
const DAMPING: f32 = 0.92;
const WAVE_EPSILON: f32 = 0.001;

impl WaveBuffer {
    /// Add an impulse (velocity) at a position, affecting radius tiles.
    pub fn add_impulse(&mut self, tx: i32, ty: i32, velocity: f32, radius: i32) {
        for dx in -radius..=radius {
            let dist = dx.abs() as f32 / (radius as f32 + 1.0);
            let falloff = 1.0 - dist;
            if let Some(col) = self.columns.get_mut(&(tx + dx, ty)) {
                col.velocity += velocity * falloff;
            }
        }
    }
}

/// System: update wave springs every frame (not per tick, for smooth animation).
pub fn fluid_wave_update(
    time: Res<Time>,
    mut wave_buffer: ResMut<WaveBuffer>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 { return; }

    // Collect current heights for neighbor lookup
    let snapshot: HashMap<(i32, i32), f32> = wave_buffer.columns.iter()
        .map(|(&k, v)| (k, v.height))
        .collect();

    let mut to_remove = Vec::new();

    for (&(tx, ty), col) in wave_buffer.columns.iter_mut() {
        let left = snapshot.get(&(tx - 1, ty)).copied().unwrap_or(0.0);
        let right = snapshot.get(&(tx + 1, ty)).copied().unwrap_or(0.0);

        let force = SPRING_K * (left + right - 2.0 * col.height);
        col.velocity += force * dt;
        col.velocity *= DAMPING;
        col.height += col.velocity * dt;

        // Mark dead columns for removal
        if col.height.abs() < WAVE_EPSILON && col.velocity.abs() < WAVE_EPSILON {
            to_remove.push((tx, ty));
        }
    }

    for key in to_remove {
        wave_buffer.columns.remove(&key);
    }
}
```

**Step 2: Sync wave columns with fluid surface**

After simulation tick, scan surface cells and ensure WaveBuffer has entries for them. Add this to the simulation system or as a separate system running after simulation.

**Step 3: Test spring model and commit**

```
git commit -m "feat(fluid): add visual wave spring model"
```

---

### Task 7: FluidMaterial & WGSL Shader

**Files:**
- Create: `src/fluid/render.rs`
- Create: `assets/engine/shaders/fluid.wgsl`
- Modify: `src/fluid/mod.rs`
- Modify: `src/world/mod.rs` (register Material2dPlugin)

**Step 1: Write FluidMaterial**

```rust
// src/fluid/render.rs
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::sprite::Material2d;

#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct FluidMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub fluid_texture: Handle<Image>,    // per-chunk RGBA fluid data
    #[texture(2)]
    #[sampler(3)]
    pub lightmap: Handle<Image>,         // RC lighting output
    #[uniform(4)]
    pub lightmap_uv_rect: Vec4,          // affine transform world→lightmap UV
    #[uniform(5)]
    pub time: f32,                       // for ripple animation
    #[uniform(5)]
    pub _pad: [f32; 3],                  // padding to 16-byte alignment
}

impl Material2d for FluidMaterial {
    fn vertex_shader() -> ShaderRef {
        "engine/shaders/fluid.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "engine/shaders/fluid.wgsl".into()
    }

    fn specialize(
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        layout: &bevy::render::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::sprite::Material2dKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        // Enable alpha blending
        if let Some(fragment) = &mut descriptor.fragment {
            if let Some(target) = fragment.targets.first_mut().and_then(|t| t.as_mut()) {
                target.blend = Some(bevy::render::render_resource::BlendState::ALPHA_BLENDING);
            }
        }
        Ok(())
    }
}

/// Shared material handle, similar to SharedTileMaterial.
#[derive(Resource)]
pub struct SharedFluidMaterial {
    pub handle: Handle<FluidMaterial>,
}
```

**Step 2: Write fluid.wgsl shader**

```wgsl
// assets/engine/shaders/fluid.wgsl
#import bevy_sprite::mesh2d_functions

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world_pos: vec2<f32>,
};

@group(2) @binding(0) var fluid_texture: texture_2d<f32>;
@group(2) @binding(1) var fluid_sampler: sampler;  // bilinear
@group(2) @binding(2) var lightmap_texture: texture_2d<f32>;
@group(2) @binding(3) var lightmap_sampler: sampler;

struct FluidUniforms {
    lightmap_uv_rect: vec4<f32>,
    time: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};
@group(2) @binding(4) var<uniform> lm_xform: vec4<f32>;
@group(2) @binding(5) var<uniform> uniforms_block: vec4<f32>; // time in .x

@vertex
fn vertex(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world = mesh2d_functions::mesh2d_position_local_to_world(
        mesh2d_functions::get_world_from_local(in.instance_index),
        vec4<f32>(in.position, 1.0)
    );
    out.clip_position = mesh2d_functions::mesh2d_position_world_to_clip(world);
    out.world_pos = world.xy;
    out.uv = in.uv;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample fluid texture with bilinear filtering
    let fluid = textureSample(fluid_texture, fluid_sampler, in.uv);

    if (fluid.a < 0.01) {
        discard;
    }

    // Sample one pixel above to detect surface
    let texel_size = vec2<f32>(1.0 / f32(textureDimensions(fluid_texture).x),
                                1.0 / f32(textureDimensions(fluid_texture).y));
    let above = textureSample(fluid_texture, fluid_sampler, in.uv + vec2(0.0, texel_size.y));
    let is_surface = above.a < 0.01;

    var color = fluid;

    // Surface ripple: subtle alpha modulation
    if (is_surface) {
        let time = uniforms_block.x;
        let ripple = sin(time * 3.0 + in.world_pos.x * 0.3) * 0.03;
        color.a = clamp(color.a + ripple, 0.0, 1.0);
    }

    // Apply lightmap
    let lm = lm_xform;
    let lightmap_uv = in.world_pos * lm.xy + lm.zw;
    let light = textureSample(lightmap_texture, lightmap_sampler, lightmap_uv).rgb;
    color = vec4<f32>(color.rgb * light, color.a);

    return color;
}
```

**Step 3: Register material plugin**

In `src/world/mod.rs` add:
```rust
app.add_plugins(Material2dPlugin::<crate::fluid::render::FluidMaterial>::default());
```

**Step 4: Test compilation and commit**

```
git commit -m "feat(fluid): add FluidMaterial and WGSL fluid shader"
```

---

### Task 8: Fluid Texture Build & Chunk Mesh Spawning

**Files:**
- Modify: `src/fluid/render.rs`

**Step 1: Build per-chunk fluid data texture**

```rust
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::render_asset::RenderAssetUsages;

/// Marker component for fluid overlay entities.
#[derive(Component)]
pub struct FluidChunkEntity;

/// Tracks which chunk coords have spawned fluid entities.
#[derive(Resource, Default)]
pub struct FluidChunkEntities {
    pub map: HashMap<(i32, i32), Entity>,
}

/// Set of chunk coords whose fluid data changed this frame.
#[derive(Resource, Default)]
pub struct DirtyFluidChunks {
    pub chunks: HashSet<(i32, i32)>,
}

/// Build a 32×32 RGBA Image from chunk fluid data.
fn build_fluid_texture(
    chunk: &ChunkData,
    chunk_size: u32,
    fluid_registry: &FluidRegistry,
    wave_buffer: &WaveBuffer,
    chunk_tx0: i32,
    chunk_ty0: i32,
) -> Image {
    let cs = chunk_size as usize;
    let mut data = vec![0u8; cs * cs * 4];

    for ly in 0..cs {
        for lx in 0..cs {
            let idx = ly * cs + lx;
            let cell = chunk.fluids[idx];
            if cell.is_empty() { continue; }
            let def = fluid_registry.get(cell.fluid_id);
            let mass = cell.mass.min(1.0);
            let pixel = idx * 4;
            data[pixel] = def.color[0];
            data[pixel + 1] = def.color[1];
            data[pixel + 2] = def.color[2];
            data[pixel + 3] = (def.color[3] as f32 * mass) as u8;
        }
    }

    let mut image = Image::new(
        Extent3d { width: chunk_size, height: chunk_size, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    // Enable bilinear filtering
    image.sampler = bevy::image::ImageSampler::linear();
    image
}
```

**Step 2: System to rebuild dirty chunk textures and spawn/despawn entities**

```rust
pub fn fluid_texture_rebuild_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut fluid_materials: ResMut<Assets<FluidMaterial>>,
    mut fluid_entities: ResMut<FluidChunkEntities>,
    mut dirty: ResMut<DirtyFluidChunks>,
    world_map: Res<WorldMap>,
    active_world: Res<ActiveWorld>,
    fluid_registry: Option<Res<FluidRegistry>>,
    wave_buffer: Res<WaveBuffer>,
    time: Res<Time>,
    shared_material: Option<Res<SharedFluidMaterial>>,
) {
    let Some(fluid_reg) = fluid_registry else { return; };

    for &(cx, cy) in &dirty.chunks {
        let Some(chunk) = world_map.chunk(cx, cy) else { continue; };
        let has_fluid = chunk.fluids.iter().any(|c| !c.is_empty());

        if !has_fluid {
            // Despawn entity if no fluid
            if let Some(entity) = fluid_entities.map.remove(&(cx, cy)) {
                commands.entity(entity).despawn();
            }
            continue;
        }

        let cs = active_world.chunk_size;
        let ts = active_world.tile_size;
        let chunk_tx0 = cx * cs as i32;
        let chunk_ty0 = cy * cs as i32;

        let image = build_fluid_texture(
            chunk, cs, &fluid_reg, &wave_buffer, chunk_tx0, chunk_ty0,
        );
        let image_handle = images.add(image);

        // Create or update entity
        // ... (spawn with Mesh2d quad covering chunk area, FluidMaterial, z=0.25)
    }

    dirty.chunks.clear();
}
```

**Step 3: Mark dirty chunks during simulation**

In `simulate_fluids_tick`, whenever a cell is modified, insert its chunk coord into `DirtyFluidChunks`.

**Step 4: Test and commit**

```
git commit -m "feat(fluid): add fluid texture build and chunk mesh spawning"
```

---

### Task 9: Splash Detection & Particle Spawning

**Files:**
- Create: `src/fluid/splash.rs`
- Modify: `src/fluid/mod.rs`

**Step 1: Define SplashEvent**

```rust
use bevy::prelude::*;
use crate::fluid::cell::FluidId;

#[derive(Message, Debug)]
pub struct SplashEvent {
    pub position: Vec2,
    pub fluid_id: FluidId,
    pub intensity: f32,
}
```

**Step 2: Splash detection system**

```rust
pub fn fluid_splash_detection(
    world_map: Res<WorldMap>,
    active_world: Res<ActiveWorld>,
    mut query: Query<(&Transform, &Velocity, &mut FluidContactState)>,
    mut splash_events: MessageWriter<SplashEvent>,
) {
    for (transform, velocity, mut contact) in &mut query {
        let (tx, ty) = world_to_tile(
            transform.translation.x,
            transform.translation.y,
            active_world.tile_size,
        );

        let cell = world_map.get_fluid(tx, ty, /* ctx */)
            .unwrap_or(FluidCell::EMPTY);

        let current_fluid = if cell.is_empty() { FluidId::NONE } else { cell.fluid_id };

        if contact.last_fluid == FluidId::NONE && current_fluid != FluidId::NONE {
            // Just entered fluid — splash!
            let fall_speed = (-velocity.y).max(0.0);
            let intensity = (fall_speed / 500.0).min(1.0); // normalize
            if intensity > 0.05 {
                splash_events.send(SplashEvent {
                    position: transform.translation.truncate(),
                    fluid_id: current_fluid,
                    intensity,
                });
            }
        }

        contact.last_fluid = current_fluid;
    }
}
```

**Step 3: Splash spawn system**

```rust
pub fn fluid_splash_spawn(
    mut splash_events: MessageReader<SplashEvent>,
    mut pool: ResMut<ParticlePool>,
    mut wave_buffer: ResMut<WaveBuffer>,
    fluid_registry: Option<Res<FluidRegistry>>,
    active_world: Res<ActiveWorld>,
) {
    let Some(fluid_reg) = fluid_registry else { return; };

    for event in splash_events.read() {
        let def = fluid_reg.get(event.fluid_id);
        let color = [
            def.color[0] as f32 / 255.0,
            def.color[1] as f32 / 255.0,
            def.color[2] as f32 / 255.0,
            def.color[3] as f32 / 255.0,
        ];

        let count = (event.intensity * 12.0) as usize;
        for i in 0..count {
            let angle = std::f32::consts::PI * (0.2 + 0.6 * i as f32 / count as f32);
            let speed = 80.0 + event.intensity * 200.0;
            let vel = Vec2::new(angle.cos() * speed, angle.sin() * speed);

            pool.spawn(
                event.position,
                vel,
                0.0,                // visual-only mass
                event.fluid_id,
                0.3 + event.intensity * 0.5,  // lifetime
                2.0 + event.intensity * 2.0,  // size
                color,
                1.0,  // gravity
                true, // fade out
            );
        }

        // Wave impulse
        let (tx, ty) = world_to_tile(
            event.position.x, event.position.y, active_world.tile_size,
        );
        wave_buffer.add_impulse(tx, ty, -event.intensity * 8.0, 3);
    }
}
```

**Step 4: Test and commit**

```
git commit -m "feat(fluid): add splash detection and particle spawning"
```

---

### Task 10: Debug Fluid Placement

**Files:**
- Create: `src/fluid/debug.rs`
- Modify: `src/fluid/mod.rs`

**Step 1: Write debug state and input system**

```rust
use bevy::prelude::*;
use crate::fluid::cell::{FluidCell, FluidId};
use crate::fluid::registry::FluidRegistry;

#[derive(Resource, Debug, Default)]
pub struct FluidDebugState {
    pub active: bool,
    pub selected_index: usize,  // index into FluidRegistry
    pub brush_mass: f32,
}

impl FluidDebugState {
    pub fn new() -> Self {
        Self { active: false, selected_index: 0, brush_mass: 1.0 }
    }
}

pub fn fluid_debug_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut debug_state: ResMut<FluidDebugState>,
    mut world_map: ResMut<WorldMap>,
    active_world: Res<ActiveWorld>,
    fluid_registry: Option<Res<FluidRegistry>>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window>,
    mut dirty: ResMut<DirtyFluidChunks>,
) {
    if keyboard.just_pressed(KeyCode::F5) {
        debug_state.active = !debug_state.active;
    }
    if !debug_state.active { return; }

    let Some(fluid_reg) = fluid_registry else { return; };

    if keyboard.just_pressed(KeyCode::F6) {
        debug_state.selected_index = (debug_state.selected_index + 1) % fluid_reg.len();
    }
    if keyboard.just_pressed(KeyCode::F7) {
        debug_state.selected_index = debug_state.selected_index
            .checked_sub(1).unwrap_or(fluid_reg.len() - 1);
    }

    // Mouse click → place/remove fluid
    let (camera, cam_transform) = camera_query.single();
    let window = windows.single();
    if let Some(cursor_pos) = window.cursor_position() {
        if let Ok(world_pos) = camera.viewport_to_world_2d(cam_transform, cursor_pos) {
            let (tx, ty) = world_to_tile(world_pos.x, world_pos.y, active_world.tile_size);

            if mouse.pressed(MouseButton::Left) {
                let fluid_id = FluidId((debug_state.selected_index + 1) as u8);
                let cell = FluidCell::new(fluid_id, debug_state.brush_mass);
                // world_map.set_fluid(tx, ty, cell, &ctx_ref);
                let (cx, cy) = tile_to_chunk(tx, ty, active_world.chunk_size);
                dirty.chunks.insert((cx, cy));
            }
            if mouse.pressed(MouseButton::Right) {
                // world_map.set_fluid(tx, ty, FluidCell::EMPTY, &ctx_ref);
                let (cx, cy) = tile_to_chunk(tx, ty, active_world.chunk_size);
                dirty.chunks.insert((cx, cy));
            }
        }
    }
}
```

**Step 2: Register and commit**

```
git commit -m "feat(fluid): add debug fluid placement (F5/F6/F7)"
```

---

### Task 11: Full Integration & System Registration

**Files:**
- Modify: `src/fluid/mod.rs` — full FluidPlugin with all systems
- Modify: `src/world/rc_lighting.rs` — patch FluidMaterial lightmap (add to `update_tile_lightmap`)

**Step 1: Complete FluidPlugin**

```rust
impl Plugin for FluidPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FluidSimConfig>()
            .init_resource::<FluidTickAccumulator>()
            .init_resource::<ActiveFluidChunks>()
            .init_resource::<simulation::SettledBuffer>()
            .init_resource::<wave::WaveBuffer>()
            .init_resource::<render::FluidChunkEntities>()
            .init_resource::<render::DirtyFluidChunks>()
            .insert_resource(debug::FluidDebugState::new())
            .add_event::<splash::SplashEvent>()
            .add_systems(
                Update,
                (
                    simulation::fluid_tick_accumulator,
                    simulation::fluid_simulation_system,
                    wave::fluid_wave_update,
                    splash::fluid_splash_detection,
                    splash::fluid_splash_spawn,
                    render::fluid_texture_rebuild_system,
                    debug::fluid_debug_input,
                )
                    .chain()
                    .in_set(GameSet::WorldUpdate)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
```

**Step 2: Patch lightmap for FluidMaterial in rc_lighting.rs**

In `update_tile_lightmap`, add a loop to update FluidMaterial assets:
```rust
// After existing lit_sprite_materials loop:
for (_id, mat) in fluid_materials.iter_mut() {
    mat.lightmap = gpu_images.lightmap.clone();
    mat.lightmap_uv_rect = lm_params;
}
```

**Step 3: `cargo check --tests` and full build verification**

Run: `cargo check --tests`
Expected: PASS with only pre-existing warnings

**Step 4: Commit**

```
git commit -m "feat(fluid): complete FluidPlugin integration with all systems"
```

---

### Task 12: End-to-End Testing

**Step 1: Launch game, press F5, place water, verify it flows**

**Step 2: Place lava near water, verify instant reaction (stone + steam)**

**Step 3: Fall into water, verify splash particles and wave ripples**

**Step 4: Verify lighting looks correct through water**

**Step 5: Fix any issues found during testing**

**Step 6: Final commit**

```
git commit -m "fix(fluid): address issues found during end-to-end testing"
```
