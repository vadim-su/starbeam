# Lighting System Design

**Date:** 2026-02-27
**Status:** Approved
**Style:** Starbound-style RGB colored lighting

## Overview

Starbound-style lighting system for the Starbeam 2D sandbox. RGB colored light with per-vertex smooth interpolation, sunlight top-down propagation, and BFS point light flood-fill.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Light style | Starbound-style (RGB colored) | Best visual quality for 2D sandbox |
| Sources (v1) | Sunlight + point lights | Minimum playable set (surface/underground contrast + torches) |
| Color model | RGB `[u8; 3]` per tile | Colored light is Starbound's core identity; retrofitting mono→RGB is painful |
| Propagation | Multi-pass BFS | Pass 1: sunlight columns, Pass 2: point light BFS, merge via `max()` per channel |
| Recalc granularity | Per-chunk on generation + local recalc (radius 16) on tile change | No lag spikes, scales well |
| Interpolation | Per-vertex corner averaging | Industry standard for tile games, smooth gradients, free on GPU |
| Day/night | Static sunlight (v1) | Easy to extend later with a uniform + GameTime resource |
| Tile properties | `light_emission: [u8; 3]` + `light_opacity: u8` (0–15) | Minimal sufficient set |

## Architecture

Three layers:

```
┌─────────────────────────────────────┐
│  GPU Layer (Shader)                 │
│  custom vertex shader → pass RGB    │
│  fragment: color.rgb * light.rgb    │
├─────────────────────────────────────┤
│  Mesh Layer (mesh_builder.rs)       │
│  per-vertex corner averaging        │
│  ATTRIBUTE_LIGHT: Float32x3 (RGB)   │
├─────────────────────────────────────┤
│  Simulation Layer (lighting.rs)     │
│  sunlight pass (top-down columns)   │
│  point light pass (BFS flood-fill)  │
│  merge: max(sun, point) per channel │
│  local recalc on tile change        │
└─────────────────────────────────────┘
```

## Simulation Layer

### Sunlight Pass (top-down columns)

For each column x, scan top-to-bottom:

```
light = SUN_COLOR [255, 250, 230]
for y in (max..min):
    if tile == AIR:
        light_levels[x, y] = light     // passes through
    else:
        opacity = tile_def.light_opacity
        light[channel] = max(light[channel] - opacity * 17, 0)  // 15 * 17 = 255
        light_levels[x, y] = light
```

- O(width x height), linear, instant.
- `opacity * 17` maps 0–15 to 0–255 attenuation.
- Solid tiles with opacity 15 fully block light (15 x 17 = 255).

### Point Light Pass (BFS flood-fill)

```
for each tile with emission != (0,0,0):
    queue.push((x, y, emission))
    while let Some((x, y, light)) = queue.pop():
        if light <= (0,0,0): continue
        if light <= existing_point_light[x, y]: continue  // per-channel
        point_light[x, y] = max(point_light[x, y], light)
        for each neighbor (nx, ny):
            opacity = tile_def[neighbor].light_opacity
            spread = light - opacity * 17 - FALLOFF        // FALLOFF = 16-18 per step
            queue.push((nx, ny, spread))
```

- Radius ~14-16 tiles (255 / 17 ~ 15 steps in air).
- FALLOFF: base attenuation per distance (light is not infinite even in air).
- BFS guarantees closest path processed first.

### Merge

```
final_light[x, y] = max(sunlight[x, y], point_light[x, y])  // per-channel RGB
```

### Local Recalc on Tile Change

- On tile break/place: recalculate light in radius 16 tiles from change.
- Sunlight: recalculate affected columns top-to-bottom.
- Point lights: BFS from all emitters within radius.
- Dirty-mark all affected chunks for mesh rebuild.

## Mesh Layer

### Per-Vertex Corner Averaging

Each tile quad has 4 vertices. Each vertex averages light from 4 tiles sharing that corner:

```
  TL ●───────● TR      top-left  = avg(tile, left, above, above-left)
     │       │          top-right = avg(tile, right, above, above-right)
     │ tile  │          bot-left  = avg(tile, left, below, below-left)
     │       │          bot-right = avg(tile, right, below, below-right)
  BL ●───────● BR
```

