# Fluid CA v2 — Design Document

**Goal:** Implement a stable, visually polished fluid simulation using Cellular Automata with pressure-equalization, visual-only waves, instant reactions, smooth shader rendering, and splash particles.

**Architecture:** CA flow operates on the global active chunk space (not per-chunk with delta buffers). Fluid data texture per chunk with bilinear filtering for smooth blending. Visual-only spring wave model on surface cells. Fixed tick rate simulation decoupled from frame rate.

**Previous attempt problems & solutions:**
- Lava making water jump → eliminated by one-directional density swap (heavy displaces light, never reverse) + settled flag
- Wrong spread direction → eliminated by alternating L↔R iteration per tick + equalization formula
- Light bleeding → already fixed in rc_lighting.rs (emission coverage pre-pass)

---

## 1. CA Flow Algorithm

### Data per cell

Existing `FluidCell { fluid_id: FluidId, mass: f32 }` — no changes needed.

### Constants

| Constant | Value | Source |
|----------|-------|--------|
| `MAX_MASS` | 1.0 | hardcoded |
| `MIN_MASS` | 0.001 | hardcoded, cells below this are cleared |
| `MAX_COMPRESS` | per-fluid | `FluidDef.max_compress` (default 0.02) |
| `MIN_FLOW` | 0.01 | hardcoded, prevents infinite micro-oscillation |

### Flow order per non-empty cell (liquid)

1. **Down** — stable state formula:
   ```
   total = cell.mass + below.mass
   if total <= MAX_MASS:
       stable_below = total
   else:
       stable_below = MAX_MASS + max_compress * (total - MAX_MASS)
   flow = cell.mass - (total - stable_below)
   flow = clamp(flow, 0, cell.mass)
   if flow < MIN_FLOW: skip
   ```
   If below has different fluid_id and is non-empty:
   - Check reaction → if found, execute reaction (Section 2)
   - No reaction → density check: if `self.density > below.density` → **full swap**, mark both settled
   - If `self.density <= below.density` → skip

2. **Left/Right** — equalization:
   ```
   flow = (cell.mass - neighbor.mass) / 4.0
   ```
   Clamped to remaining mass, skip if < MIN_FLOW.
   If neighbor has different fluid_id and is non-empty:
   - Check reaction → execute if found
   - No reaction → density check: heavy displaces light via **full swap**, both settled
   - Light does not displace heavy → skip

3. **Up (pressure)** — only when `cell.mass > MAX_MASS`:
   ```
   flow = cell.mass - MAX_MASS
   flow = clamp(flow, 0, MAX_MASS - above.mass)
   ```
   Only into empty or same-type cells.

### Gas flow

Same rules but Y-inverted: "down" = up, "up(pressure)" = down. Iteration order: top-to-bottom.

### Global iteration

- Simulation runs on **all cells across all active chunks** as one continuous field
- Addressing: global tile coords `(tx, ty)` → chunk `(cx, cy)` + local `(lx, ly)`
- Iteration: bottom-to-top by Y, alternating L→R / R→L by X per tick
- `ActiveFluidChunks: HashSet<(i32, i32)>` — chunks with fluid or adjacent to fluid
- After tick: chunks with no changes → removed from active set (sleep)
- Wake: block placed/removed near fluid, or fluid arrived from neighbor

### Anti-oscillation guarantees

- Density swap is one-directional: heavy→light only. After swap, positions are equilibrium.
- `settled` flag per cell per tick: swapped cells are not reprocessed.
- `MIN_FLOW` threshold: flows below 0.01 are skipped.
- Alternating horizontal iteration direction removes directional bias.

---

## 2. Instant Reactions

Checked inline during CA flow. Before transferring mass from cell A to cell B:

1. If `A.fluid_id == B.fluid_id` or `B.is_empty()` → normal flow
2. Different fluid_ids, both non-empty → look up `CompiledReaction` by `(fluid_a, fluid_b)` pair
3. Found → check `min_mass_a`, `min_mass_b` thresholds
4. Execute:
   - Subtract `consume_a` from A, `consume_b` from B
   - If `result_tile` → place tile at B's position
   - If `result_fluid` → fill cell B with this fluid
   - If `byproduct_fluid` → place in cell A (gas rises on next tick)
5. Flow does NOT happen — reaction replaces flow

Lookup: linear scan of `FluidReactionRegistry.reactions` (≤10 reactions for 5 fluid types).

---

## 3. Visual Waves (spring model)

Surface cells: non-empty cell with empty cell above.

### Data

```rust
struct WaveColumn {
    height: f32,    // displacement from baseline (-0.5..+0.5 tile fraction)
    velocity: f32,
}

// Resource
struct WaveBuffer {
    columns: HashMap<(i32, i32), WaveColumn>,  // keyed by global tile coords
}
```

