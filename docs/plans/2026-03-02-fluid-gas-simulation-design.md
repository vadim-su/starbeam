# Fluid & Gas Simulation Design

## Overview

Cellular automata (CA) based fluid and gas simulation for the 2D sandbox world.
Starbound-style approach: slightly compressible liquids with pressure modeled as
excess mass, multi-step iteration per frame, gases as inverted liquids.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Storage | Separate `fluids: Vec<FluidCell>` in `ChunkData` | Independent from fg/bg tiles; auto-serialization; no conflicts |
| Algorithm | Push-only CA + multi-step | Starbound-proven; configurable speed via iterations_per_tick |
| Pressure | Slightly compressible liquid | mass > 1.0 = pressure; U-tubes work; no explicit pressure tracking |
| Fluid types | Data-driven via RON files | Same pattern as TileRegistry; hot-reloadable |
| Gas model | Inverted liquid (flow up) | Same algorithm, `is_gas` flag flips primary direction |
| Reactions | Data-driven via RON | water + lava = stone + steam; extensible |
| Chunk scope | Loaded chunks + one edge read | Can read neighbor chunks but only write to current |
| Rendering | Semi-transparent quads with fill level | Minimal, visual verification possible |

## Data Structures

### FluidId

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct FluidId(pub u8);

impl FluidId {
    pub const NONE: FluidId = FluidId(0);
}
```

### FluidCell

```rust
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
pub struct FluidCell {
    pub fluid_id: FluidId,
    pub mass: f32,  // 0.0=empty, 1.0=full, >1.0=pressurized
}
```

### FluidDef (from RON)

```rust
pub struct FluidDef {
    pub id: String,
    pub density: f32,           // kg/m3, determines displacement order
    pub viscosity: f32,         // 0.0=instant, 1.0=slow (scales MaxSpeed)
    pub max_compress: f32,      // excess mass per depth unit (default 0.02)
    pub is_gas: bool,           // true = flow inverted (up instead of down)
    pub color: [u8; 4],         // RGBA for rendering
    pub damage_on_contact: f32,
    pub light_emission: [u8; 3],
    pub effects: Vec<String>,
}
```

### FluidReactionDef (from RON)

```rust
pub struct FluidReactionDef {
    pub fluid_a: String,
    pub fluid_b: String,
    pub adjacency: Option<String>,      // "above", "below", "side", None=any
    pub result_tile: Option<String>,     // creates solid tile
    pub result_fluid: Option<String>,    // transforms into fluid
    pub min_mass_a: f32,
    pub min_mass_b: f32,
    pub consume_a: f32,
    pub consume_b: f32,
    pub byproduct_fluid: Option<String>,
    pub byproduct_mass: f32,
}
```

### ChunkData extension

```rust
pub struct ChunkData {
    pub fg: TileLayer,
    pub bg: TileLayer,
    pub fluids: Vec<FluidCell>,  // NEW: chunk_size * chunk_size cells
    pub objects: Vec<PlacedObject>,
    pub occupancy: Vec<Option<OccupancyRef>>,
    pub damage: Vec<u8>,
}
```

## Algorithm

### Core Loop

```
for _ in 0..iterations_per_tick:
    prepare double-buffer (copy mass -> new_mass)
    for each active chunk:
        for each cell with fluid_id != NONE:
            def = registry.get(fluid_id)
            if def.is_gas:
                flow_vertical(UP)
                flow_horizontal()
                flow_vertical(DOWN)  // decompression
            else:
                flow_vertical(DOWN)
                flow_horizontal()
                flow_vertical(UP)    // decompression
    copy new_mass -> mass
    clear cells with mass < MIN_MASS
    process reactions
```

### get_stable_state(total_mass, max_compress) -> bottom_mass

Returns how much mass should be in the bottom cell of two vertically adjacent cells:

```
if total_mass <= 1.0:
    return total_mass                                    // all goes to bottom
elif total_mass < 2 * MAX_MASS + max_compress:
    return (MAX_MASS^2 + total_mass * max_compress)      // proportional compression
         / (MAX_MASS + max_compress)
else:
    return (total_mass + max_compress) / 2               // both full, bottom has +compress
