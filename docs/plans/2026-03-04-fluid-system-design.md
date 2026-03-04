# Fluid System Design

**Date:** 2026-03-04
**Status:** Draft
**Related:** Minimum Planet Prototype, Tile Registry, Chunk System

## Overview

Cellular-automaton fluid simulation for Starbeam. Each tile stores a fluid type and level (0–255). Fluids flow under gravity, equalize pressure sideways, and rise through U-shaped channels via pressure propagation. The system uses a sleep/wake optimization to avoid processing settled fluid.

## Fluid Model

### Per-Tile Fluid State

```rust
/// Fluid data for a single tile.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct FluidCell {
    /// Which fluid occupies this tile (0 = none).
    pub fluid_id: FluidId,
    /// Fluid amount: 0 = empty, 255 = full tile.
    pub level: u8,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FluidId(pub u8);

impl FluidId {
    pub const NONE: FluidId = FluidId(0);
    pub const WATER: FluidId = FluidId(1);
    pub const LAVA: FluidId = FluidId(2);
}
```

A solid tile (foreground) cannot contain fluid. Only `AIR` tiles in the FG layer hold fluid data.

### Fluid Definition (RON)

```ron
// assets/world/fluids.registry.ron
[
    (
        id: "water",
        viscosity: 1.0,         // 1.0 = normal flow speed, higher = slower
        density: 1.0,           // heavier fluids sink below lighter
        color: [64, 128, 255],
        light_opacity: 2,       // how much light it blocks per tile
        damage_on_contact: 0.0,
        flow_rate: 6,           // max level transferred per tick
    ),
    (
        id: "lava",
        viscosity: 4.0,
        density: 3.0,
        color: [255, 100, 20],
        light_opacity: 0,
        damage_on_contact: 40.0,
        flow_rate: 2,
    ),
]
```

### Full Tile Level

A tile is "full" when `level == 255`. A full column of fluid produces pressure at the bottom. Pressure = sum of levels above / 255 (depth in full tiles). Pressure allows fluid to rise through U-bends.

---

## Storage: Per-Chunk Fluid Layer

### ChunkData Extension

```rust
pub struct ChunkData {
    pub fg: TileLayer,
    pub bg: TileLayer,
    pub damage: Vec<u8>,
    pub fluid: Vec<FluidCell>,  // NEW: chunk_size * chunk_size entries
}
```

`fluid[idx]` corresponds to the same tile position as `fg.tiles[idx]`. If `fg.tiles[idx]` is solid, `fluid[idx]` must be `FluidCell::default()` (no fluid in solid tiles).

### WorldMap Fluid Access

```rust
impl WorldMap {
    pub fn get_fluid(&self, tile_x: i32, tile_y: i32, ctx: &WorldCtxRef) -> Option<FluidCell> {
        // Same wrapping & chunk resolution as get_tile()
    }

    pub fn set_fluid(&mut self, tile_x: i32, tile_y: i32, cell: FluidCell, ctx: &WorldCtxRef) {
        // Same wrapping & chunk resolution as set_tile()
    }
}
```

These methods reuse the existing coordinate wrapping logic (`wrap_tile_x`, `tile_to_chunk`, `tile_to_local`), so **cross-chunk fluid access works identically to cross-chunk tile access** — no special boundary handling needed.

---

## Cross-Chunk Fluid Flow

### Why It Works Transparently

The existing `WorldMap::get_tile()` / `set_tile()` already resolve world coordinates to the correct chunk via `div_euclid`/`rem_euclid`. The fluid simulation operates in **world coordinates**, not chunk-local coordinates. When fluid at tile `(31, y)` in chunk `(0, 0)` checks its right neighbor `(32, y)`, `get_fluid(32, y)` resolves to chunk `(1, 0)`, local `(0, y)` automatically.

### Simulation Scope

Fluid is simulated **only in loaded chunks** (those present in `WorldMap::chunks`). If fluid would flow into an unloaded chunk area, `get_fluid()` returns `None` → flow is blocked until the chunk loads.

When a chunk loads and its neighbors have active fluid at the boundary, the wake system (below) activates those boundary tiles.

### Chunk Dirty Propagation for Fluid

When fluid changes at a tile position, the containing chunk is marked dirty for mesh rebuild. If the changed tile is at a chunk boundary (local_x == 0, 31, local_y == 0, 31), the adjacent chunk is also marked dirty — same pattern as `update_bitmasks_around()`.

