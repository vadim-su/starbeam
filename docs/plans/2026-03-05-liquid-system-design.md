# Liquid System Design — Pipe Model

## Requirements

- **Style**: Starbound — tile-based liquid with fill levels and pressure
- **Liquid types**: Water (density 1.0), lava (3.0), oil (0.8) with reactions and density-based displacement
- **Pressure**: Full hydrostatic pressure — U-tubes, communicating vessels
- **Scale**: Oceans spanning tens of thousands of tiles
- **Simulation rate**: 15-20 FPS (FixedUpdate), visual interpolation to 60 FPS
- **Player interaction**: Swimming, slowdown, damage, buckets for collection/placement
- **Chunks**: Simulation radius = render radius + 1

## Approach: Pipe Model

Chosen over Cellular Automata (unstable pressure, oscillations) and hybrid CA+solver (two systems, complex transitions). Pipe Model gives physically correct pressure from the box via flow-based simulation.

## Data Model

### Per-tile liquid data

```rust
struct LiquidCell {
    liquid_type: LiquidId,   // u8 index into registry
    level: f32,              // volume 0.0..1.0
    flow: [f32; 4],          // flow through faces: [right, up, left, down]
}
```

### Chunk storage

```rust
struct ChunkData {
    fg: TileLayer,
    bg: TileLayer,
    liquid: LiquidLayer,  // Vec<Option<LiquidCell>>, chunk_size²
}
```

`Option<LiquidCell>` — most tiles are empty or solid, no wasted memory.

### Liquid registry (data-driven, RON)

```rust
struct LiquidDef {
    name: String,
    density: f32,         // water=1.0, oil=0.8, lava=3.0
    viscosity: f32,       // flow resistance coefficient
    color: Color,
    damage_on_contact: f32,
    light_emission: f32,
    reactions: Vec<LiquidReaction>,
}

struct LiquidReaction {
    other: LiquidId,
    result: ReactionResult, // ProduceTile(TileId) | ProduceLiquid(LiquidId) | Destroy
}
```

## Simulation Algorithm

Three phases per step:

### Phase 1: Pressure computation

```
pressure(x, y) = level(x, y) + density * gravity * depth(x, y)
```

Depth computed iteratively top-down per column — no surface search needed.

### Phase 2: Flow computation

For each face between cells A and B:

```
flow(A→B) += dt * (pressure(A) - pressure(B)) / viscosity
flow(A→B) = clamp(flow, -max_flow, max_flow)
```

Validation — outgoing flows cannot exceed cell volume:

```
total_out = sum(max(0, flow[i]) for i in 0..4)
if total_out > level:
    scale = level / total_out
    flow[i] *= scale  // for all outgoing
```

Gravity adds bias to downward flow. Viscosity reduces all flows (oil slower than water, lava slowest).

### Phase 3: Level update

```
new_level = old_level - sum(outgoing_flows) + sum(incoming_flows)
if new_level < 0.001: remove cell
```

### Multi-liquid: density displacement

When liquid A flows into cell containing liquid B:
- `density(A) > density(B)`: A displaces B upward (B gets upward flow)
- `density(A) < density(B)`: flow blocked
- Oil (0.8) floats on water (1.0), lava (3.0) sinks below everything

### Reactions

Checked in Phase 3. If two liquid types meet in a cell:
- Water + Lava → obsidian tile + steam particles
- Oil + Lava → fire particles + oil destroyed

### Sleep optimization

Cell sleeps when all flows < epsilon for N consecutive steps. Sleeping cells skip processing. Wake trigger: neighbor cell changes (tile broken, liquid added).

## Rendering

### Fill level visualization

Each cell rendered as rectangle filled from bottom by `clamp(level, 0.0, 1.0)` of tile height.

### Surface smoothing

Linear interpolation of levels between neighboring cells for top edge. Shader receives `level_left`, `level_self`, `level_right` and interpolates the surface line.

### Temporal interpolation

Two states stored: `prev` and `curr`. Render uses:

```
render_level = lerp(prev_level, curr_level, t)
```

Where `t` = time since last sim step / step duration. Smooth 60 FPS visuals from 20 FPS simulation.

### Mesh

Separate `LiquidMesh` per chunk, analogous to tile mesh in `mesh_builder.rs`. Rebuilt only on dirty flag. Interpolation via shader uniform, no mesh rebuild between sim steps.

### Colors and transparency

- Water: semi-transparent blue, tiles visible behind
- Lava: opaque, emits light (RC lighting integration)
- Oil: dark, semi-transparent

### Surface animation

Optional sine wave in shader (`sin(x * freq + time)`), 1-2px amplitude for idle water.

## Integration

### Player physics

In `physics.rs`, after tile collision:
- **Swimming**: buoyancy force when submerged above waist (depends on liquid density)
- **Slowdown**: velocity × `viscosity_factor` from LiquidDef
- **Damage**: `damage_on_contact` per tick

### Tile interaction

- Breaking tile near liquid → wakes neighbor cells, liquid flows in
- Placing tile in liquid cell → displaces liquid to neighbors
- Solid tiles block flows through their faces

### Buckets

Item "bucket": on use, collects up to 1.0 volume + records LiquidId. On place, pours back. Via `block_action.rs`.

### World generation

`terrain_gen.rs` fills `LiquidLayer` during chunk generation:
- Below sea level + no solid tile → water (level = 1.0)
- Lava lakes in deep caves
- Oil pockets in specific biomes

### Chunk boundaries

Simulation radius = `chunk_load_radius + 1`. Flows blocked at absolute boundary. On chunk load — wake all cells at seam.

### Lighting

Integrates with RC lighting:
- `light_opacity` per liquid type (water slight, oil heavy)
- `light_emission` for lava
- Additional data layer in `rc_lighting.rs`

### System ordering

```
GameSet::Physics → GameSet::LiquidSim → GameSet::WorldUpdate
```

`LiquidSim` runs on FixedUpdate ~20Hz.

## Edge Cases

### Horizontal wrap-around

Flows across `x=2047 → x=0` work normally via existing chunk wrap logic.

### Volume loss

Float precision losses ignored — sub-epsilon. Cells with level < 0.001 are removed.

### Oceans at world start

Generated as level=1.0, immediately marked sleeping. Zero compute until disturbed.

### Cascade wakeup

Breaking ocean wall → cascade. Capped at ~10-20k active cells per step. Overflow deferred to next step — visually reads as realistic flood wave.

### Two liquids in one cell

Not allowed. On contact: reaction if defined, otherwise density displacement. If displacement impossible (nowhere to go) — flow blocked.

### Save/load

`LiquidLayer` serialized with chunk. Flows zeroed on save (only level + type persisted). On load all cells start sleeping.
