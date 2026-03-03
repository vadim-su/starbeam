# Fluid Simulation v2 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace per-chunk fluid simulation with a unified global simulation on a fixed timestep, eliminating chunk-boundary seams and frame-rate dependence.

**Architecture:** A `FluidWorld` abstraction wraps `WorldMap` with global `(i32, i32)` addressing, handling chunk lookups and horizontal wrapping internally. A single `simulate_tick()` function processes all active cells globally with alternating scan direction. A fixed-timestep accumulator controls tick rate independently of frame rate.

**Tech Stack:** Rust, Bevy ECS (resources, systems), existing FluidCell/FluidRegistry/TileRegistry types.

---

### Task 1: Add Fixed Timestep to FluidSimConfig

**Files:**
- Modify: `src/fluid/simulation.rs:19-37` (FluidSimConfig)
- Modify: `src/fluid/systems.rs:144-155` (fluid_simulation system signature)

**Step 1: Update FluidSimConfig**

Replace `iterations_per_tick` with `tick_rate` and `max_ticks_per_frame`:

```rust
#[derive(Debug, Clone, Resource)]
pub struct FluidSimConfig {
    /// Simulation ticks per second (default 20 = like Minecraft).
    pub tick_rate: f32,
    /// Max ticks per frame to prevent death spiral (default 3).
    pub max_ticks_per_frame: u32,
    pub min_mass: f32,
    pub min_flow: f32,
    pub max_speed: f32,
}

impl Default for FluidSimConfig {
    fn default() -> Self {
        Self {
            tick_rate: 20.0,
            max_ticks_per_frame: 3,
            min_mass: MIN_MASS,
            min_flow: MIN_FLOW,
            max_speed: MAX_SPEED,
        }
    }
}
```

**Step 2: Add FluidTickAccumulator resource**

Add to `src/fluid/systems.rs`:

```rust
/// Tracks time between fixed fluid simulation ticks.
#[derive(Resource, Default)]
pub struct FluidTickAccumulator {
    pub accumulator: f32,
}
```

Register in `src/fluid/mod.rs` with `.init_resource::<systems::FluidTickAccumulator>()`.

**Step 3: Update fluid_simulation to use accumulator**

At the top of `fluid_simulation`, add accumulator logic:

```rust
pub fn fluid_simulation(
    time: Res<Time>,
    mut accumulator: ResMut<FluidTickAccumulator>,
    // ... rest of params ...
    config: Res<FluidSimConfig>,
) {
    let tick_interval = 1.0 / config.tick_rate;
    accumulator.accumulator += time.delta_secs();
    let mut ticks_this_frame = 0u32;

    while accumulator.accumulator >= tick_interval && ticks_this_frame < config.max_ticks_per_frame {
        accumulator.accumulator -= tick_interval;
        ticks_this_frame += 1;
        // ... run one tick ...
    }
    // Clamp leftover to prevent spiral
    if accumulator.accumulator > tick_interval * 2.0 {
        accumulator.accumulator = tick_interval * 2.0;
    }
}
```

**Step 4: Verify it compiles**

Run: `cargo check 2>&1 | head -30`
Expected: compiles (warnings ok)

**Step 5: Commit**

```
feat(fluid): add fixed timestep to fluid simulation config
```

---

### Task 2: Create FluidWorld Abstraction

**Files:**
- Create: `src/fluid/fluid_world.rs`
- Modify: `src/fluid/mod.rs` (add `pub mod fluid_world;`)

**Step 1: Create FluidWorld struct**