```rust
pub fn mark_fluid_dirty(
    tile_x: i32,
    tile_y: i32,
    ctx: &WorldCtxRef,
) -> HashSet<(i32, i32)> {
    let mut dirty = HashSet::new();
    let wrapped_x = ctx.config.wrap_tile_x(tile_x);
    let (cx, cy) = tile_to_chunk(wrapped_x, tile_y, ctx.config.chunk_size);
    dirty.insert((cx, cy));

    // If at chunk boundary, also dirty the neighbor chunk
    let (lx, ly) = tile_to_local(wrapped_x, tile_y, ctx.config.chunk_size);
    let cs = ctx.config.chunk_size;
    if lx == 0 {
        let nx = ctx.config.wrap_tile_x(tile_x - 1);
        dirty.insert(tile_to_chunk(nx, tile_y, cs));
    }
    if lx == cs - 1 {
        let nx = ctx.config.wrap_tile_x(tile_x + 1);
        dirty.insert(tile_to_chunk(nx, tile_y, cs));
    }
    if ly == 0 && tile_y > 0 {
        dirty.insert(tile_to_chunk(wrapped_x, tile_y - 1, cs));
    }
    if ly == cs - 1 && tile_y < ctx.config.height_tiles - 1 {
        dirty.insert(tile_to_chunk(wrapped_x, tile_y + 1, cs));
    }

    dirty
}
```

---

## Sleep/Wake System

### Motivation

A world of 2048×1024 tiles may have thousands of fluid tiles (oceans, lakes, underground pools). Simulating all of them every tick is wasteful — most are settled and don't change. Only fluid that is actively flowing needs to be processed.

### Active Set Resource

```rust
/// Tracks which fluid tiles need simulation this tick.
#[derive(Resource, Default)]
pub struct ActiveFluids {
    /// Tiles to process this tick (world coordinates).
    pub current: HashSet<(i32, i32)>,
    /// Tiles to add to current set next tick (buffered to avoid mutation during iteration).
    pub pending_wake: HashSet<(i32, i32)>,
    /// Settling counter: how many ticks a tile has been unchanged.
    /// Removed when tile goes to sleep.
    pub settle_ticks: HashMap<(i32, i32), u8>,
}
```

### Wake Triggers

A fluid tile is **woken** (added to `pending_wake`) when:

| Trigger | Source | Notes |
|---------|--------|-------|
| Fluid placed | Player places water bucket / world gen | New fluid always active |
| Block broken next to fluid | `block_interaction_system` | Fluid may flow into opened space |
| Block placed on fluid tile | `block_interaction_system` | Displacement (see below) |
| Neighbor fluid changed level | Fluid simulation step | Chain reaction: flowing fluid wakes neighbors |
| Chunk loaded with fluid | `chunk_loading_system` | Wake all fluid tiles on chunk boundary that have fluid neighbors |

### Sleep Conditions

After each simulation step, check if a tile should go to sleep:

```rust
fn should_sleep(pos: (i32, i32), settle_ticks: &HashMap<(i32, i32), u8>) -> bool {
    // Sleep after 3 consecutive ticks with no change
    settle_ticks.get(&pos).copied().unwrap_or(0) >= 3
}
```

A tile's settle counter increments each tick where its `FluidCell` didn't change. If it changes, the counter resets to 0.

### Wake on Block Break

```rust
// In block_interaction_system, after breaking a tile:
fn wake_adjacent_fluids(
    tile_x: i32,
    tile_y: i32,
    active_fluids: &mut ActiveFluids,
    world_map: &WorldMap,
    ctx: &WorldCtxRef,
) {
    // Check all 4 neighbors + the tile itself
    for (dx, dy) in [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)] {
        let nx = tile_x + dx;
        let ny = tile_y + dy;
        if let Some(cell) = world_map.get_fluid(nx, ny, ctx) {
            if cell.fluid_id != FluidId::NONE {
                active_fluids.pending_wake.insert((nx, ny));
            }
        }
    }
}
```

### Wake on Chunk Load

When `spawn_chunk()` loads a new chunk, scan its boundary tiles (edges of the 32×32 grid). For each boundary tile with fluid, check if the neighbor tile (in the adjacent, already-loaded chunk) also has fluid or is air. If so, wake both tiles — fluid may need to equalize across the boundary.

