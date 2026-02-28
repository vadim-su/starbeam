# Background Density for RC Lighting

## Problem

Currently, the Radiance Cascades (RC) lighting system only considers the foreground (FG) layer for light occlusion. When a player creates a hole through both FG and BG layers (both are AIR), light from outside should penetrate into caves and illuminate the interior. Currently, light is blocked because BG tiles are ignored.

**Use case:** Player digs through both layers. Sunlight enters through the hole and illuminates the cave interior (both FG and BG surfaces inside).

**Requirement:** Light passes through only when BOTH FG AND BG are empty (AIR). If either layer is solid, light is blocked.

## Solution

Add a separate density texture for the background layer. The shader combines both densities during raymarching using `max(fg_density, bg_density)`.

```
Current pipeline:
  density_map (FG only) → radiance_cascades.wgsl → cascade → lightmap

New pipeline:
  density_map (FG) ─┐
                    ├→ radiance_cascades.wgsl → cascade → lightmap
  density_bg (BG) ──┘
```

## Implementation

### 1. CPU-side: `src/world/rc_lighting.rs`

#### 1.1 Add density_bg to RcInputData

```rust
#[derive(Resource, Clone, Default, ExtractResource)]
pub struct RcInputData {
    pub density: Vec<u8>,      // FG (exists)
    pub density_bg: Vec<u8>,   // BG (NEW)
    pub emissive: Vec<[f32; 4]>,
    pub albedo: Vec<[u8; 4]>,
    // ...
}
```

#### 1.2 Add get_bg_tile function

```rust
fn get_bg_tile(
    world_map: &WorldMap,
    tile_x: i32,
    tile_y: i32,
    world_config: &WorldConfig,
    tile_registry: &TileRegistry,
) -> Option<TileId> {
    if tile_y < 0 {
        return Some(tile_registry.by_name("stone"));
    }
    if tile_y >= world_config.height_tiles {
        return None;
    }
    let wrapped_x = world_config.wrap_tile_x(tile_x);
    let (cx, cy) = tile_to_chunk(wrapped_x, tile_y, world_config.chunk_size);
    let (lx, ly) = tile_to_local(wrapped_x, tile_y, world_config.chunk_size);
    world_map
        .chunk(cx, cy)
        .map(|chunk| chunk.bg.get(lx, ly, world_config.chunk_size))
}
```

#### 1.3 Modify extract_lighting_data

- Resize `density_bg` buffer when dimensions change
- Fill `density_bg` alongside `density` in the tile loop

### 2. GPU Pipeline: `src/world/rc_pipeline.rs`

#### 2.1 Add density_bg to RcGpuImages

```rust
#[derive(Resource, Clone, ExtractResource)]
pub struct RcGpuImages {
    pub density: Handle<Image>,
    pub density_bg: Handle<Image>,   // NEW
    pub emissive: Handle<Image>,
    // ...
}
```

#### 2.2 Update create_gpu_images

```rust
density_bg: make_gpu_texture(images, s, s, TextureFormat::R8Unorm),
```

#### 2.3 Update resize_gpu_textures

Recreate density_bg when dimensions change.

#### 2.4 Update prepare_rc_textures

Upload density_bg data to GPU (same pattern as density).

#### 2.5 Update cascade bind group layout

Add new binding after cascade_write:

```rust
// @binding(7) density_bg
texture_2d(TextureSampleType::Float { filterable: false }),
```

#### 2.6 Update prepare_rc_bind_groups

Include density_bg texture view in cascade bind groups.

### 3. Shader: `assets/shaders/radiance_cascades.wgsl`

#### 3.1 Add binding

```wgsl
@group(0) @binding(7) var density_bg: texture_2d<f32>;
```

#### 3.2 Combine densities in raymarch loop

```wgsl
let fg_density = textureLoad(density_map, sample_px, 0).r;
let bg_density = textureLoad(density_bg, sample_px, 0).r;
let density = max(fg_density, bg_density);

if density > 0.5 {
    // hit solid surface (either FG or BG)
    // ...
}
```

## Files Changed

| File | Changes |
|------|---------|
| `src/world/rc_lighting.rs` | Add density_bg buffer, get_bg_tile function, fill both densities |
| `src/world/rc_pipeline.rs` | Add density_bg texture handle, upload, bind group entry |
| `assets/shaders/radiance_cascades.wgsl` | Add binding, combine densities |

## Memory Impact

- Additional texture: ~100-200KB (R8Unorm, viewport + padding size)
- CPU buffer: same size as existing density buffer

## Future Extensions

This design enables:
- Per-tile light penetration values (glass, water could have partial density)
- Different light behavior for BG materials