Not serialized. Created/destroyed dynamically as surface changes.

### Update (every frame, not every tick)

```
force = SPRING_K * (left.height + right.height - 2.0 * self.height)
velocity += force * dt
velocity *= DAMPING  // e.g. 0.95
height += velocity * dt
```

`SPRING_K` scaled by `FluidDef.wave_speed`. Final render displacement scaled by `FluidDef.wave_amplitude`.

### Wave sources

- Splash (entity enters water) → initial velocity impulse on ~3 columns
- Mass increase (new flow arriving) → small impulse
- Reaction (water+lava) → explosive impulse in ~3 tile radius

---

## 4. Splash Particles

### Detection

`FluidContactState` component on entities. Each frame: check cell at entity position.
- Previous frame `last_fluid == NONE`, current frame `!= NONE` → emit `SplashEvent`

### SplashEvent

```rust
struct SplashEvent {
    position: Vec2,
    fluid_id: FluidId,
    intensity: f32,  // from fall velocity, 0.0..1.0
}
```

### Behavior

- Spawn `N = (intensity * 12) as usize` particles via existing particle pool
- Particles: fan upward+outward, gravity-affected, color from `FluidDef.color`
- Reabsorption: particles landing on same fluid type → disappear, add small mass
- Wave impulse: set `velocity` on WaveColumns in ~2-3 tile radius

---

## 5. Rendering (fluid data texture + shader)

### Per-chunk fluid texture

- Size: `chunk_size × chunk_size` pixels (e.g. 32×32 = 4 KB)
- Each pixel: `RGBA` from `FluidDef.color`, alpha scaled by `mass.min(1.0)`
- Empty cell → `(0, 0, 0, 0)`
- Filtering: **Bilinear** (`Linear`) — GPU interpolates between adjacent cells
- Rebuild only when chunk is dirty

### Mesh

One quad per chunk covering the full chunk area. Shader does all visual work.

### WGSL shader

Uniforms:
- `fluid_texture: texture_2d<f32>` — fluid data texture
- `time: f32` — for ripple animation
- `wave_data: array<f32, CHUNK_SIZE>` — wave displacement per column

Fragment:
- Sample `fluid_texture` with bilinear sampler → smooth boundaries
- Discard if alpha < 0.01
- Surface detection: sample pixel above, if alpha < 0.01 → this is surface
- Surface ripple: `sin(time * wave_speed + position.x * freq)` modulates UV.y
- Output: sampled color with alpha blending

### Visual results of bilinear filtering

- Water/air boundary → smooth alpha fadeout (not blocky)
- Water/lava boundary → smooth blue-to-red gradient
- Partial mass cells → semi-transparent, blends with neighbors

### Z-order

Fluid mesh renders after terrain tiles, before entities. Single layer.

---

## 6. Debug Tools & System Integration

### Debug controls (dev only)

- F5: toggle debug fluid mode
- F6/F7: cycle fluid type
- Click: place fluid, Shift+Click: remove fluid
- Overlay: fluid type + mass under cursor

### Bevy systems (execution order)

```
FluidPlugin:
  1. fluid_tick_accumulator    — accumulate dt, decide when to tick
  2. fluid_simulation          — CA flow + reactions (fixed tick)
  3. fluid_wave_update         — spring model (every frame)
  4. fluid_splash_detection    — FluidContactState → SplashEvent
  5. fluid_splash_spawn        — spawn particles from SplashEvent
  6. fluid_texture_rebuild     — update GPU textures for dirty chunks
  7. fluid_debug_input         — debug key handling (dev only)
```

### Resources

```rust
FluidSimConfig { tick_rate: f32, min_flow: f32, max_mass: f32 }
FluidTickAccumulator { accumulated: f32 }
ActiveFluidChunks { chunks: HashSet<(i32, i32)> }
WaveBuffer { columns: HashMap<(i32, i32), WaveColumn> }
```

`FluidRegistry` and `FluidReactionRegistry` already exist, loaded via RON pipeline.

### File structure

```
src/fluid/
  mod.rs          — FluidPlugin, system registration
  cell.rs         — FluidCell, FluidId, FluidContactState (exists)
  registry.rs     — FluidDef, FluidRegistry (exists)
  reactions.rs    — FluidReactionDef, FluidReactionRegistry (exists)
  simulation.rs   — NEW: CA flow + density displacement + reaction execution
  wave.rs         — NEW: WaveColumn, WaveBuffer, spring model
  render.rs       — NEW: fluid data texture, mesh, dirty tracking
  splash.rs       — NEW: SplashEvent, splash detection, particle spawn
  debug.rs        — NEW: debug fluid placement

assets/engine/shaders/fluid.wgsl — NEW: fluid rendering shader
```