```rust
fn wake_chunk_boundary_fluids(
    chunk_x: i32,
    chunk_y: i32,
    world_map: &WorldMap,
    active_fluids: &mut ActiveFluids,
    ctx: &WorldCtxRef,
) {
    let cs = ctx.config.chunk_size as i32;
    let base_x = chunk_x * cs;
    let base_y = chunk_y * cs;

    for i in 0..cs {
        // Left edge (lx=0): check tile and its left neighbor
        wake_if_fluid(base_x, base_y + i, world_map, active_fluids, ctx);
        // Right edge (lx=31): check tile and its right neighbor
        wake_if_fluid(base_x + cs - 1, base_y + i, world_map, active_fluids, ctx);
        // Bottom edge (ly=0): check tile and its bottom neighbor
        wake_if_fluid(base_x + i, base_y, world_map, active_fluids, ctx);
        // Top edge (ly=31): check tile and its top neighbor
        wake_if_fluid(base_x + i, base_y + cs - 1, world_map, active_fluids, ctx);
    }
}

fn wake_if_fluid(
    x: i32, y: i32,
    world_map: &WorldMap,
    active_fluids: &mut ActiveFluids,
    ctx: &WorldCtxRef,
) {
    if let Some(cell) = world_map.get_fluid(x, y, ctx) {
        if cell.fluid_id != FluidId::NONE {
            active_fluids.pending_wake.insert((x, y));
            // Also wake neighbors that might need to equalize
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                active_fluids.pending_wake.insert((x + dx, y + dy));
            }
        }
    }
}
```

### Performance Budget

- **Target**: ≤1ms per frame for fluid simulation
- **Batch size**: If `active_fluids.current.len()` exceeds a budget (e.g., 4096 tiles), process in priority order: tiles closer to the camera first, others deferred to next tick
- **Tick rate**: Fluid simulation does NOT need to run every frame. Use a fixed timestep (e.g., every 3 frames at 60fps → 20 fluid ticks/second). This also makes viscosity easier: high-viscosity fluids skip ticks.

---

## Water Displacement by Block Placement

### Problem

Player places a solid block at a tile that contains fluid. The fluid needs to go somewhere.

### Solution

1. **Save** the fluid cell at the target position before overwriting.
2. **Place** the block (set tile to placed block, clear fluid at that position).
3. **Redistribute** the displaced fluid to adjacent tiles.
4. **Wake** all affected tiles.

### Redistribution Algorithm

```rust
fn displace_fluid(
    tile_x: i32,
    tile_y: i32,
    displaced: FluidCell,
    world_map: &mut WorldMap,
    active_fluids: &mut ActiveFluids,
    ctx: &WorldCtxRef,
) {
    let mut remaining = displaced.level as i32;

    // Priority order: up first (fluid pushed up by block), then sides
    // Down is last because the block is being placed, fluid shouldn't go through it
    let neighbors = [(0, 1), (-1, 0), (1, 0), (0, -1)]; // up, left, right, down

    for (dx, dy) in neighbors {
        if remaining <= 0 { break; }

        let nx = tile_x + dx;
        let ny = tile_y + dy;

        // Can't push into solid tiles
        if world_map.is_solid(nx, ny, ctx) { continue; }

        let neighbor_fluid = world_map.get_fluid(nx, ny, ctx)
            .unwrap_or_default();

        // Can't mix different fluid types
        if neighbor_fluid.fluid_id != FluidId::NONE
            && neighbor_fluid.fluid_id != displaced.fluid_id {
            continue;
        }

        let space = 255 - neighbor_fluid.level as i32;
        if space <= 0 { continue; }

        let transfer = remaining.min(space);
        let new_cell = FluidCell {
            fluid_id: displaced.fluid_id,
            level: (neighbor_fluid.level as i32 + transfer) as u8,
        };
        world_map.set_fluid(nx, ny, new_cell, ctx);
        active_fluids.pending_wake.insert((nx, ny));
        remaining -= transfer;
    }

    // If remaining > 0, fluid is destroyed (fully enclosed, nowhere to go).
    // This is rare and acceptable — player intentionally sealed the space.
}
```

### Integration with block_interaction_system

```rust
// In block_interaction_system, when placing a block:
// 1. Check and save existing fluid
let existing_fluid = world_map.get_fluid(tile_x, tile_y, &ctx_ref)
    .unwrap_or_default();

// 2. Place the block
world_map.set_tile(tile_x, tile_y, Layer::Fg, place_id, &ctx_ref);

// 3. Clear fluid at placed position
world_map.set_fluid(tile_x, tile_y, FluidCell::default(), &ctx_ref);

// 4. Displace fluid to neighbors
if existing_fluid.fluid_id != FluidId::NONE {
    displace_fluid(tile_x, tile_y, existing_fluid, &mut world_map, &mut active_fluids, &ctx_ref);
}

// 5. Wake neighbors (fluid might flow into newly available spaces or away from placed block)
wake_adjacent_fluids(tile_x, tile_y, &mut active_fluids, &world_map, &ctx_ref);
```