```rust
use crate::fluid::cell::{FluidCell, FluidId};
use crate::fluid::registry::FluidRegistry;
use crate::registry::tile::{TileId, TileRegistry};
use crate::world::chunk::WorldMap;
use std::collections::HashMap;

/// Read-only snapshots of active chunks for consistent simulation reads.
pub struct FluidSnapshots {
    pub data: HashMap<(i32, i32), Vec<FluidCell>>,
}

/// Virtual global grid over chunk-based WorldMap.
/// Provides (global_x, global_y) addressing with horizontal wrapping.
pub struct FluidWorld<'a> {
    pub world_map: &'a mut WorldMap,
    pub snapshots: FluidSnapshots,
    pub chunk_size: u32,
    pub width_chunks: i32,
    pub height_chunks: i32,
    pub tile_registry: &'a TileRegistry,
    pub fluid_registry: &'a FluidRegistry,
}

impl<'a> FluidWorld<'a> {
    /// Create snapshots of all active chunks for consistent reads.
    pub fn new(
        world_map: &'a mut WorldMap,
        active_chunks: &[(i32, i32)],
        chunk_size: u32,
        width_chunks: i32,
        height_chunks: i32,
        tile_registry: &'a TileRegistry,
        fluid_registry: &'a FluidRegistry,
    ) -> Self {
        let mut data = HashMap::new();
        for &(cx, cy) in active_chunks {
            if let Some(chunk) = world_map.chunks.get(&(cx, cy)) {
                data.insert((cx, cy), chunk.fluids.clone());
            }
        }
        Self {
            world_map,
            snapshots: FluidSnapshots { data },
            chunk_size,
            width_chunks,
            height_chunks,
            tile_registry,
            fluid_registry,
        }
    }

    /// Convert global coords to (chunk_x, chunk_y, local_x, local_y).
    /// Handles horizontal wrapping. Returns None if out of vertical bounds.
    fn resolve(&self, gx: i32, gy: i32) -> Option<(i32, i32, u32, u32)> {
        let cs = self.chunk_size as i32;
        let cy = gy.div_euclid(cs);
        if cy < 0 || cy >= self.height_chunks {
            return None;
        }
        let cx = gx.div_euclid(cs).rem_euclid(self.width_chunks);
        let lx = gx.rem_euclid(cs) as u32;
        let ly = gy.rem_euclid(cs) as u32;
        Some((cx, cy, lx, ly))
    }

    fn local_idx(&self, lx: u32, ly: u32) -> usize {
        (ly * self.chunk_size + lx) as usize
    }

    /// Read from snapshot (consistent within tick).
    pub fn read(&self, gx: i32, gy: i32) -> FluidCell {
        let Some((cx, cy, lx, ly)) = self.resolve(gx, gy) else {
            return FluidCell::EMPTY;
        };
        let idx = self.local_idx(lx, ly);
        self.snapshots
            .data
            .get(&(cx, cy))
            .map(|s| s[idx])
            .unwrap_or(FluidCell::EMPTY)
    }

    /// Read current (potentially modified) state from chunk.
    pub fn read_current(&self, gx: i32, gy: i32) -> FluidCell {
        let Some((cx, cy, lx, ly)) = self.resolve(gx, gy) else {
            return FluidCell::EMPTY;
        };
        let idx = self.local_idx(lx, ly);
        self.world_map
            .chunks
            .get(&(cx, cy))
            .map(|c| c.fluids[idx])
            .unwrap_or(FluidCell::EMPTY)
    }

    /// Write directly to chunk data.
    pub fn write(&mut self, gx: i32, gy: i32, cell: FluidCell) {
        let Some((cx, cy, lx, ly)) = self.resolve(gx, gy) else {
            return;
        };
        let idx = self.local_idx(lx, ly);
        if let Some(chunk) = self.world_map.chunks.get_mut(&(cx, cy)) {
            chunk.fluids[idx] = cell;
        }
    }

    /// Add mass to a cell (respecting fluid type).
    pub fn add_mass(&mut self, gx: i32, gy: i32, fluid_id: FluidId, amount: f32) {
        let Some((cx, cy, lx, ly)) = self.resolve(gx, gy) else {
            return;
        };
        let idx = self.local_idx(lx, ly);
        if let Some(chunk) = self.world_map.chunks.get_mut(&(cx, cy)) {
            if chunk.fluids[idx].fluid_id == FluidId::NONE {
                chunk.fluids[idx].fluid_id = fluid_id;
            }
            chunk.fluids[idx].mass += amount;
        }
    }

    /// Subtract mass from a cell.
    pub fn sub_mass(&mut self, gx: i32, gy: i32, amount: f32) {
        let Some((cx, cy, lx, ly)) = self.resolve(gx, gy) else {
            return;
        };
        let idx = self.local_idx(lx, ly);
        if let Some(chunk) = self.world_map.chunks.get_mut(&(cx, cy)) {
            chunk.fluids[idx].mass -= amount;
        }
    }

    /// Check if a tile at global coords is solid.
    pub fn is_solid(&self, gx: i32, gy: i32) -> bool {
        let Some((cx, cy, lx, ly)) = self.resolve(gx, gy) else {
            return true; // treat out-of-bounds as solid
        };
        let idx = self.local_idx(lx, ly);
        self.world_map
            .chunks
            .get(&(cx, cy))
            .map(|c| self.tile_registry.is_solid(c.fg.tiles[idx]))
            .unwrap_or(true)
    }

    /// Read tile ID at global coords.
    pub fn tile_at(&self, gx: i32, gy: i32) -> TileId {
        let Some((cx, cy, lx, ly)) = self.resolve(gx, gy) else {
            return TileId::AIR;
        };
        let idx = self.local_idx(lx, ly);
        self.world_map
            .chunks
            .get(&(cx, cy))
            .map(|c| c.fg.tiles[idx])
            .unwrap_or(TileId::AIR)
    }

    /// Set tile ID at global coords (for reactions that produce tiles).
    pub fn set_tile(&mut self, gx: i32, gy: i32, tile: TileId) {
        let Some((cx, cy, lx, ly)) = self.resolve(gx, gy) else {
            return;
        };
        let idx = self.local_idx(lx, ly);
        if let Some(chunk) = self.world_map.chunks.get_mut(&(cx, cy)) {
            chunk.fg.tiles[idx] = tile;
        }
    }

    /// Swap fluid cells at two global positions (for density displacement).
    pub fn swap_fluids(&mut self, a: (i32, i32), b: (i32, i32)) {
        let cell_a = self.read_current(a.0, a.1);
        let cell_b = self.read_current(b.0, b.1);
        self.write(a.0, a.1, cell_b);
        self.write(b.0, b.1, cell_a);
    }

    /// Check if a chunk exists in the world.
    pub fn has_chunk(&self, cx: i32, cy: i32) -> bool {
        self.world_map.chunks.contains_key(&(cx, cy))
    }
}
```

