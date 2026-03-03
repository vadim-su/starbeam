# Hybrid Water Engine Design

## Goal

Replace the current basic fluid rendering (single sin wave, no interactions) with a hybrid CA + particle water engine that produces realistic waves, splashes, and dynamic interactions while scaling to large worlds with oceans.

## Architecture

Four layers connected by an event bus:

```
┌─────────────────────────────────────────────────┐
│                   RENDERING                      │
│  CA water: fluid mesh + shader (multi-octave)   │
│  Particles: metaball shader (render-to-texture) │
│  Waves: wave_height → vertex offset in shader   │
└──────────────┬──────────────────┬────────────────┘
               │                  │
┌──────────────▼───┐  ┌──────────▼────────────────┐
│   CA Simulation  │  │   Particle Simulation     │
│   (bulk water)   │◄─┤   (splashes, streams)     │
│                  ├─►│                            │
│  wave_height buf │  │  gravity, collision,       │
│  wave_velocity   │  │  reabsorption into CA      │
└──────────────────┘  └───────────────────────────┘
               ▲                  ▲
               │    Events        │
         ┌─────┴──────────────────┴──────┐
         │      WaterImpactEvent bus     │
         │  (player, NPC, items, pour)   │
         └───────────────────────────────┘
```

**Principle**: CA handles oceans and lakes (cheap, scales to huge worlds). Particles spawn only during interactions (expensive, but local and short-lived). Mass is conserved across CA ↔ particle transitions.

## 1. Wave Propagation

A wave layer on top of CA simulation, separate from fluid mass.

### Buffer

Per active chunk, two `Vec<f32>` of size `chunk_size × chunk_size`:
- `wave_height[i]` — current vertical surface displacement (can be negative)
- `wave_velocity[i]` — rate of change

### Update (each tick, only for fluid cells)

```
vel[i] += (avg_neighbor_heights - height[i]) * speed - damping * vel[i]
height[i] += vel[i]
if abs(height[i]) < epsilon: height[i] = 0  // decay to zero
```

### Event integration

`WaterImpactEvent` writes impulse into `wave_velocity[cell]`. Amplitude depends on object mass and velocity.

### Rendering integration

`wave_height` passed via vertex attribute. Shader offsets surface vertices. Layered on top of shader ripples.

### Cross-chunk

Exchange `wave_height` at boundaries, same approach as `reconcile_chunk_boundaries`.

### Decay

Waves die within 2-4 seconds via damping ~0.97. Chunks with all heights ≈ 0 are skipped.

## 2. Particle System + CA ↔ Particle Transition

### Particle struct

```rust
struct Particle {
    position: Vec2,
    velocity: Vec2,
    mass: f32,          // how much fluid this particle carries (for CA return)
    fluid_id: FluidId,  // which fluid
    lifetime: f32,      // max lifetime
    age: f32,           // current age
    size: f32,          // radius for metaball rendering
}
```

### Pool

Pre-allocated ring buffer, max ~2000-4000 simultaneous particles (configurable limit). When limit reached, oldest particles force-reabsorbed into CA.

### Particle physics (each frame)

1. `velocity.y -= gravity * dt`
2. `position += velocity * dt`
3. Tile collision — bounce or stick
4. If particle enters a cell with same fluid type → **reabsorption**: mass returns to CA cell, particle dies, impulse written to `wave_velocity`

### CA → Particles (displacement)

Player jumps into water:
1. Detector sees: entity crossed fluid surface with velocity `v`
2. Compute `displaced_mass = min(entity_volume, cell.mass) * splash_factor`
3. CA cell loses `displaced_mass`
4. Spawn 8-20 particles with total mass = `displaced_mass`
5. Initial velocity: fan-shaped upward+sideways, proportional to `v`
6. Simultaneously write impulse to `wave_velocity` for wave

### Particle → CA (reabsorption)

Droplet falls back:
1. Particle position enters a cell with fluid (or hits fluid surface)
2. `cell.mass += particle.mass`
3. `wave_velocity[cell] += particle.velocity.y * absorption_impulse`
4. Particle removed

Mass is always conserved: what was displaced returns.

## 3. Metaball Particle Rendering

Two-pass rendering for particles that merge visually.

### Pass 1 — Accumulation (render-to-texture)