---

## Fluid Simulation Step

### Overview

Each fluid tick, process all tiles in `active_fluids.current`. For each tile:
1. Try to flow **down** (gravity).
2. If blocked below (solid or full fluid), **equalize sideways**.
3. If pressure from above, allow **upward flow** through U-bends.

### Flow Down (Gravity)

```
if tile below is air or same fluid type with room:
    transfer = min(self.level, 255 - below.level, flow_rate)
    self.level -= transfer
    below.level += transfer
    wake below
```

### Equalize Sideways

When blocked below, fluid spreads to left and right to equalize levels:

```
for each side neighbor (left, right):
    if neighbor is air or same fluid type:
        diff = self.level - neighbor.level
        if diff > 1:  // threshold to prevent oscillation
            transfer = min(diff / 3, flow_rate)  // partial equalization
            self.level -= transfer
            neighbor.level += transfer
            wake neighbor
```

The `diff / 3` ensures gradual equalization (not instant snapping) and prevents oscillation where fluid bounces back and forth.

### Pressure-Based Upward Flow

For U-shaped channels, fluid needs to rise on the other side. Conceptually:

```
  Water level
  ▓▓▓▓      ▓▓▓▓
  ▓▓▓▓██████▓▓▓▓   ← fluid should rise on right to match left
  ▓▓▓▓██  ██▓▓▓▓
  ████████████████  ← solid
```

**Approach**: After down+sideways passes, compute local pressure. For a column of fluid, pressure at tile `y` = number of connected full fluid tiles above it. If a tile has high pressure and the tile above it is air/partial fluid, push fluid upward.

```
pressure = count full fluid tiles directly above this tile
if tile above is air or partial fluid:
    if pressure > 0:
        transfer = min(self.level, 255 - above.level, flow_rate, pressure_based_limit)
        // Only push up if this doesn't drain below equilibrium
```

### Double-Buffering to Avoid Order Dependence

Fluid simulation is sensitive to processing order. To avoid artifacts:

1. **Read** from current fluid state.
2. **Write** to a temporary buffer (deltas).
3. **Apply** all deltas after processing all active tiles.

```rust
struct FluidDelta {
    pos: (i32, i32),
    new_cell: FluidCell,
}

// Process all active tiles → collect deltas
let deltas: Vec<FluidDelta> = process_active_fluids(&active_fluids, &world_map, ctx);

// Apply all deltas at once
for delta in &deltas {
    world_map.set_fluid(delta.pos.0, delta.pos.1, delta.new_cell, ctx);
}
```

### Anti-Oscillation

To prevent fluid bouncing back and forth between two tiles:
- Only transfer if `diff > 1` (not just diff > 0)
- Transfer `diff / 3` (not `diff / 2`) — under-equalize to converge
- Track flow direction from previous tick: if a tile received from left last tick, don't send right this tick (simple flag per active tile)

---

## Rendering

### Fluid Mesh Layer

Fluid tiles are rendered as a **separate mesh layer** between BG and FG, at z = -0.5:

```
z = -1.0  Background tiles
z = -0.5  Fluid layer       ← NEW
z =  0.0  Foreground tiles
z =  1.0  Dropped items
```

### Fluid Visuals

Each fluid tile renders as a quad. The top of the quad is adjusted based on `level`:

```
full tile height = tile_size
rendered height = tile_size * (level / 255.0)
// Quad is anchored at bottom of tile, extends upward by rendered height
```

For tiles with `level == 255` (full), render full tile. For partial tiles, only the top tile in a column should show a partial surface — tiles below a full tile render as full even if they're partial (they're under pressure).

### Fluid Autotiling

Fluids don't use the Blob47 bitmask system. Instead:
- Full tiles with full neighbors: solid fill (no surface visible)
- Top surface tiles (air above or partial fill): animated wave texture
- Side-exposed tiles (air left/right): edge texture

### Chunk Mesh Integration

Option A: Separate fluid mesh per chunk entity (simple, recommended for MVP)
Option B: Bake fluid quads into existing chunk mesh (better batching, harder to update)

Recommend **Option A**: Each chunk gets a third entity for fluid rendering. This allows fluid mesh to rebuild independently from tile mesh (fluid changes more frequently than tiles).

