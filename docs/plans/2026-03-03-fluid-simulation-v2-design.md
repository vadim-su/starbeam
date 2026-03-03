# Fluid Simulation v2 — Design

## Problem Statement

The current fluid simulation has three critical issues:

1. **No frame-rate independence** — simulation runs once per frame (60-144 ticks/sec depending on monitor). Water flows way too fast and speed varies across hardware.
2. **Cross-chunk seams** — each chunk is simulated independently, with `reconcile_chunk_boundaries` running once after all intra-chunk iterations. This creates visible flow slowdown at chunk edges.
3. **Asymmetric flow** — row-major scan order (bottom-up, left-to-right) causes directional bias in horizontal spreading.

## Design Decisions

- **Fixed timestep** with configurable tick rate (default 20 ticks/sec)
- **Unified global simulation** — all active cells processed as one field, no per-chunk boundaries
- **Full rewrite** of simulation loop — preserve FluidCell, FluidRegistry, rendering

## Architecture

### 1. Fixed Timestep

Custom accumulator in the fluid system (not Bevy's `FixedUpdate`, as we want an independent rate):

```
accumulator += delta_time;
while accumulator >= TICK_INTERVAL {
    simulate_one_tick();
    accumulator -= TICK_INTERVAL;
}
```

**Parameters:**
- `fluid_tick_rate: f32 = 20.0` — ticks per second
- `max_ticks_per_frame: u32 = 3` — death spiral protection
- `iterations_per_tick` — **removed**, speed controlled via tick_rate

### 2. FluidWorld — Global Virtual Grid

A thin wrapper over `WorldMap` converting `(global_x, global_y)` to `(chunk_cx, chunk_cy, local_x, local_y)`.

**API:**
```rust
impl FluidWorld<'_> {
    fn get(&self, gx: i32, gy: i32) -> &FluidCell;
    fn get_mut(&mut self, gx: i32, gy: i32) -> &mut FluidCell;
    fn is_solid(&self, gx: i32, gy: i32) -> bool;
    fn swap(&mut self, a: (i32, i32), b: (i32, i32));
}
```

Handles horizontal wrapping. No vertical wrapping — bounded by world height.

### 3. simulate_tick — Single Global Pass

Replaces `simulate_grid` + `reconcile_chunk_boundaries`.

**Cell processing order:**
- Alternating scan direction: even ticks = left-to-right, odd ticks = right-to-left
- Bottom-to-top for liquids (gravity first), top-to-bottom for gases

**Per-cell flow order (liquids):**
1. Vertical down (gravity) via `get_stable_state`
2. Horizontal equalization (left + right)
3. Decompression up (only if mass > MAX_MASS)

**Per-cell flow order (gases):**
1. Vertical up (buoyancy) via `get_stable_state`
2. Horizontal equalization
3. Decompression down

### 4. Tick Pipeline

```
1. Collect active cells from active chunks
2. Flow pass (with alternating scan direction)
3. Density displacement — global bubble-sort
   (heavy sinks, light floats, horizontal 3-cell rotation)
4. Fluid reactions (when enabled)
5. Cleanup: zero cells with mass < MIN_MASS
6. Update calm_ticks per chunk for sleep system
```

### 5. Sleep System (preserved, adapted)

- Per-chunk calm_ticks tracking remains
- SLEEP_THRESHOLD = 60 ticks (now 3 seconds at 20 ticks/sec instead of ~1 second)
- Active chunks: cells included in simulation
- Sleeping chunks: cells skipped
- Wake condition: neighbor chunk has flow near boundary

### 6. Density Displacement (unified)

Same algorithm as current (bubble-sort + horizontal 3-cell rotation), but operating on global coordinates. No separate cross-chunk swap pass needed.

### 7. What We Keep

- `FluidCell` (fluid_id + mass) — unchanged
- `FluidRegistry` / `FluidDef` — unchanged
- `get_stable_state` algorithm — correct as-is
- Flow smoothing (0.5 factor for small flows)
- Write-buffer type check (different fluid can't claim same cell)
- Rendering: `build_fluid_mesh`, RC lighting emission — untouched
- `emission_cover_seed` cross-chunk hack — can be removed, coverage computable globally

### 8. What We Remove

| Remove | Replace with |
|--------|-------------|
| `simulate_grid` (per-chunk, double-buffered) | `simulate_tick` (global, in-place or single double-buffer) |
| `reconcile_chunk_boundaries` | Not needed — global addressing |
| `collect_horizontal/vertical_transfer` | Not needed |
| Cross-chunk density displacement | Built into normal displacement pass |
| `iterations_per_tick` config | `fluid_tick_rate` config |
| `emission_cover_seed` parameter | Global coverage computation |

## Files Affected

- `src/fluid/simulation.rs` — full rewrite
- `src/fluid/systems.rs` — rewrite main loop, add fixed timestep, add FluidWorld
- `src/fluid/reactions.rs` — adapt to global coordinates
- `src/fluid/render.rs` — remove `emission_cover_seed` parameter, fix 18 test call sites
- `src/fluid/mod.rs` — minor wiring changes
- `src/world/rc_lighting.rs` — simplify emission coverage (no cross-chunk seed)

## Risks

- Global simulation is O(active_cells) per tick — at 20 ticks/sec with thousands of active cells this should be fast enough, but needs profiling
- In-place mutation (vs double-buffer) may cause different settling behavior — we can keep double-buffer at global level if needed
- Sleep wake/sleep transitions need careful testing at chunk boundaries
