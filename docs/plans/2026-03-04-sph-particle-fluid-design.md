# SPH Particle-Based Fluid Simulation Design

## Context

Replace cellular automata (CA) water system with SPH (Smoothed Particle Hydrodynamics) for improved physics and visuals. Gases (steam, toxic_gas, smoke) remain on CA.

## Requirements

- 20K+ particles, 30+ FPS target
- Liquids only (water, lava) on SPH; gases stay on CA
- Hybrid metaballs + pixelization rendering
- Tile collision integration with existing WorldMap

---

## Section 1: SPH Simulation

### Particle Data

```rust
struct FluidParticle {
    position: Vec2,
    velocity: Vec2,
    density: f32,
    pressure: f32,
    fluid_id: FluidId,
    mass: f32,
}
```

### Spatial Hashing

- Cell size = smoothing radius `h` (~8-12 px)
- Hash: `hash(floor(x/h), floor(y/h))`
- Neighbor search: 9 cells (3x3)
- Structure: `HashMap<(i32, i32), Vec<usize>>`, rebuilt each tick

### Simulation Steps (per tick)

1. Build spatial hash — O(n)
2. Compute density — Poly6 kernel over neighbors
3. Compute pressure — equation of state: `P = k * (ρ - ρ₀)`
4. Compute forces — pressure (Spiky kernel) + viscosity (Laplacian kernel) + gravity
5. Integrate — symplectic Euler: `v += (f/ρ) * dt`, `x += v * dt`
6. Tile collision — project out of solid tiles, reflect velocity
7. Boundary enforcement — world edges

### Timestep

- Fixed 60Hz accumulator (existing pattern)
- dt = ~0.016s — small enough for SPH stability without sub-steps in most cases

---

## Section 2: Rendering (Metaballs + Pixelization)

### Pipeline

1. **Render particles → offscreen texture** (low res, ~1/4 screen)
   - Each particle = soft circle (Gaussian falloff)
   - Additive blending in R-channel (scalar field)
   - Fluid color in G/B channels (weighted average)

2. **Threshold pass** (fragment shader)
   - Sum > threshold → fluid visible
   - Smooth edges, droplets merge automatically

3. **Pixelization pass**
   - Downscale to tile-size grid (8×8 or 16×16 px per tile)
   - Snap edges to pixel grid
   - Result: pixel-art water with smooth dynamics

4. **Final composite**
   - Overlay on tile world
   - Apply color, caustics, transparency from existing shader

### Why offscreen texture, not mesh

- 20K quads = expensive; offscreen low-res = fast
- Threshold in shader = free particle merging
- Pixelization = natural downscale

---

## Section 3: Tile Collisions

### Particle-vs-Tile

1. Determine tile at new position: `tile = floor(position / tile_size)`
2. If solid — project particle out to nearest tile edge
3. Reflect velocity with energy loss: `v_normal *= -restitution` (0.1-0.3)
4. Surface friction: `v_tangent *= (1.0 - friction)`

### Multi-tile check

Check 2×2 nearest tiles around particle position to handle corner cases.

### Reuse

- `FluidWorld::is_solid(gx, gy)` reused for collision queries
- X-axis wrapping preserved

### World boundaries

- Bottom: solid wall
- Sides: wrapping (existing)
- Top: free

---

## Section 4: Integration with Existing Systems

### What stays / goes

| System | Status | Reason |
|--------|--------|--------|
| `FluidRegistry` / `FluidDef` (RON) | Keep | Fluid properties reused |
| `FluidId` | Keep | Fluid type identification |
| `WaterImpactEvent` | Keep | Entity-fluid interaction |
| `FluidReactionEvent` + `reactions.rs` | Keep, adapt | Reactions via particle proximity |
| `detectors.rs` | Adapt | Proximity check via spatial hash |
| `wave.rs` / `WaveBuffer` | Remove | SPH produces waves naturally |
| `splash.rs` | Remove | Particles are already splashes |
| `cell.rs` / `FluidCell` / `FluidSlot` | Remove | Replaced by FluidParticle |
| `fluid_world.rs` (CA grid) | Simplify | Only `is_solid()` + tile queries |
| `simulation.rs` (CA automata) | Replace | New SPH simulation |
| `render.rs` (mesh per cell) | Replace | Metaballs render pipeline |
| CA gases (steam, toxic_gas, smoke) | Keep on CA | Separate module |

### Entity-Fluid Detection

Spatial query via SPH spatial hash: "particles within radius R of entity?" — O(1) lookup.

### Bevy System Order

```
Update chain:
  1. sph_build_spatial_hash()
  2. sph_compute_density_pressure()
  3. sph_compute_forces()
  4. sph_integrate()
  5. sph_tile_collision()
  6. sph_reactions()
  7. detect_entity_fluid_contact()
  8. gas_ca_simulation()
  9. fluid_render()
```