```rust
pub struct ChunkEntities {
    pub fg: Entity,
    pub bg: Entity,
    pub fluid: Entity,  // NEW
}
```

### Lighting Integration

Fluid tiles contribute to the Radiance Cascades lighting pipeline:
- `light_opacity` from FluidDef → adds to density texture (same as solid tiles but lower opacity)
- Lava: emissive source (add to emissive texture)
- Water: slightly reduces light but doesn't block it completely

---

## Fluid ↔ Tile Interactions

### Fluid Meets Fluid

If two different fluid types meet (e.g., water + lava):
- Reaction table in RON config:
```ron
reactions: [
    (fluid_a: "water", fluid_b: "lava", result_tile: "obsidian", result_fluid: None),
]
```
- When water flows into a lava tile or vice versa, consume both and place `result_tile`.

### Fluid Source Blocks

Some tiles can be infinite fluid sources (like Minecraft's water source mechanic):
- A "source" tile produces fluid at a fixed rate when a neighbor has room.
- Defined in tile registry: `fluid_source: Some("water")`.
- Source blocks are always active (never sleep).

---

## Unloaded Chunk Behavior

### Rule: No Simulation in Unloaded Chunks

Fluid is **only simulated in loaded chunks**. Rationale:
- Player can't observe unloaded chunks
- Simulating off-screen is expensive and can cascade infinitely
- Starbound and Terraria both use this approach

### Edge Cases

1. **Fluid at load boundary**: `get_fluid()` returns `None` for unloaded chunks → treated as solid wall (fluid can't flow into the unknown)

2. **Chunk unload with active fluid**: When a chunk is unloaded, its active fluid positions are removed from `ActiveFluids`. Fluid state is preserved in `WorldMap::chunks` (chunks are not removed from `WorldMap` on unload — they stay in memory).

3. **Chunk reload**: When a chunk is loaded again, `wake_chunk_boundary_fluids()` re-activates boundary fluid. Interior fluid stays asleep unless disturbed.

4. **Long-distance flow**: If a player breaks a dam and walks away, fluid will flow as far as loaded chunks allow, then freeze at the boundary. When the player returns, flow resumes. This is acceptable — the player can't see the intermediate area anyway.

---

## Execution Order

```
GameSet::Input
    └── block_interaction_system (may wake fluids, displace water)

GameSet::Physics
    └── fluid_simulation_system (process active fluids, fixed timestep)
        ├── Swap pending_wake → current
        ├── Process all current tiles
        ├── Collect deltas (double-buffered)
        ├── Apply deltas
        ├── Wake neighbors of changed tiles
        ├── Sleep settled tiles
        └── Mark dirty chunks for fluid mesh rebuild

GameSet::WorldUpdate
    ├── rebuild_dirty_chunks (existing tile meshes)
    └── rebuild_dirty_fluid_meshes (NEW: fluid meshes)
```

---

## New Module Structure

```
src/
├── fluid/
│   ├── mod.rs              # FluidPlugin, system registration
│   ├── definition.rs       # FluidId, FluidDef, FluidRegistry
│   ├── cell.rs             # FluidCell, per-tile state
│   ├── simulation.rs       # fluid_simulation_system, flow logic
│   ├── active.rs           # ActiveFluids, sleep/wake logic
│   ├── displacement.rs     # Block placement displacement
│   └── renderer.rs         # Fluid mesh building, chunk fluid entities
```

---

## Implementation Phases

### Phase 1: Data Layer (MVP)
- `FluidCell` in `ChunkData`
- `FluidRegistry` with RON definitions
- `WorldMap::get_fluid()` / `set_fluid()`
- Water tile in registry

### Phase 2: Basic Simulation
- `ActiveFluids` resource
- Gravity flow (down only)
- Sideways equalization
- Integration with `block_interaction_system` (wake on break, displacement on place)

### Phase 3: Rendering
- Fluid mesh layer per chunk
- Level-based quad height
- Surface animation
- Lighting integration (density + emissive for lava)

### Phase 4: Polish
- Pressure-based upward flow (U-bends)
- Fluid reactions (water + lava → obsidian)
- Viscosity-based tick skipping
- Anti-oscillation tuning
- Performance profiling and budget enforcement

### Phase 5: Content
- Water source blocks (world gen: oceans, lakes, rivers)
- Lava in deep underground biomes
- Water/lava buckets as items
- Fluid-based puzzle mechanics