- Each particle drawn as a soft circle (gaussian blob) into offscreen texture
- Formula: `intensity = exp(-distance² / radius²)`
- Nearby particles: intensities add up
- Texture format: `R16Float`, size = viewport / 2 (half resolution for performance)

### Pass 2 — Threshold + color (fullscreen quad)

- Read accumulation texture
- Where `intensity > threshold` (≈ 0.5) → draw water with FluidDef color
- Below threshold → transparent
- At the edge (near threshold) → soft alpha transition for anti-aliasing
- Apply same lightmap as CA water

### Result

Individual drops look like round droplets. When 3-4 fly close together they merge into a stream. When falling back into a lake they visually blend into CA water.

### Performance

- 2000 particles = 2000 quads in one draw call (GPU instancing)
- Half-resolution texture — natural blur
- Threshold pass — one fullscreen quad

## 4. Shader Ripples (multi-octave)

Replace current single `sin()` with layered ripples. Purely GPU, zero CPU cost.

### 3 octaves

| Octave | Frequency | Amplitude | Speed | Purpose |
|--------|-----------|-----------|-------|---------|
| Base   | 1.5       | 1.2 px    | 1.0   | Slow main wave |
| Mid    | 4.0       | 0.5 px    | 1.8   | Medium wave |
| Detail | 9.0       | 0.2 px    | 3.0   | Fine ripple |

### Vertex shader

```
wave = base_sin + mid_sin + detail_sin
// each sin with slightly different direction (not purely along X)
// base:   sin(x * 1.5 + time * 1.0)
// mid:    sin(x * 4.0 + y * 0.5 + time * 1.8)
// detail: sin(x * 9.0 - y * 1.2 + time * 3.0)
```

Plus dynamic `wave_height` from wave propagation (section 1). Final vertex offset = `shader_ripple + dynamic_wave_height`.

### Parameterization from FluidDef

- Water: all 3 octaves, fast ripple
- Lava: base octave only, slow, large amplitude
- Gas: high-frequency fine ripple

Wave parameters passed via vertex attribute or uniform from FluidDef at load time.

## 5. Event Bus and Detectors

### Event

```rust
pub struct WaterImpactEvent {
    pub position: Vec2,
    pub velocity: Vec2,
    pub kind: ImpactKind,
    pub fluid_id: FluidId,
    pub mass: f32,           // object mass (affects splash strength)
}

pub enum ImpactKind {
    Splash,    // enter/exit water
    Wake,      // movement in water
    Pour,      // fluid stream hits surface
}
```

### 5 detectors (separate systems)

1. **`detect_entity_water_entry`** — each frame checks entities with `PhysicsBody`. If previous frame = air, current = fluid (or vice versa) → `Splash`. Strength proportional to `velocity.y`.

2. **`detect_entity_swimming`** — entity inside fluid and `velocity.length() > threshold` → `Wake` every 0.15s. Small waves on sides.

3. **`detect_fluid_pour`** — during CA simulation: if fluid falls down (mass transferred vertically) and below is standing fluid → `Pour` at impact point.

4. **`detect_item_water_entry`** — dropped items cross fluid surface → `Splash` with small mass.

5. **`detect_block_water_displacement`** — player places/removes block inside fluid → `Splash` at that point.

### Consumers (read `EventReader<WaterImpactEvent>`)

- Wave system: writes impulse to `wave_velocity[cell]`
- Particle system: spawns particles (count and velocity depend on `kind` + `mass` + `velocity`)

### Response tuning by kind

| Kind   | Particles           | Wave                        | Example              |
|--------|---------------------|-----------------------------|----------------------|
| Splash | 8-20, fan upward    | strong impulse ±3 cells     | jump into lake       |
| Wake   | 1-3, sideways       | weak impulse ±1 cell        | swimming             |
| Pour   | 2-5, small upward   | medium impulse ±2 cells     | waterfall into lake  |

## Key Decisions

- **Hybrid CA + particles**: CA for bulk water (oceans scale), particles only for interactions
- **Mass conservation**: displaced CA mass becomes particle mass, reabsorbed on return
- **2D wave buffer per chunk**: wave equation simulation for dynamic waves
- **Metaball rendering**: two-pass (accumulate gaussian → threshold) for merging droplets
- **Multi-octave shader ripples**: 3 sin layers replace single sin, parameterized per fluid type
- **Particle pool limit**: 2000-4000 max, oldest force-reabsorbed when exceeded
- **General particle system**: reusable for fire, dust, smoke later
