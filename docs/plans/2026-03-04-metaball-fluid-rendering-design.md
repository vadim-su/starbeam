# Metaball Fluid Rendering Design

## Problem

The current per-cell quad rendering has two conflicting issues:
1. Grid/stripe artifacts from discrete depth transitions at tile boundaries
2. Setting `render_fill=1.0` for non-surface cells makes thin streams (mass=0.05) render as full opaque tiles

These problems are fundamentally unsolvable with per-cell quads because they can't simultaneously show smooth continuous bodies for settled water AND proportional visual volume for streams/waterfalls.

## Solution: Density Texture + Metaball Field

Replace per-cell quads with a **density texture per chunk** approach. Each chunk gets one quad; the fragment shader computes a metaball field from the density texture and applies a hard threshold for pixel-perfect blob rendering.

## Requirements

- **Pixel-perfect blob**: Hard threshold, no blur, crisp pixel edges
- **Mass-proportional radius**: Low mass = smaller metaball radius (solves waterfall problem)
- **Effects**: Emission glow + lightmap integration only (remove caustics, shimmer, depth darkening, waves)
- **Chunk boundaries**: 1-cell padding from neighbors via FluidWorld
- **Multi-fluid**: One metaball algorithm for all fluid types, differ by color/emission
- **Performance**: Up to ~2000 cells on screen

## Architecture

### 1. Data Pipeline (CPU → GPU)

#### Density Texture
- **Format**: R8Unorm, size `(chunk_size + 2) × (chunk_size + 2)`
- Center `chunk_size × chunk_size` region = own cells
- 1-cell border padding from neighboring chunks via `FluidWorld`
- Each texel = `mass` value normalized to `[0.0, 1.0]`

#### Fluid ID Texture
- **Format**: R8Uint, same size as density texture
- Values: 0 = empty, 1 = water, 2 = lava, etc.
- Used by shader to select color and emission per pixel

#### Update Strategy
- Use existing chunk `dirty` flag
- On dirty: rewrite both textures (chunk_size² ≈ 4KB — cheap)
- On clean: no work

#### Mesh
- **One static quad per chunk** (4 vertices, 6 indices)
- UV covers `[0, 0]` → `[1, 1]`
- Position = chunk world coordinates
- Created once at chunk spawn, never rebuilt

### 2. Shader Architecture

#### Vertex Shader
Minimal pass-through: transform position, forward UV.

#### Fragment Shader
For each fragment:
1. Map UV → cell coordinates (accounting for padding offset)
2. Sample 3×3 neighborhood from density texture (9 samples)
3. For each neighbor with mass > 0, compute metaball contribution:
   ```
   radius_i = RADIUS_MIN + (RADIUS_MAX - RADIUS_MIN) * mass_i
   contribution_i = mass_i * radius_i² / distance²
   ```
4. Sum all contributions → field value
5. Hard threshold: `field > THRESHOLD` → fluid pixel, else transparent

#### Color Determination
- Sample fluid_id texture at the cell with the highest contribution
- Look up color from `fluid_colors[fluid_id]` uniform array
- Gives clean boundaries between different fluid types

#### Filtering
- Density texture: **Nearest** (preserves discrete values)
- Fluid ID texture: **Nearest** (must be exact)
- Lightmap: **Linear** (existing behavior)

#### Tunable Parameters (uniforms)
```
THRESHOLD = 0.5
RADIUS_MIN = 0.3
RADIUS_MAX = 0.7
FALLOFF_POWER = 2.0
```

### 3. Visual Effects

#### Emission Glow
- Per fluid_id emission value from `fluid_emission[MAX_FLUIDS]` uniform array
- Emissive fluids (lava): `color.rgb *= (1.0 + emission * 2.0)`, alpha = 1.0
- Non-emissive fluids (water): standard color with alpha from fluid definition

#### Lightmap Integration
- Same mechanism as current: `FluidMaterial` holds `lightmap: Handle<Image>` and `lightmap_uv_rect: Vec4`
- Fragment shader maps chunk UV → lightmap UV via `lightmap_uv_rect`
- `final_color.rgb *= lightmap_sample.rgb`
- No changes needed in `rc_lighting.rs`

#### Removed Effects
- Depth darkening (no per-cell depth concept)
- Caustics / voronoi
- Shimmer
- Wave height / wave params rendering
- Edge flags / SDF droplet rounding (metaballs replace this)

### 4. Rust-Side Changes

#### FluidMaterial Bindings
```
@group(2) @binding(0) density_texture     // R8Unorm, (chunk_size+2)²
@group(2) @binding(1) density_sampler     // Nearest
@group(2) @binding(2) fluid_id_texture    // R8Uint, (chunk_size+2)²
@group(2) @binding(3) fluid_id_sampler    // Nearest
@group(2) @binding(4) lightmap_texture
@group(2) @binding(5) lightmap_sampler    // Linear
@group(2) @binding(6) uniform {
    lightmap_uv_rect: vec4<f32>,
    time: f32,
    tile_size: f32,
    chunk_size: f32,
    threshold: f32,
    radius_min: f32,
    radius_max: f32,
    fluid_colors: array<vec4<f32>, 8>,
    fluid_emission: array<f32, 8>,
}
```

#### render.rs Changes
- Delete `build_fluid_mesh()` and all vertex attribute structs
- New `build_fluid_textures()`: creates density + fluid_id textures per dirty chunk
- Padding filled via `FluidWorld::get_cell()` for neighbor chunks

#### Mesh Simplification
- `build_chunk_quad()`: static quad covering chunk area, created once
- Remove all custom vertex attributes (COLOR, FLUID_DATA, WAVE_HEIGHT, WAVE_PARAMS, EDGE_FLAGS)
- Keep only POSITION and UV_0

#### Deletions
- Wave rendering integration (wave.rs stays for simulation, just not used in render)
- Edge flag computation
- Per-cell color/depth computation

## Performance Characteristics

- **Draw calls**: Same (1 per chunk with fluid, was already 1)
- **Vertices**: 4 per chunk (was N×4 per chunk)
- **Texture uploads**: ~4KB per dirty chunk per frame
- **Fragment cost**: 9 texture samples + field computation per pixel (moderate)
- **Expected**: Comfortable for ~2000 cells across visible chunks