```

### Flow Rules

**Rule 1 — Primary vertical (down for liquids, up for gases):**
- Calculate stable state for total mass of cell + neighbor
- Flow = target - neighbor.mass
- Smooth small flows (* 0.5 if flow > MIN_FLOW)
- Clamp to [0, min(max_speed * (1-viscosity), remaining_mass)]

**Rule 2 — Horizontal spreading:**
- flow = (cell.mass - neighbor.mass) / 4
- Same smoothing and clamping
- Process left, then right

**Rule 3 — Decompression (up for liquids, down for gases):**
- Only if cell.mass > MAX_MASS (pressurized)
- Calculate reverse stable state
- Same flow logic

### Constants

```
MAX_MASS = 1.0
MIN_MASS = 0.001
MIN_FLOW = 0.005
MAX_SPEED = 1.0 (modulated by viscosity)
```

## Reactions

Processed after CA step:
1. For each cell with fluid, check all 4 neighbors
2. If neighbor has different fluid, lookup reaction registry
3. If reaction matches (adjacency, min_mass), apply:
   - Consume specified mass from both cells
   - Create result tile if specified (set fg tile)
   - Create byproduct fluid if specified

### Density Displacement

Separate pass after reactions:
- If cell below has lighter fluid than cell above, swap
- Ensures heavy liquids sink, light gases rise

## Rendering

### Fluid Mesh (per chunk)

- For each cell with fluid: add semi-transparent quad
- Quad height = min(mass, 1.0) * tile_size (fill level)
- Color from FluidDef.color (RGBA)
- Falling fluids (flowing down) render as full cells
- Z-order: between fg tiles and player entities
- Rebuild on ChunkDirty flag

## ECS Integration

### Systems (in GameSet::WorldUpdate)

1. `fluid_simulation` — main CA loop
2. `fluid_reactions` — process fluid reactions and density displacement
3. `fluid_mark_dirty` — mark chunks with changed fluids as dirty
4. `fluid_mesh_rebuild` — rebuild fluid meshes for dirty chunks

### Systems (in GameSet::Physics)

1. `fluid_player_interaction` — slowdown, damage, breathing effects

### Resources

```rust
#[derive(Resource)]
pub struct FluidConfig {
    pub iterations_per_tick: u32,  // 2-5
    pub min_mass: f32,
    pub min_flow: f32,
    pub max_speed: f32,
}

#[derive(Resource)]
pub struct FluidRegistry { defs: Vec<FluidDef>, by_name: HashMap<String, FluidId> }

#[derive(Resource)]
pub struct FluidReactionRegistry { reactions: Vec<FluidReactionDef> }

#[derive(Resource)]
pub struct ActiveFluidChunks { chunks: HashSet<(i32, i32)> }
```

## Optimization

- **ActiveFluidChunks**: only simulate chunks with at least one fluid cell
- **Double-buffer**: swap two mass arrays instead of copying
- **Chunk boundary**: read neighbor chunk data, write only to current chunk
- **Horizontal wrap-around**: uses existing `config.wrap_tile_x()`
- **Sleep/wake**: chunks with settled fluids (no flow for N ticks) can be deactivated

## File Structure

```
src/fluid/
    mod.rs           — FluidPlugin, system registration
    cell.rs          — FluidId, FluidCell
    registry.rs      — FluidDef, FluidRegistry, RON loading
    simulation.rs    — CA algorithm (flow_down, flow_horizontal, flow_up)
    reactions.rs     — FluidReactionDef, FluidReactionRegistry
    render.rs        — build_fluid_mesh, rebuild system
    interaction.rs   — player-fluid interaction

assets/content/fluids/
    water.ron
    lava.ron
    steam.ron
    toxic_gas.ron
    smoke.ron
    reactions.ron
```

## Testing Strategy

Unit tests for:
- `get_stable_state()` correctness for various mass values
- U-tube scenario (two columns connected at bottom equalize)
- Gas rises upward
- Density displacement (heavy sinks, light rises)
- Reactions (water + lava = stone + steam)
- Horizontal wrap-around at world edge
- Chunk boundary flow (read neighbor, write only current)
- Active chunk tracking (add/remove from set)

## References

- [Tom Forsyth: Cellular Automata for Physical Modelling](https://tomforsyth1000.github.io/papers/cellular_automata_for_physical_modelling.html)
- [W-Shadow: Simple Fluid Simulation](https://w-shadow.com/blog/2009/09/01/simple-fluid-simulation/)
- [jgallant: 2D Liquid Simulator with CA](http://www.jgallant.com/2d-liquid-simulator-with-cellular-automaton-in-unity/)
- [GitHub: LiquidSimulator](https://github.com/jongallant/LiquidSimulator)