For chunk border tiles: fallback to own value if neighbor chunk not loaded.

### Data Changes

- `ATTRIBUTE_LIGHT`: `Float32` → `Float32x3`
- `MeshBuildBuffers.lights`: `Vec<f32>` → `Vec<[f32; 3]>`
- Each vertex gets unique `[f32; 3]` (not 4 identical values)
- GPU interpolates between vertices automatically (varying)

## GPU Layer

### Custom Shader (`assets/shaders/tile.wgsl`)

```wgsl
#import bevy_sprite::mesh2d_functions

struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) light: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light: vec3<f32>,
}

@vertex
fn vertex(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = mesh2d_position_local_to_clip(
        mesh2d_functions::get_world_from_local(in.instance_index),
        vec4<f32>(in.position, 1.0),
    );
    out.uv = in.uv;
    out.light = in.light;
    return out;
}

@group(2) @binding(0) var atlas_texture: texture_2d<f32>;
@group(2) @binding(1) var atlas_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(atlas_texture, atlas_sampler, in.uv);
    if color.a < 0.01 {
        discard;
    }
    return vec4<f32>(color.rgb * in.light, color.a);
}
```

### TileMaterial Change

Add `vertex_shader()` override pointing to same file:

```rust
impl Material2d for TileMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/tile.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "shaders/tile.wgsl".into()
    }
}
```

## Tile Properties

### New TileDef Fields

```rust
pub light_emission: [u8; 3],  // RGB emission. (0,0,0) = no light
pub light_opacity: u8,        // 0 = transparent (air), 15 = full block (stone)
```

### Tile Values

| Tile | emission | opacity | Notes |
|------|----------|---------|-------|
| air | (0,0,0) | 0 | Light passes freely |
| grass | (0,0,0) | 15 | Fully blocks |
| dirt | (0,0,0) | 15 | Fully blocks |
| stone | (0,0,0) | 15 | Fully blocks |
| torch (future) | (240,180,80) | 0 | Warm orange |
| lava (future) | (255,80,20) | 0 | Red glow |
| glass (future) | (0,0,0) | 1 | Nearly transparent |

## System Ordering

```
GameSet::WorldUpdate:
  chunk_loading_system          // generate chunks
  compute_chunk_lighting        // light for new chunks (after generation)

// On tile break/place (block_action):
  modify_tile → local_light_recalc (radius 16) → mark dirty chunks

GameSet::WorldUpdate (chained):
  rebuild_dirty_chunks          // mesh rebuild with new light_levels
```

## Data Flow

```
Tile placed/broken
       │
       ▼
light_system (Simulation Layer)
  ├─ sunlight pass: top-down columns, opacity blocking
  ├─ point light pass: BFS from emitters, falloff + opacity
  └─ merge: max(sun, point) per RGB channel
       │
       ▼
ChunkData.light_levels: Vec<[u8; 3]>
       │
       ▼
build_chunk_mesh (Mesh Layer)
  └─ per-vertex corner averaging (4 neighbors per corner)
       │
       ▼
Mesh { ATTRIBUTE_LIGHT: Float32x3 }
       │
       ▼
tile.wgsl (GPU Layer)
  ├─ vertex: pass-through light as varying
  └─ fragment: color.rgb * light.rgb
       │
       ▼
Pixel on screen (lit & smooth)
```

## Constants

```rust
const SUN_COLOR: [u8; 3] = [255, 250, 230];  // warm white
const LIGHT_FALLOFF: u8 = 17;                 // per-step base attenuation in air
const OPACITY_SCALE: u8 = 17;                 // opacity 0-15 → attenuation 0-255
const MAX_LIGHT_RADIUS: i32 = 16;             // max BFS/recalc radius
```

## Future Extensions (out of scope for v1)

- **Day/night cycle:** Replace `SUN_COLOR` constant with uniform, add `GameTime` resource.
- **Light filter (colored glass):** Add `light_filter: [u8; 3]` to TileDef — modifies color instead of blocking.
- **Background walls:** Separate light layer for background tiles.
- **Ambient per layer:** Depth-based minimum light level underground.
