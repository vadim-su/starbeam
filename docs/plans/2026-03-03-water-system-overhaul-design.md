# Water System Overhaul Design

## Summary

Refactoring the fluid system: keep and improve the working CA simulation core, completely rewrite shaders, add runtime reactions, integrate with RC lighting, and finish particle effects.

**Approach**: Enhanced single-pass shader (one custom material, one mesh per chunk, one fragment shader handles everything).

## Current State

**Working:**
- CA simulation: push-only compressible liquid, cross-chunk flow, density displacement
- Wave propagation buffers and simulation with cross-chunk reconciliation
- Event system (WaterImpactEvent) + 2/5 detectors (entity entry, swimming)
- Splash particles with mass conservation (CA extraction + reabsorption)
- Particle pool (ring buffer, 3000 capacity)
- 5 fluid types (water, lava, steam, toxic_gas, smoke) in RON

**Broken / Missing:**
- `ATTRIBUTE_WAVE_HEIGHT` is computed and passed to mesh but never read in shader — dynamic waves have zero visual effect
- `FluidReactionRegistry` exists but no system executes reactions at runtime
- Fluids invisible to RC lighting: lava doesn't emit into RC, water doesn't absorb light
- Particles pass through solid tiles
- No caustics, no depth darkening, no foam, no refraction
- `damage_on_contact` and `effects` fields defined but unprocessed

## Section 1: New Fluid Shader (fluid.wgsl)

### Vertex Shader — Wave Displacement

For upper vertices of surface cells (`is_wave_vertex` flag):
- `position.y += wave_height` — direct displacement from wave propagation system
- Plus procedural: `position.y += sin(world_x * freq + time * speed) * amplitude * wave_amplitude` (2 octaves)
- Solves "waves detach from surface" — vertices move instead of fragment discard

Pass to fragment:
- `depth_in_fluid` (f32): depth from nearest surface above (0.0 = surface, 1.0+ = deep)
- `is_surface` (bool): top row of cell
- `edge_flags` (u32): which sides border solid tiles (for foam)
- Per-fluid data: color, emission, wave params

### Fragment Shader — Layered Effects

Bottom to top:

1. **Base color** — `fluid_color` from FluidDef, rgba
2. **Depth darkening** — `color.rgb *= 1.0 - clamp(depth_in_fluid * 0.3, 0.0, 0.6)`. Per-fluid configurable.
3. **Caustics** (liquids only, not gas) — two layers of scrolling Voronoi noise at different scale/speed, additive blending. Intensity inversely proportional to depth (brighter on shallow). Pixelated UV: `floor(uv * pixel_density) / pixel_density` for pixel-art style.
4. **Lightmap** — `color.rgb *= lightmap_sample` (RC lighting, unchanged)
5. **Surface effects** (only `is_surface`):
   - Wave crest glint — thin bright band at top edge, brightness from `ddy(wave_height)` (wave slope)
   - Shore foam — whitish band where `edge_flags` indicate solid neighbor
6. **Emission** — `color.rgb = max(color.rgb, emission)` (lava glows in darkness)

### Caustics Algorithm

Procedural Voronoi noise (no texture):

```wgsl
fn caustic(uv: vec2f, time: f32) -> f32 {
    let puv = floor(uv * PIXEL_DENSITY) / PIXEL_DENSITY;
    let c1 = voronoi(puv * 3.0 + vec2(time * 0.4, time * 0.3));
    let c2 = voronoi(puv * 5.0 - vec2(time * 0.2, time * 0.5));
    return smoothstep(0.3, 0.0, min(c1, c2));
}
```

Intensity: `caustic_strength = clamp(1.0 - depth * 0.5, 0.0, 0.4)` — caustics fade below ~2 tiles depth.

### Mesh Builder Changes

New vertex attributes in `fluid_rebuild_meshes()`:
- **`ATTRIBUTE_DEPTH`** (f32): depth from nearest surface above. Computed top-to-bottom per column during mesh build.
- **`ATTRIBUTE_EDGE_FLAGS`** (u32): bitflags for which sides border solid tiles or air. Needed for foam and surface detection.
- Existing attributes (`fluid_data`, `wave_height`, `wave_params`) remain, but `wave_height` is now actually used in vertex shader.

## Section 2: Fluid Reactions

### Runtime Reaction System

New system: `execute_fluid_reactions()`, runs after each CA iteration.

**Algorithm:**
For each non-empty cell, check 4 neighbors. If neighbor has different fluid, lookup reaction in registry.

**Optimization:**
- Only check cells at boundaries between different fluids
- `FluidReactionRegistry` stores `HashMap<(FluidId, FluidId), CompiledReaction>` — O(1) lookup
- Rate limiting: max N reactions per iteration to prevent instant lava-to-stone conversion

