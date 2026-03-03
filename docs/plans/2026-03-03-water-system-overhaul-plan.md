# Water System Overhaul Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Rewrite the fluid visual pipeline (shader, lighting, reactions, particles) while keeping the working CA simulation core.

**Architecture:** Enhanced single-pass shader approach. One custom `FluidMaterial`, one mesh per chunk, one fragment shader handles all visual effects (waves, caustics, depth darkening, foam, emission). CA simulation stays on CPU. RC lighting integration adds fluid emission and absorption. Runtime reaction system activates the existing `FluidReactionRegistry`.

**Tech Stack:** Rust, Bevy 0.18, WGSL shaders, Radiance Cascades lighting

**Design doc:** `docs/plans/2026-03-03-water-system-overhaul-design.md`

---

## Phase 1: Shader + Waves

### Task 1: Add `ATTRIBUTE_EDGE_FLAGS` to mesh builder

New vertex attribute encoding which sides of a fluid cell border solid tiles or air. Used by the shader for foam rendering.

**Files:**
- Modify: `src/fluid/render.rs:19-29` (add new attribute constant)
- Modify: `src/fluid/render.rs:210-452` (compute and emit edge flags in `build_fluid_mesh`)
- Modify: `src/fluid/systems.rs:55-70` (register new attribute in `specialize()`)
- Test: `src/fluid/render.rs` (existing test module, add new tests)

**Step 1: Define the new attribute constant**

In `src/fluid/render.rs`, after `ATTRIBUTE_WAVE_PARAMS` (line 28):

```rust
/// Per-vertex edge flags: bitflags indicating which sides border solid tiles or air.
/// Bit 0 = left solid, Bit 1 = right solid, Bit 2 = top air/empty, Bit 3 = bottom solid.
/// Used by shader for shore foam effect.
pub const ATTRIBUTE_EDGE_FLAGS: MeshVertexAttribute =
    MeshVertexAttribute::new("EdgeFlags", 982301570, VertexFormat::Float32);
```

**Step 2: Add edge_flags computation to `build_fluid_mesh`**

In `build_fluid_mesh()`, add a new `edge_flags_data: Vec<f32>` alongside existing attribute vectors. For each cell, compute:

```rust
fn compute_edge_flags(
    fluids: &[FluidCell],
    tiles: &[TileId],
    local_x: u32,
    local_y: u32,
    chunk_size: u32,
    tile_registry: &TileRegistry,
) -> f32 {
    let mut flags: u32 = 0;
    // Left
    if local_x > 0 {
        let left_idx = (local_y * chunk_size + local_x - 1) as usize;
        if tile_registry.is_solid(tiles[left_idx]) { flags |= 1; }
    } else {
        flags |= 1; // chunk boundary treated as solid for foam
    }
    // Right
    if local_x + 1 < chunk_size {
        let right_idx = (local_y * chunk_size + local_x + 1) as usize;
        if tile_registry.is_solid(tiles[right_idx]) { flags |= 2; }
    } else {
        flags |= 2;
    }
    // Above is air/empty (for surface foam)
    if local_y + 1 < chunk_size {
        let above_idx = ((local_y + 1) * chunk_size + local_x) as usize;
        if fluids[above_idx].is_empty() && !tile_registry.is_solid(tiles[above_idx]) {
            flags |= 4;
        }
    } else {
        flags |= 4;
    }
    // Below solid
    if local_y > 0 {
        let below_idx = ((local_y - 1) * chunk_size + local_x) as usize;
        if tile_registry.is_solid(tiles[below_idx]) { flags |= 8; }
    }
    flags as f32
}
```