**Step 2: Add module to mod.rs**

Add `pub mod fluid_world;` to `src/fluid/mod.rs`.

**Step 3: Write unit tests for FluidWorld addressing**

In `fluid_world.rs`, add tests for resolve(), wrapping, read/write.

**Step 4: Verify it compiles**

Run: `cargo check 2>&1 | head -30`

**Step 5: Run tests**

Run: `cargo test -p starbeam --lib fluid::fluid_world`

**Step 6: Commit**

```
feat(fluid): add FluidWorld global addressing abstraction
```

---

### Task 3: Rewrite simulate_tick as Global Pass

**Files:**
- Modify: `src/fluid/simulation.rs` — rewrite `simulate_grid` → `simulate_tick`, remove `try_flow_horizontal`, simplify flow functions to use FluidWorld.

**Step 1: Write simulate_tick**

Replace `simulate_grid` with a new function that operates on `FluidWorld`:

```rust
/// Run one tick of the fluid simulation on all active cells globally.
/// `tick_parity` alternates scan direction: even=L→R, odd=R→L.
pub fn simulate_tick(
    world: &mut FluidWorld,
    active_chunks: &[(i32, i32)],
    config: &FluidSimConfig,
    tick_parity: u32,
) {
    let cs = world.chunk_size as i32;

    // Process each active chunk's cells
    for &(cx, cy) in active_chunks {
        let base_gx = cx * cs;
        let base_gy = cy * cs;

        for ly in 0..cs {
            let gy = base_gy + ly;

            // Alternate horizontal scan direction for symmetry
            let x_range: Vec<i32> = if tick_parity % 2 == 0 {
                (0..cs).collect()
            } else {
                (0..cs).rev().collect()
            };

            for lx in x_range {
                let gx = base_gx + lx;
                let cell = world.read(gx, gy);

                if cell.is_empty() {
                    continue;
                }

                let def = world.fluid_registry.get(cell.fluid_id);
                let max_speed = config.max_speed * (1.0 - def.viscosity).max(0.1);
                let mut remaining = world.read_current(gx, gy).mass;

                if def.is_gas {
                    // Gas: UP first, horizontal, DOWN decompression
                    remaining = flow_vertical(world, gx, gy, 1, true, remaining,
                        cell.fluid_id, def.max_compress, max_speed, config.min_flow);
                    remaining = flow_horizontal(world, gx, gy, remaining,
                        cell.fluid_id, cell.mass, max_speed, config.min_flow);
                    flow_vertical(world, gx, gy, -1, false, remaining,
                        cell.fluid_id, def.max_compress, max_speed, config.min_flow);
                } else {
                    // Liquid: DOWN first, horizontal, UP decompression
                    remaining = flow_vertical(world, gx, gy, -1, true, remaining,
                        cell.fluid_id, def.max_compress, max_speed, config.min_flow);
                    remaining = flow_horizontal(world, gx, gy, remaining,
                        cell.fluid_id, cell.mass, max_speed, config.min_flow);
                    flow_vertical(world, gx, gy, 1, false, remaining,
                        cell.fluid_id, def.max_compress, max_speed, config.min_flow);
                }
            }
        }
    }

    // Cleanup: zero cells with negligible mass
    for &(cx, cy) in active_chunks {
        if let Some(chunk) = world.world_map.chunks.get_mut(&(cx, cy)) {
            for cell in chunk.fluids.iter_mut() {
                if cell.mass > 0.0 && cell.mass < config.min_mass {
                    *cell = FluidCell::EMPTY;
                }
            }
        }
    }
}
```