**Reaction execution (example: lava + water):**
1. Subtract `consume_a` mass from lava, `consume_b` from water
2. If `result_tile` — place solid tile at reaction position
3. If `result_fluid` — create fluid with `byproduct_mass` above reaction point
4. Emit `FluidReactionEvent { position, reaction_type }` for VFX (steam particles, sound)
5. Optional: wave impulse at reaction point

**Density displacement** (`resolve_density_displacement()`) — already works, keep as-is.

## Section 3: RC Lighting Integration

### 3.1 Fluids as Light Sources

In `extract_lighting_data()` (rc_lighting.rs), add pass over `chunk.fluids`:
- If `fluid_def.light_emission != [0,0,0]` and `cell.mass > 0.1`, write emission to emitters texture
- Intensity proportional to `cell.mass`
- Result: lava illuminates caves, acid can glow faintly

### 3.2 Fluids as Light Absorbers

In RC density texture:
- Fluid cells get non-zero density: `density = cell.mass * fluid_def.light_absorption`
- Light attenuates through water, creating natural depth darkening via RC that complements shader darkening

### 3.3 New FluidDef Field

```rust
pub light_absorption: f32,  // 0.0 = transparent to light, 1.0 = fully blocks
```

Defaults:
| Fluid | light_absorption |
|---|---|
| water | 0.3 |
| lava | 0.8 |
| steam | 0.05 |
| toxic_gas | 0.1 |
| smoke | 0.4 |

## Section 4: Splash Effects and Bubble Trails

### 4.1 Splash Improvements

**Particle-tile collision** (currently particles fly through walls):
- Each tick: check `world_map.is_solid(particle_tile_pos)`
- On collision: stick + drain down (water), stick + solidify (lava)

**Entity-based splash mass** — replace hardcoded 5.0 with `entity_mass` component (or default from collider size).

**Missing detectors** — add:
- `detect_item_water_entry` — dropped items create small splash (2-3 particles)
- `detect_block_water_displacement` — block destruction near water → fill void + small splash

`detect_fluid_pour` deferred (depends on inventory/tools system).

### 4.2 Bubble Trails from Projectiles

New detector: `detect_projectile_in_fluid`
- For entities with `Projectile` + `Velocity` inside fluid
- Throttled every ~0.05s, spawn 1-2 bubble particles at projectile position
- Bubbles: small (2-3px), white/bright, negative gravity (float up), short lifetime (0.5-0.8s), fade-out
- No CA mass extraction (visual effect only)
- On water entry: mini-splash (3-5 particles) + wave impulse

### 4.3 Particle System Changes

Add to `Particle`:
```rust
pub gravity_scale: f32,  // 1.0 = normal, -0.3 = bubbles rise slowly
pub fade_out: bool,      // alpha decreases with age
```

Current quad renderer is sufficient. Metaballs can be added later.

## Section 5: CA Simulation Refactoring

### 5.1 Viscosity

Already works correctly via flow rate multiplier. No changes needed.

### 5.2 Eliminate Array Cloning

`simulate_grid()` clones `tiles` and `fluids` arrays each iteration (simulation.rs:160-161).

Solution: double-buffering — two arrays, swap pointers after each iteration. Allocate once on chunk activation.

### 5.3 Sleep/Wake for Stable Regions

If all flows in a chunk are < `MIN_FLOW` for N consecutive ticks, sleep the chunk. Wake on:
- New fluid added to chunk
- Neighbor chunk sends flow across boundary
- Block placed/destroyed in chunk

Significant optimization for large worlds with lakes.

## Implementation Phases

### Phase 1: Shader + Waves (visual breakthrough) — PRIORITY
1. Rewrite `fluid.wgsl` — vertex displacement, depth darkening, caustics, surface glint, foam
2. Update mesh builder — pass `depth_in_fluid`, `edge_flags`
3. Connect `wave_height` to vertex shader
4. Result: water is visually transformed, wave splashes are finally visible

### Phase 2: RC Integration (atmosphere)
5. Fluid emission into RC pipeline
6. Fluid light absorption in RC density
7. Add `light_absorption` to FluidDef + RON

### Phase 3: Reactions (gameplay)
8. `execute_fluid_reactions()` system
9. `FluidReactionEvent` → VFX (steam particles for lava+water)
10. Reaction rate limiting

### Phase 4: Particles and Trails (polish)
11. Particle-tile collision
12. Bubble trails from projectiles
13. Missing detectors (item entry, block displacement)
14. `gravity_scale` + `fade_out` in Particle

### Phase 5: Optimization (if needed)
15. Double-buffering instead of clone
16. Sleep/wake for stable chunks