Note: `build_fluid_mesh` needs a new parameter `tiles: &[TileId]` (the chunk's foreground tiles) and `tile_registry: &TileRegistry`.

**Step 3: Update `build_fluid_mesh` signature and callers**

Add `tiles: &[TileId]` and `tile_registry: &TileRegistry` parameters to `build_fluid_mesh`. Update the call in `fluid_rebuild_meshes` (`systems.rs:324`) to pass `&chunk.fg.tiles` and `&tile_registry`.

**Step 4: Emit edge_flags into mesh**

Add `edge_flags_data` vec, push 4 identical values per quad (same flags for all 4 vertices), insert as mesh attribute.

**Step 5: Register in `specialize()`**

In `systems.rs:60-68`, add `ATTRIBUTE_EDGE_FLAGS.at_shader_location(6)` to the vertex layout.

**Step 6: Write tests**

```rust
#[test]
fn edge_flags_solid_left_neighbor() {
    // Test that a fluid cell with a solid tile to its left gets bit 0 set
}

#[test]
fn edge_flags_no_solid_neighbors() {
    // Test that a fluid cell surrounded by air gets flags = 4 (above is air)
}
```

**Step 7: Run tests**

Run: `cargo test --lib fluid::render`
Expected: All existing tests pass + new tests pass.

**Step 8: Commit**

```
git add src/fluid/render.rs src/fluid/systems.rs
git commit -m "feat(fluid): add ATTRIBUTE_EDGE_FLAGS for shore foam detection"
```

---

### Task 2: Restructure UV attributes for depth-in-fluid

Currently `UV_0` carries `[fill_level, local_y_fraction]` for surface cells and `[fill, depth]` for non-surface cells. The new shader needs `depth_in_fluid` available on ALL cells and `fill` separately. Restructure: `UV_0.x = fill`, `UV_0.y = depth_in_fluid` for all cells. Surface wave animation moves to vertex displacement (Task 3) so the fragment shader no longer needs local_y in UV.

**Files:**
- Modify: `src/fluid/render.rs:391-416` (UV computation)
- Test: `src/fluid/render.rs` (update existing UV tests)

**Step 1: Simplify UV emission**

Replace the conditional UV emission (lines 407-416) with uniform depth-based UVs for ALL cells:

```rust
// UV_0: [fill_level, depth_in_fluid] — same for all vertex types
uvs.extend_from_slice(&[uv, uv, uv, uv]);
```

This means removing the special `is_surface && !def.is_gas` branch that emits per-vertex local_y gradient.

**Step 2: Update affected tests**

The test `uv0_contains_fill_and_depth` (line 670) currently expects surface cells to have local_y gradient. Update to expect uniform `[fill, depth]`.

**Step 3: Run tests**

Run: `cargo test --lib fluid::render`

**Step 4: Commit**

```
git commit -m "refactor(fluid): unify UV_0 to [fill, depth] for all cells"
```

---

### Task 3: Rewrite `fluid.wgsl` — vertex displacement + visual effects

Complete shader rewrite. The fragment-discard wave approach is replaced with vertex displacement. New effects: depth darkening, procedural caustics, surface glint, shore foam.

**Files:**
- Rewrite: `assets/engine/shaders/fluid.wgsl`
- Modify: `src/fluid/systems.rs:31-71` (FluidMaterial struct, specialize)

**Step 1: Update shader input structs**

```wgsl
struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,           // [fill_level, depth_in_fluid]
    @location(3) fluid_data: vec4<f32>,   // [emission_r, emission_g, emission_b, flags]
    @location(4) wave_height: f32,        // dynamic wave from propagation sim
    @location(5) wave_params: vec2<f32>,  // [amplitude_multiplier, speed_multiplier]
    @location(6) edge_flags: f32,         // bitflags for solid neighbors
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world_pos: vec2<f32>,
    @location(2) uv: vec2<f32>,           // [fill, depth]
    @location(3) fluid_data: vec4<f32>,
    @location(4) wave_params: vec2<f32>,
    @location(5) edge_flags: f32,
}
```

**Step 2: Vertex shader with wave displacement**

```wgsl
@vertex
fn vertex(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);

    var pos = in.position;

    // Wave vertex displacement (surface vertices only)
    let flags = in.fluid_data.w;
    let is_wave = (flags % 2.0) >= 0.5;
    let is_gas = flags >= 1.5;
    let amp = in.wave_params.x;
    let speed = in.wave_params.y;

    if is_wave {
        let world_x = (world_from_local * vec4<f32>(pos, 1.0)).x;
        // Physics-driven wave from propagation sim
        pos.y += in.wave_height;
        // Procedural waves: 2 octaves
        let w1 = sin(world_x * 0.10 + uniforms.time * 1.2 * speed) * 0.5;
        let w2 = sin(world_x * 0.22 - uniforms.time * 1.7 * speed) * 0.3;
        pos.y += (w1 + w2) * amp * 2.0; // scale to world units
    }

    let world_pos = (world_from_local * vec4<f32>(pos, 1.0)).xy;
    out.clip_position = mesh_functions::mesh2d_position_local_to_clip(
        world_from_local, vec4<f32>(pos, 1.0),
    );
    out.color = in.color;
    out.world_pos = world_pos;
    out.uv = in.uv;
    out.fluid_data = in.fluid_data;
    out.wave_params = in.wave_params;
    out.edge_flags = in.edge_flags;
    return out;
}
```

**Step 3: Fragment shader with layered effects**

```wgsl
// Voronoi noise for caustics (simple 2D cell noise)
fn hash2(p: vec2<f32>) -> vec2<f32> {
    let k = vec2<f32>(0.3183099, 0.3678794);
    var q = p * k + k.yx;
    q = fract(sin(q * 715.836) * 349.572);
    return q;
}

fn voronoi(uv: vec2<f32>) -> f32 {
    let cell = floor(uv);
    let frac = fract(uv);
    var min_dist: f32 = 1.0;
    for (var y: i32 = -1; y <= 1; y++) {
        for (var x: i32 = -1; x <= 1; x++) {
            let neighbor = vec2<f32>(f32(x), f32(y));
            let point = hash2(cell + neighbor);
            let diff = neighbor + point - frac;
            min_dist = min(min_dist, dot(diff, diff));
        }
    }
    return sqrt(min_dist);
}

fn caustic(uv: vec2<f32>, time: f32) -> f32 {
    let PIXEL_DENSITY = 8.0;
    let puv = floor(uv * PIXEL_DENSITY) / PIXEL_DENSITY;
    let c1 = voronoi(puv * 3.0 + vec2<f32>(time * 0.4, time * 0.3));
    let c2 = voronoi(puv * 5.0 - vec2<f32>(time * 0.2, time * 0.5));
    return smoothstep(0.3, 0.0, min(c1, c2));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = in.color;

    let flags = in.fluid_data.w;
    let is_surface = (flags % 2.0) >= 0.5;
    let is_gas = flags >= 1.5;
    let emission = in.fluid_data.xyz;
    let fill = in.uv.x;
    let depth = in.uv.y;
    let amp = in.wave_params.x;
    let edge = u32(in.edge_flags);

    // 1. Depth darkening (liquids only)
    if !is_gas {
        let darken = clamp(depth * 0.4, 0.0, 0.65);
        color = vec4<f32>(color.rgb * (1.0 - darken), color.a);
    }

    // 2. Caustics (liquids, shallow depth only)
    if !is_gas && depth < 0.5 {
        let caustic_uv = in.world_pos / 32.0; // tile_size-relative
        let c = caustic(caustic_uv, uniforms.time);
        let caustic_strength = clamp(1.0 - depth * 2.0, 0.0, 0.35);
        color = vec4<f32>(color.rgb + c * caustic_strength * vec3<f32>(0.6, 0.8, 1.0), color.a);
    }

    // 3. Shimmer
    let shimmer = 1.0 + 0.05 * sin(in.world_pos.x * 0.5 + uniforms.time * 0.8);
    color = vec4<f32>(color.rgb * shimmer, color.a);

    // 4. Surface effects
    if is_surface && !is_gas {
        // Surface glint — thin bright band
        let glint = 0.25 * amp;
        color = vec4<f32>(min(color.rgb + glint * 0.3, vec3<f32>(1.0)), color.a);

        // Shore foam — whitish where bordering solid
        let has_solid_side = (edge & 1u) != 0u || (edge & 2u) != 0u;
        let has_solid_below = (edge & 8u) != 0u;
        if has_solid_side || has_solid_below {
            let foam_t = 0.15 + 0.05 * sin(in.world_pos.x * 2.0 + uniforms.time * 1.5);
            color = vec4<f32>(mix(color.rgb, vec3<f32>(0.9, 0.95, 1.0), foam_t), color.a);
        }
    }

    // 5. Lightmap
    let lm_scale = uniforms.lightmap_uv_rect.xy;
    let lm_offset = uniforms.lightmap_uv_rect.zw;
    let lm_uv = in.world_pos * lm_scale + lm_offset;
    let light = textureSample(lightmap_texture, lightmap_sampler, lm_uv).rgb;
    color = vec4<f32>(color.rgb * light, color.a);

    // 6. Emission glow
    color = vec4<f32>(max(color.rgb, emission), color.a);

    return color;
}
```

**Step 4: Update FluidMaterial specialize**

Already handled in Task 1 (edge_flags at location 6).

**Step 5: Visual test**

Run the game: `cargo run` — place water and lava, verify:
- Waves move vertices (no fragment discard)
- Depth darkening visible
- Caustic patterns on shallow water
- Shore foam where water meets blocks
- Lava glows

**Step 6: Commit**

```
git commit -m "feat(fluid): rewrite fluid.wgsl with vertex displacement, caustics, foam, depth darkening"
```

---

### Task 4: Fix wave_height scale in mesh builder

Currently `wave_height` from the wave propagation system is passed as a raw float but the mesh builder doesn't scale it to world units. The wave buffer stores abstract height values, but vertices need displacement in pixel/world units.

**Files:**
- Modify: `src/fluid/render.rs:424-426` (wave_data emission)

**Step 1: Scale wave_height by tile_size**

```rust
let wave_h = wave_heights.map(|wh| wh[idx] * tile_size).unwrap_or(0.0);
```

This ensures `wave_height` in the shader is in world units, matching vertex positions.

**Step 2: Run tests + visual check**

Run: `cargo test --lib fluid::render && cargo run`
Place water, create splashes — wave propagation should now be visible.

**Step 3: Commit**

```
git commit -m "fix(fluid): scale wave_height to world units for vertex displacement"
```

---

## Phase 2: RC Lighting Integration

### Task 5: Add `light_absorption` field to FluidDef

**Files:**
- Modify: `src/fluid/registry.rs:34-58` (FluidDef struct)
- Modify: `assets/content/fluids/fluids.fluid.ron`
- Test: `src/fluid/registry.rs` (update test fixtures)

**Step 1: Add field with serde default**

In `registry.rs`, add default function and field:

```rust
fn default_light_absorption() -> f32 {
    0.0
}

// In FluidDef struct:
/// How much this fluid blocks light (0.0 = transparent, 1.0 = opaque).
/// Used by RC lighting to attenuate light through fluid.
#[serde(default = "default_light_absorption")]
pub light_absorption: f32,
```

**Step 2: Update RON definitions**

Add `light_absorption` to each fluid in `fluids.fluid.ron`:
- water: 0.3
- lava: 0.8
- steam: 0.05
- toxic_gas: 0.1
- smoke: 0.4

**Step 3: Update test fixtures**

Add `light_absorption: 0.0` (or appropriate) to ALL `FluidDef` test constructors across `registry.rs`, `render.rs`, `reactions.rs`, `simulation.rs` test modules.

**Step 4: Run tests**

Run: `cargo test --lib`
Expected: All tests pass (serde default handles missing field in existing code).

**Step 5: Commit**

```
git commit -m "feat(fluid): add light_absorption field to FluidDef"
```

---

### Task 6: Fluid emission into RC pipeline

Make emissive fluids (lava) contribute light to the radiance cascades system.

**Files:**
- Modify: `src/world/rc_lighting.rs:610-693` (after object emissive section)
- Modify: `src/world/rc_lighting.rs:270` (add FluidRegistry to system params)

**Step 1: Add FluidRegistry to `extract_lighting_data` parameters**

Add `fluid_registry: Option<Res<FluidRegistry>>` to the system signature at line 270.

**Step 2: Add fluid emissive pass after object emissive (after line 690)**

```rust
// --- Fluid emissive (iterate by chunk) ---
if let Some(ref fluid_reg) = fluid_registry {
    let cs = world_config.chunk_size as i32;
    let cs_u = world_config.chunk_size;
    let clamp_min_ty = min_ty.max(0);
    let clamp_max_ty = max_ty.min(height_tiles - 1);
    if clamp_min_ty <= clamp_max_ty {
        let fl_min_cy = clamp_min_ty.div_euclid(cs);
        let fl_max_cy = clamp_max_ty.div_euclid(cs);
        let fl_min_cx = min_tx.div_euclid(cs);
        let fl_max_cx = max_tx.div_euclid(cs);

        for cy in fl_min_cy..=fl_max_cy {
            for cx in fl_min_cx..=fl_max_cx {
                let data_cx = world_config.wrap_chunk_x(cx);
                let Some(chunk) = world_map.chunk(data_cx, cy) else { continue };
                let chunk_tx0 = cx * cs;
                let chunk_ty0 = cy * cs;
                let tx0 = chunk_tx0.max(min_tx);
                let tx1 = (chunk_tx0 + cs).min(max_tx + 1);
                let ty0 = chunk_ty0.max(clamp_min_ty);
                let ty1 = (chunk_ty0 + cs).min(clamp_max_ty + 1);

                for ty in ty0..ty1 {
                    let ly = (ty - chunk_ty0) as u32;
                    for tx in tx0..tx1 {
                        let lx = (tx - chunk_tx0) as u32;
                        let fidx = (ly * cs_u + lx) as usize;
                        let cell = chunk.fluids[fidx];
                        if cell.is_empty() { continue; }
                        let def = fluid_reg.get(cell.fluid_id);
                        if def.light_emission == [0, 0, 0] { continue; }
                        if cell.mass < 0.1 { continue; }

                        let buf_x = (tx - min_tx) as u32;
                        let buf_y = (max_ty - ty) as u32;
                        let idx = (buf_y * input_w + buf_x) as usize;
                        let intensity = cell.mass.min(1.0);
                        let e = def.light_emission;
                        // Only overwrite if stronger than existing emitter
                        let new_emission = [
                            e[0] as f32 / 255.0 * POINT_LIGHT_BOOST * intensity,
                            e[1] as f32 / 255.0 * POINT_LIGHT_BOOST * intensity,
                            e[2] as f32 / 255.0 * POINT_LIGHT_BOOST * intensity,
                            1.0,
                        ];
                        let existing = input.emissive[idx];
                        if new_emission[0] > existing[0]
                            || new_emission[1] > existing[1]
                            || new_emission[2] > existing[2]
                        {
                            input.emissive[idx] = new_emission;
                        }
                    }
                }
            }
        }
    }
}
```

**Step 3: Visual test**

Run: `cargo run` — place lava in a dark cave, verify it illuminates surrounding tiles through RC.

**Step 4: Commit**

```
git commit -m "feat(lighting): fluid emission into RC pipeline — lava illuminates caves"
```

---

### Task 7: Fluid light absorption in RC density

Make fluid cells partially block light in the RC density texture.

**Files:**
- Modify: `src/world/rc_lighting.rs:478-492` (density rebuild section)
- Modify: `src/world/rc_lighting.rs:270` (add FluidRegistry already done in Task 6)

**Step 1: After tile density, overlay fluid density**

After the tile density loop (line 492), add a second pass over chunks for fluid density. OR integrate into the existing chunk iteration that builds `cache.fg`/`cache.bg` (lines 438-476). The cleanest approach: after the density/albedo rebuild loop (481-492), do a second chunk iteration:

```rust
// Overlay fluid density on top of tile density
if let Some(ref fluid_reg) = fluid_registry {
    let cs = world_config.chunk_size as i32;
    let cs_u = world_config.chunk_size;
    let clamp_min_ty = min_ty.max(0);
    let clamp_max_ty = max_ty.min(height_tiles - 1);
    if clamp_min_ty <= clamp_max_ty {
        // same chunk iteration pattern as above
        for cy in grid_min_cy..=grid_max_cy {
            for cx in grid_min_cx..=grid_max_cx {
                let data_cx = world_config.wrap_chunk_x(cx);
                let Some(chunk) = world_map.chunk(data_cx, cy) else { continue };
                // ... iterate cells, for non-empty fluids:
                // let absorption = cell.mass.min(1.0) * def.light_absorption;
                // input.density[idx] = input.density[idx].max((absorption * 255.0) as u8);
            }
        }
    }
}
```

Only applies where tile density is 0 (air) — fluids don't make solid tiles more opaque.

**Step 2: Visual test**

Run: `cargo run` — deep water should be darker (light attenuated by RC).

**Step 3: Commit**

```
git commit -m "feat(lighting): fluid light absorption in RC density texture"
```

---

## Phase 3: Reactions

### Task 8: Implement `execute_fluid_reactions` system

**Files:**
- Modify: `src/fluid/reactions.rs` (add new system function)
- Modify: `src/fluid/mod.rs:42-57` (register in system chain)
- Modify: `src/fluid/systems.rs:131-195` (call reactions after CA iteration)
- New event: `src/fluid/events.rs` (add `FluidReactionEvent`)
- Test: `src/fluid/reactions.rs` (add integration tests)

**Step 1: Add `FluidReactionEvent` to events.rs**

```rust
/// Emitted when a fluid reaction occurs, for VFX systems to consume.
#[derive(Event, Debug, Clone)]
pub struct FluidReactionEvent {
    pub position: Vec2,
    pub fluid_a: FluidId,
    pub fluid_b: FluidId,
    pub result_tile: Option<TileId>,
    pub result_fluid: Option<FluidId>,
}
```

**Step 2: Write `execute_fluid_reactions` function in reactions.rs**

```rust
/// Maximum reactions per chunk per tick (rate limiting).
const MAX_REACTIONS_PER_CHUNK: u32 = 8;

/// Process fluid reactions for a single chunk's fluid grid.
/// Modifies fluids in-place, sets tiles via callback, returns reaction events.
pub fn execute_fluid_reactions(
    fluids: &mut [FluidCell],
    tiles: &mut [TileId],
    width: u32,
    height: u32,
    reaction_registry: &FluidReactionRegistry,
    tile_registry: &TileRegistry,
    chunk_x: i32,
    chunk_y: i32,
    tile_size: f32,
) -> Vec<FluidReactionEvent> {
    let mut events = Vec::new();
    let mut reaction_count: u32 = 0;

    for y in 0..height {
        for x in 0..width {
            if reaction_count >= MAX_REACTIONS_PER_CHUNK { return events; }
            let idx = (y * width + x) as usize;
            let cell = fluids[idx];
            if cell.is_empty() { continue; }

            // Check 4 neighbors
            let neighbors: [(i32, i32, Adjacency); 4] = [
                (0, -1, Adjacency::Below),
                (0, 1, Adjacency::Above),
                (-1, 0, Adjacency::Side),
                (1, 0, Adjacency::Side),
            ];

            for (dx, dy, adj) in &neighbors {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || nx >= width as i32 || ny < 0 || ny >= height as i32 {
                    continue;
                }
                let nidx = (ny as u32 * width + nx as u32) as usize;
                let neighbor = fluids[nidx];
                if neighbor.is_empty() || neighbor.fluid_id == cell.fluid_id {
                    continue;
                }

                let Some(reaction) = reaction_registry.find_reaction(
                    cell.fluid_id, neighbor.fluid_id, adj
                ) else { continue };

                // Check minimum mass requirements
                let (a_idx, b_idx) = if cell.fluid_id == reaction.fluid_a {
                    (idx, nidx)
                } else {
                    (nidx, idx)
                };
                if fluids[a_idx].mass < reaction.min_mass_a { continue; }
                if fluids[b_idx].mass < reaction.min_mass_b { continue; }

                // Execute reaction
                fluids[a_idx].mass -= reaction.consume_a;
                fluids[b_idx].mass -= reaction.consume_b;

                // Clean up depleted cells
                if fluids[a_idx].mass < 0.001 { fluids[a_idx] = FluidCell::EMPTY; }
                if fluids[b_idx].mass < 0.001 { fluids[b_idx] = FluidCell::EMPTY; }

                // Place result tile
                if let Some(tile_id) = reaction.result_tile {
                    tiles[idx] = tile_id;
                    fluids[idx] = FluidCell::EMPTY; // tile replaces fluid
                }

                // Place result fluid
                if let Some(fluid_id) = reaction.result_fluid {
                    fluids[idx] = FluidCell::new(fluid_id, reaction.byproduct_mass.max(0.1));
                }

                // Emit event for VFX
                let world_x = (chunk_x * width as i32 + x as i32) as f32 * tile_size + tile_size * 0.5;
                let world_y = (chunk_y * height as i32 + y as i32) as f32 * tile_size + tile_size * 0.5;
                events.push(FluidReactionEvent {
                    position: Vec2::new(world_x, world_y),
                    fluid_a: cell.fluid_id,
                    fluid_b: neighbor.fluid_id,
                    result_tile: reaction.result_tile,
                    result_fluid: reaction.result_fluid,
                });

                reaction_count += 1;
                break; // one reaction per cell per tick
            }
        }
    }
    events
}
```

**Step 3: Call from `fluid_simulation` system**

In `systems.rs`, after `resolve_density_displacement` (line 176), call `execute_fluid_reactions`. This requires adding `FluidReactionRegistry` to the system params. Send events via `EventWriter<FluidReactionEvent>`.

**Step 4: Register event and system**

In `mod.rs`, add `app.add_event::<events::FluidReactionEvent>()`.

**Step 5: Write tests**

```rust
#[test]
fn water_lava_produces_stone_and_steam() {
    // Setup: water cell adjacent to lava cell
    // Execute reactions
    // Assert: result tile = stone, byproduct fluid = steam
}

#[test]
fn reaction_rate_limited() {
    // Setup: many adjacent water-lava pairs
    // Assert: max MAX_REACTIONS_PER_CHUNK reactions per call
}

#[test]
fn reaction_respects_min_mass() {
    // Setup: water mass < min_mass_a
    // Assert: no reaction
}
```

**Step 6: Run tests**

Run: `cargo test --lib fluid::reactions`

**Step 7: Commit**

```
git commit -m "feat(fluid): runtime fluid reaction execution — lava+water=stone+steam"
```

---

## Phase 4: Particles and Trails

### Task 9: Add `gravity_scale` and `fade_out` to Particle

**Files:**
- Modify: `src/particles/particle.rs:6-22` (Particle struct)
- Modify: `src/particles/pool.rs:46-56,113-156` (spawn/init_particle/make_particle)
- Modify: `src/particles/physics.rs:7-34` (gravity + alpha fade)
- Modify: `src/particles/render.rs` (alpha from fade_out)
- Modify: `src/fluid/splash.rs:145-153` (pass new params in spawn calls)
- Test: `src/particles/physics.rs`

**Step 1: Add fields to Particle**

```rust
pub struct Particle {
    // ... existing fields ...
    /// Gravity multiplier: 1.0 = normal, -0.3 = bubbles float up slowly.
    pub gravity_scale: f32,
    /// Whether alpha fades to 0 as particle ages.
    pub fade_out: bool,
}
```

**Step 2: Update ParticlePool::spawn and helpers**

Add `gravity_scale: f32` and `fade_out: bool` parameters.

**Step 3: Update physics.rs**

```rust
p.velocity.y -= config.gravity * p.gravity_scale * dt;
```

**Step 4: Update render.rs for fade_out**

When building the particle mesh, if `p.fade_out`, multiply alpha by `1.0 - p.age_ratio()`.

**Step 5: Update all spawn call sites in splash.rs**

Pass `gravity_scale: 1.0, fade_out: false` for existing splash particles (preserving behavior).

**Step 6: Run tests**

Run: `cargo test --lib particles`

**Step 7: Commit**

```
git commit -m "feat(particles): add gravity_scale and fade_out to Particle"
```

---

### Task 10: Particle-tile collision

**Files:**
- Modify: `src/particles/physics.rs` (add collision check)
- Needs access to: `WorldMap`, `TileRegistry`, `ActiveWorld`

**Step 1: Add WorldMap/TileRegistry/ActiveWorld to particle_physics params**

**Step 2: After position integration, check if new position is inside solid tile**

```rust
let tile_x = (p.position.x / tile_size).floor() as i32;
let tile_y = (p.position.y / tile_size).floor() as i32;
if world_map.is_solid(tile_x, tile_y, &active_world, &tile_registry) {
    // Kill the particle (water splashes stick to walls and disappear)
    p.alive = false;
}
```

**Step 3: Run tests + visual check**

**Step 4: Commit**

```
git commit -m "feat(particles): kill particles on solid tile collision"
```

---

### Task 11: Bubble trail detector for projectiles

**Files:**
- Modify: `src/fluid/detectors.rs` (add `detect_projectile_in_fluid`)
- Modify: `src/fluid/mod.rs` (register in system chain)

**Step 1: Define Projectile marker component** (or use existing one — check codebase)

**Step 2: Write detector**

```rust
pub fn detect_projectile_in_fluid(
    query: Query<(&Transform, &Velocity), With<Projectile>>,
    world_map: Res<WorldMap>,
    active_world: Res<ActiveWorld>,
    fluid_registry: Res<FluidRegistry>,
    mut pool: ResMut<ParticlePool>,
    time: Res<Time>,
    mut throttle: Local<f32>,
) {
    *throttle += time.delta_secs();
    if *throttle < 0.05 { return; }
    *throttle = 0.0;

    for (transform, velocity) in &query {
        let pos = transform.translation.truncate();
        // Check if in fluid
        let tile_x = (pos.x / active_world.tile_size).floor() as i32;
        let tile_y = (pos.y / active_world.tile_size).floor() as i32;
        // ... lookup fluid cell ...
        // If in fluid: spawn 1-2 bubble particles
        pool.spawn(
            pos,
            Vec2::new(0.0, 30.0), // float upward
            0.0,                    // no CA mass
            FluidId::NONE,
            0.6,                    // short lifetime
            2.5,                    // small
            [0.8, 0.9, 1.0, 0.6],  // whitish translucent
            -0.3,                   // negative gravity = float up
            true,                   // fade out
        );
    }
}
```

**Step 3: Register in system chain**

Add before `fluid_simulation` in the Update chain.

**Step 4: Visual test**

**Step 5: Commit**

```
git commit -m "feat(fluid): bubble trail detector for projectiles in fluid"
```

---

## Phase 5: Optimization (if needed)

### Task 12: Double-buffering in CA simulation

**Files:**
- Modify: `src/fluid/systems.rs:152-182` (eliminate clone per iteration)

Replace per-iteration `clone()` with pre-allocated double buffers that swap after each iteration. This removes O(chunk_size^2) allocation per iteration per chunk.

**Step 1: Pre-allocate two buffers per active chunk**

```rust
let mut buf_a = chunk.fluids.clone(); // once
let mut buf_b = vec![FluidCell::EMPTY; len];
for iter in 0..config.iterations_per_tick {
    simulate_grid(&tiles, &buf_a, &mut buf_b, ...);
    resolve_density_displacement(&mut buf_b, ...);
    std::mem::swap(&mut buf_a, &mut buf_b);
    buf_b.fill(FluidCell::EMPTY);
}
// Write final state back
chunk.fluids = buf_a;
```

**Step 2: Run tests**

Run: `cargo test --lib fluid::simulation`
Expected: All 14 simulation tests pass.

**Step 3: Commit**

```
git commit -m "perf(fluid): double-buffering eliminates per-iteration array cloning"
```

---

### Task 13: Sleep/wake for stable chunks

**Files:**
- Modify: `src/fluid/systems.rs` (ActiveFluidChunks, fluid_simulation)

Track consecutive "calm" ticks per chunk. If a chunk has had zero flow for N ticks, mark as sleeping. Wake on: new fluid added, neighbor flow, block change.

This is a larger optimization task — implement only if performance is an issue.

**Step 1: Add sleep tracking to ActiveFluidChunks**

```rust
pub struct ActiveFluidChunks {
    pub chunks: HashSet<(i32, i32)>,
    pub calm_ticks: HashMap<(i32, i32), u32>,
}
```

**Step 2: In fluid_simulation, skip chunks with calm_ticks > SLEEP_THRESHOLD**

**Step 3: Reset calm_ticks when chunk receives fluid or block changes**

**Step 4: Run tests**

**Step 5: Commit**

```
git commit -m "perf(fluid): sleep/wake optimization for stable chunks"
```