**Step 2: Write flow_vertical using FluidWorld**

```rust
fn flow_vertical(
    world: &mut FluidWorld,
    gx: i32, gy: i32,
    dy: i32,
    is_primary: bool,
    remaining: f32,
    fluid_id: FluidId,
    max_compress: f32,
    max_speed: f32,
    min_flow: f32,
) -> f32 {
    if remaining <= 0.0 {
        return 0.0;
    }

    let ny = gy + dy;

    if world.is_solid(gx, ny) {
        return remaining;
    }

    let neighbor = world.read(gx, ny);
    if !neighbor.is_empty() && neighbor.fluid_id != fluid_id {
        return remaining;
    }

    // Write-buffer check
    let current_neighbor = world.read_current(gx, ny);
    if current_neighbor.fluid_id != FluidId::NONE && current_neighbor.fluid_id != fluid_id {
        return remaining;
    }

    let neighbor_mass = current_neighbor.mass;
    let total = remaining + neighbor_mass;

    let flow = if is_primary {
        let target = get_stable_state(total, max_compress);
        target - neighbor_mass
    } else {
        if remaining <= MAX_MASS {
            return remaining;
        }
        let target_stay = get_stable_state(total, max_compress);
        remaining - target_stay
    };

    if flow <= 0.0 {
        return remaining;
    }

    let mut flow = flow;
    if flow > min_flow {
        flow *= 0.5;
    }
    flow = flow.min(max_speed).min(remaining).max(0.0);

    if flow <= 0.0 {
        return remaining;
    }

    world.sub_mass(gx, gy, flow);
    world.add_mass(gx, ny, fluid_id, flow);

    remaining - flow
}
```

**Step 3: Write flow_horizontal using FluidWorld**

```rust
fn flow_horizontal(
    world: &mut FluidWorld,
    gx: i32, gy: i32,
    mut remaining: f32,
    fluid_id: FluidId,
    original_mass: f32,
    max_speed: f32,
    min_flow: f32,
) -> f32 {
    // Left
    remaining = flow_side(world, gx, gy, gx - 1, remaining,
        fluid_id, original_mass, max_speed, min_flow);
    // Right
    remaining = flow_side(world, gx, gy, gx + 1, remaining,
        fluid_id, original_mass, max_speed, min_flow);
    remaining
}

fn flow_side(
    world: &mut FluidWorld,
    gx: i32, gy: i32,
    ngx: i32,
    remaining: f32,
    fluid_id: FluidId,
    original_mass: f32,
    max_speed: f32,
    min_flow: f32,
) -> f32 {
    if remaining <= 0.0 {
        return 0.0;
    }

    if world.is_solid(ngx, gy) {
        return remaining;
    }

    let neighbor = world.read(ngx, gy);
    if !neighbor.is_empty() && neighbor.fluid_id != fluid_id {
        return remaining;
    }

    let current_neighbor = world.read_current(ngx, gy);
    if current_neighbor.fluid_id != FluidId::NONE && current_neighbor.fluid_id != fluid_id {
        return remaining;
    }

    let mut flow = (original_mass - world.read(ngx, gy).mass) / 4.0;
    if flow <= 0.0 {
        return remaining;
    }
    if flow > min_flow {
        flow *= 0.5;
    }
    flow = flow.min(max_speed).min(remaining).max(0.0);

    if flow <= 0.0 {
        return remaining;
    }

    world.sub_mass(gx, gy, flow);
    world.add_mass(ngx, gy, fluid_id, flow);

    remaining - flow
}
```

**Step 4: Keep get_stable_state unchanged (it's correct)**

Keep the existing `get_stable_state` function and its tests.

**Step 5: Remove old functions**

Delete: `simulate_grid`, `try_flow_vertical`, `try_flow_horizontal`, `try_flow_side`, `reconcile_chunk_boundaries`, `collect_horizontal_transfer`, `collect_vertical_transfer`.

Keep the old cross-chunk boundary tests — rewrite them to use `simulate_tick` instead.

**Step 6: Verify it compiles**

Run: `cargo check 2>&1 | head -30`

**Step 7: Commit**

```
feat(fluid): rewrite simulation as global pass with FluidWorld
```

---

### Task 4: Rewrite Density Displacement for Global Coords

**Files:**
- Modify: `src/fluid/reactions.rs:140-283` (resolve_density_displacement + horizontal_displace_pair)

**Step 1: Rewrite resolve_density_displacement**

Change signature to take `FluidWorld` and list of active chunks:

```rust
pub fn resolve_density_displacement(
    world: &mut FluidWorld,
    active_chunks: &[(i32, i32)],
) {
    let cs = world.chunk_size as i32;

    // Phase 1: Vertical displacement — multi-pass bubble sort
    for _pass in 0..(cs * 2) {  // enough passes for cross-chunk settling
        let mut any_swap = false;
        for &(cx, cy) in active_chunks {
            let base_gx = cx * cs;
            let base_gy = cy * cs;

            for ly in 0..cs {
                let gy = base_gy + ly;
                for lx in 0..cs {
                    let gx = base_gx + lx;

                    let below = world.read_current(gx, gy);
                    let above = world.read_current(gx, gy + 1);

                    if below.is_empty() || above.is_empty() { continue; }
                    if below.fluid_id == above.fluid_id { continue; }

                    let d_below = world.fluid_registry.get(below.fluid_id).density;
                    let d_above = world.fluid_registry.get(above.fluid_id).density;

                    if d_above > d_below {
                        world.swap_fluids((gx, gy), (gx, gy + 1));
                        any_swap = true;
                    }
                }
            }
        }
        if !any_swap { break; }
    }

    // Phase 2: Horizontal spreading (L→R then R→L)
    for &(cx, cy) in active_chunks {
        let base_gx = cx * cs;
        let base_gy = cy * cs;
        // L→R
        for ly in 0..cs {
            let gy = base_gy + ly;
            for lx in 0..(cs - 1) {
                let gx = base_gx + lx;
                horizontal_displace_global(world, gx, gx + 1, gy);
            }
            // Cross-chunk: rightmost column → left column of right neighbor
            // This happens naturally because flow_side uses global coords
        }
        // R→L
        for ly in 0..cs {
            let gy = base_gy + ly;
            for lx in (1..cs).rev() {
                let gx = base_gx + lx;
                horizontal_displace_global(world, gx, gx - 1, gy);
            }
        }
    }
}

fn horizontal_displace_global(
    world: &mut FluidWorld,
    src_gx: i32, dst_gx: i32, gy: i32,
) {
    let src = world.read_current(src_gx, gy);
    let dst = world.read_current(dst_gx, gy);

    if src.is_empty() || dst.is_empty() { return; }
    if src.fluid_id == dst.fluid_id { return; }

    let d_src = world.fluid_registry.get(src.fluid_id).density;
    let d_dst = world.fluid_registry.get(dst.fluid_id).density;
    if d_src <= d_dst { return; }

    // Need cell above dst for displaced light fluid
    if world.is_solid(dst_gx, gy + 1) { return; }

    let above = world.read_current(dst_gx, gy + 1);

    if above.is_empty() {
        world.write(dst_gx, gy + 1, dst);
        world.write(dst_gx, gy, src);
        world.write(src_gx, gy, FluidCell::EMPTY);
    } else if above.fluid_id == dst.fluid_id {
        let mut merged = above;
        merged.mass += dst.mass;
        world.write(dst_gx, gy + 1, merged);
        world.write(dst_gx, gy, src);
        world.write(src_gx, gy, FluidCell::EMPTY);
    }
}
```

**Step 2: Verify it compiles**

Run: `cargo check 2>&1 | head -30`

**Step 3: Commit**

```
feat(fluid): rewrite density displacement with global coordinates
```

---

### Task 5: Adapt Fluid Reactions for Global Coords

**Files:**
- Modify: `src/fluid/reactions.rs:297-420` (execute_fluid_reactions)

**Step 1: Rewrite execute_fluid_reactions**

Change to use FluidWorld:

```rust
pub fn execute_fluid_reactions(
    world: &mut FluidWorld,
    active_chunks: &[(i32, i32)],
    reaction_registry: &FluidReactionRegistry,
    tile_size: f32,
) -> Vec<FluidReactionEvent> {
    let cs = world.chunk_size as i32;
    let mut events = Vec::new();
    let mut reaction_count: u32 = 0;

    for &(cx, cy) in active_chunks {
        let base_gx = cx * cs;
        let base_gy = cy * cs;

        for ly in 0..cs {
            for lx in 0..cs {
                if reaction_count >= MAX_REACTIONS_PER_CHUNK * active_chunks.len() as u32 {
                    return events;
                }

                let gx = base_gx + lx;
                let gy = base_gy + ly;
                let cell = world.read_current(gx, gy);
                if cell.is_empty() { continue; }

                let neighbors: [(i32, i32, Adjacency); 4] = [
                    (0, -1, Adjacency::Below),
                    (0, 1, Adjacency::Above),
                    (-1, 0, Adjacency::Side),
                    (1, 0, Adjacency::Side),
                ];

                for (dx, dy, adj) in &neighbors {
                    let ngx = gx + dx;
                    let ngy = gy + dy;
                    let neighbor = world.read_current(ngx, ngy);
                    if neighbor.is_empty() || neighbor.fluid_id == cell.fluid_id {
                        continue;
                    }

                    let Some(reaction) =
                        reaction_registry.find_reaction(cell.fluid_id, neighbor.fluid_id, adj)
                    else { continue; };

                    // Determine which is a, which is b
                    let (a_pos, b_pos) = if cell.fluid_id == reaction.fluid_a {
                        ((gx, gy), (ngx, ngy))
                    } else {
                        ((ngx, ngy), (gx, gy))
                    };

                    let cell_a = world.read_current(a_pos.0, a_pos.1);
                    let cell_b = world.read_current(b_pos.0, b_pos.1);

                    if cell_a.mass < reaction.min_mass_a || cell_b.mass < reaction.min_mass_b {
                        continue;
                    }

                    // Consume mass
                    let mut new_a = cell_a;
                    let mut new_b = cell_b;
                    new_a.mass -= reaction.consume_a;
                    new_b.mass -= reaction.consume_b;

                    if new_a.mass < 0.001 { new_a = FluidCell::EMPTY; }
                    if new_b.mass < 0.001 { new_b = FluidCell::EMPTY; }

                    // Place result tile
                    if let Some(tile_id) = reaction.result_tile {
                        world.set_tile(a_pos.0, a_pos.1, tile_id);
                        new_a = FluidCell::EMPTY;
                    }

                    // Place result fluid or byproduct
                    if new_a.is_empty() {
                        if let Some(fid) = reaction.result_fluid {
                            new_a = FluidCell::new(fid, reaction.byproduct_mass.max(0.1));
                        } else if let Some(fid) = reaction.byproduct_fluid {
                            new_a = FluidCell::new(fid, reaction.byproduct_mass.max(0.1));
                        }
                    }

                    world.write(a_pos.0, a_pos.1, new_a);
                    world.write(b_pos.0, b_pos.1, new_b);

                    let world_x = gx as f32 * tile_size + tile_size * 0.5;
                    let world_y = gy as f32 * tile_size + tile_size * 0.5;
                    events.push(FluidReactionEvent {
                        position: Vec2::new(world_x, world_y),
                        fluid_a: cell_a.fluid_id,
                        fluid_b: cell_b.fluid_id,
                        result_tile: reaction.result_tile,
                        result_fluid: reaction.result_fluid,
                    });

                    reaction_count += 1;
                    break;
                }
            }
        }
    }
    events
}
```

**Step 2: Verify it compiles**

Run: `cargo check 2>&1 | head -30`

**Step 3: Commit**

```
feat(fluid): adapt fluid reactions to global FluidWorld coordinates
```

---

### Task 6: Rewrite fluid_simulation System

**Files:**
- Modify: `src/fluid/systems.rs:144-348` (fluid_simulation function)

**Step 1: Rewrite the main loop**

Replace the per-chunk loop + reconcile with unified global approach:

```rust
pub fn fluid_simulation(
    time: Res<Time>,
    mut accumulator: ResMut<FluidTickAccumulator>,
    mut world_map: ResMut<WorldMap>,
    fluid_registry: Res<FluidRegistry>,
    tile_registry: Res<TileRegistry>,
    active_world: Res<ActiveWorld>,
    mut active_fluids: ResMut<ActiveFluidChunks>,
    config: Res<FluidSimConfig>,
    reaction_registry: Option<Res<FluidReactionRegistry>>,
    mut reaction_events: MessageWriter<FluidReactionEvent>,
) {
    let tick_interval = 1.0 / config.tick_rate;
    accumulator.accumulator += time.delta_secs();
    let mut ticks_this_frame = 0u32;

    while accumulator.accumulator >= tick_interval && ticks_this_frame < config.max_ticks_per_frame {
        accumulator.accumulator -= tick_interval;
        ticks_this_frame += 1;

        run_one_tick(
            &mut world_map,
            &fluid_registry,
            &tile_registry,
            &active_world,
            &mut active_fluids,
            &config,
            reaction_registry.as_deref(),
            &mut reaction_events,
            ticks_this_frame - 1,  // tick parity for alternating scan
        );
    }

    // Prevent accumulator spiral
    let max_acc = tick_interval * config.max_ticks_per_frame as f32;
    if accumulator.accumulator > max_acc {
        accumulator.accumulator = max_acc;
    }
}

fn run_one_tick(
    world_map: &mut WorldMap,
    fluid_registry: &FluidRegistry,
    tile_registry: &TileRegistry,
    active_world: &ActiveWorld,
    active_fluids: &mut ActiveFluidChunks,
    config: &FluidSimConfig,
    reaction_registry: Option<&FluidReactionRegistry>,
    reaction_events: &mut MessageWriter<FluidReactionEvent>,
    tick_parity: u32,
) {
    let chunk_size = active_world.chunk_size;
    let width_chunks = active_world.width_chunks();
    let height_chunks = active_world.height_chunks();

    // Collect non-sleeping active chunks
    let chunks_to_process: Vec<(i32, i32)> = active_fluids.chunks.iter().copied()
        .filter(|coord| {
            active_fluids.calm_ticks.get(coord).copied().unwrap_or(0) <= SLEEP_THRESHOLD
        })
        .collect();

    if chunks_to_process.is_empty() {
        return;
    }

    // Snapshot for movement detection
    let initial_snapshots: HashMap<(i32, i32), Vec<FluidCell>> = chunks_to_process.iter()
        .filter_map(|&(cx, cy)| {
            world_map.chunks.get(&(cx, cy)).map(|c| ((cx, cy), c.fluids.clone()))
        })
        .collect();

    // Create FluidWorld with snapshots for consistent reads
    let mut fluid_world = FluidWorld::new(
        world_map,
        &chunks_to_process,
        chunk_size,
        width_chunks,
        height_chunks,
        tile_registry,
        fluid_registry,
    );

    // 1. Flow simulation
    simulate_tick(&mut fluid_world, &chunks_to_process, config, tick_parity);

    // 2. Density displacement
    resolve_density_displacement(&mut fluid_world, &chunks_to_process);

    // 3. Fluid reactions
    if let Some(rr) = reaction_registry {
        let events = execute_fluid_reactions(
            &mut fluid_world,
            &chunks_to_process,
            rr,
            active_world.tile_size,
        );
        for evt in events {
            reaction_events.write(evt);
        }
    }

    // 4. Detect movement and update calm_ticks
    for &(cx, cy) in &chunks_to_process {
        let moved = if let (Some(initial), Some(chunk)) = (
            initial_snapshots.get(&(cx, cy)),
            fluid_world.world_map.chunks.get(&(cx, cy)),
        ) {
            initial.iter().zip(chunk.fluids.iter()).any(|(old, new)| {
                old.fluid_id != new.fluid_id || (old.mass - new.mass).abs() >= CALM_MASS_EPSILON
            })
        } else {
            false
        };

        let entry = active_fluids.calm_ticks.entry((cx, cy)).or_insert(0);
        if moved {
            *entry = 0;
        } else {
            *entry = entry.saturating_add(1);
        }
    }

    // 5. Activate neighbor chunks that received fluid
    activate_neighbors(fluid_world.world_map, active_fluids, width_chunks, height_chunks);

    // 6. Prune empty chunks
    prune_empty_chunks(fluid_world.world_map, active_fluids);
}
```

**Step 2: Extract neighbor activation and pruning into helper functions**

Move the existing neighbor-activation logic (lines 270-311) and pruning logic (lines 332-347) into `activate_neighbors()` and `prune_empty_chunks()` helper functions.

**Step 3: Remove the old reconcile_chunk_boundaries call and imports**

Remove `use crate::fluid::simulation::reconcile_chunk_boundaries` from systems.rs.

**Step 4: Verify it compiles**

Run: `cargo check 2>&1 | head -30`

**Step 5: Commit**

```
feat(fluid): rewrite fluid_simulation system with global simulation + fixed timestep
```

---

### Task 7: Clean Up Removed Code and Rendering

**Files:**
- Modify: `src/fluid/simulation.rs` — delete old reconcile functions
- Modify: `src/fluid/render.rs` — remove `emission_cover_seed` parameter, fix 18 test call sites
- Modify: `src/fluid/systems.rs` — remove emission_cover_seed computation (lines 471-508)
- Modify: `src/world/rc_lighting.rs` — simplify emission coverage (remove cross-chunk seed)

**Step 1: Delete reconcile code from simulation.rs**

Remove `reconcile_chunk_boundaries`, `collect_horizontal_transfer`, `collect_vertical_transfer` functions entirely. Keep the `get_stable_state` function and constant definitions.

**Step 2: Remove emission_cover_seed parameter from build_fluid_mesh**

In `src/fluid/render.rs`, remove the 17th parameter `emission_cover_seed: Option<&[bool]>` from `build_fluid_mesh`. The emission coverage can now be computed purely within each chunk since the global simulation ensures correct fluid layering.

**Step 3: Fix all 18 test call sites in render.rs**

Each test call to `build_fluid_mesh` needs to have `None` (the 17th arg) removed since the parameter no longer exists.

**Step 4: Update the call in systems.rs**

Remove the `emission_cover_seed` computation block (lines 471-508) and the `Some(&emission_cover_seed)` argument from the `build_fluid_mesh` call.

**Step 5: Simplify rc_lighting.rs emission coverage**

Remove the cross-chunk seed logic from `rc_lighting.rs`. The per-column coverage can be computed within the chunk since lava and water now properly stratify.

**Step 6: Verify it compiles (including tests)**

Run: `cargo check --tests 2>&1 | head -50`

**Step 7: Commit**

```
refactor(fluid): remove reconcile, emission_cover_seed, simplify rendering
```

---

### Task 8: Update and Write Tests

**Files:**
- Modify: `src/fluid/simulation.rs` (tests module)
- Modify: `src/fluid/reactions.rs` (tests module)
- Modify: `src/fluid/fluid_world.rs` (tests module)

**Step 1: Rewrite simulation tests**

The old `simulate_grid` tests need to be rewritten to test `simulate_tick` via `FluidWorld`. The test pattern becomes:

```rust
#[test]
fn water_falls_down() {
    let tr = test_tile_registry();
    let fr = test_fluid_registry();
    let config = FluidSimConfig::default();
    let water_id = fr.by_name("water");
    let cs: u32 = 4;

    let mut world_map = WorldMap::default();
    let mut chunk = make_chunk(cs);
    chunk.fluids[idx(1, 2, cs)] = FluidCell::new(water_id, 1.0);
    world_map.chunks.insert((0, 0), chunk);

    let active = vec![(0, 0)];
    let mut fw = FluidWorld::new(&mut world_map, &active, cs, 1, 1, &tr, &fr);
    simulate_tick(&mut fw, &active, &config, 0);

    let chunk = world_map.chunks.get(&(0, 0)).unwrap();
    assert!(chunk.fluids[idx(1, 1, cs)].mass > 0.0);
    assert!(chunk.fluids[idx(1, 2, cs)].mass < 1.0);
}
```

**Step 2: Rewrite cross-chunk tests**

Cross-chunk tests now work naturally — just place water at chunk boundary and run `simulate_tick`:

```rust
#[test]
fn water_flows_across_chunk_boundary() {
    // Two horizontal chunks, water at right edge of chunk 0
    // After simulate_tick, water should flow to left edge of chunk 1
    // No reconcile needed — FluidWorld handles it
}
```

**Step 3: Write new test: fixed timestep doesn't run when accumulator is low**

**Step 4: Write new test: density displacement works across chunks**

**Step 5: Run all tests**

Run: `cargo test -p starbeam --lib fluid 2>&1`
Expected: All tests pass.

**Step 6: Commit**

```
test(fluid): rewrite simulation tests for global FluidWorld
```

---

### Task 9: Final Integration Test

**Step 1: Run full build**

Run: `cargo check --tests 2>&1`
Expected: Clean compile.

**Step 2: Run all fluid tests**

Run: `cargo test -p starbeam --lib fluid 2>&1`
Expected: All pass.

**Step 3: Run full test suite**

Run: `cargo test -p starbeam 2>&1 | tail -20`
Expected: No regressions.

**Step 4: Commit any remaining fixes**

```
fix(fluid): address integration test issues
```
