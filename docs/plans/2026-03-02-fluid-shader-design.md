# Fluid & Gas Shader Design

## Goal

Replace the current flat `ColorMaterial` fluid rendering with a custom `FluidMaterial` + WGSL shader that provides Starbound-style visual effects: animated wavy surface, depth darkening, internal shimmer, lightmap integration, and glow for emissive fluids (lava).

## Architecture

### FluidMaterial (Rust)

A custom `Material2d` following the same pattern as `TileMaterial` and `LitSpriteMaterial`.

```rust
#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct FluidMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub lightmap: Handle<Image>,
    #[uniform(2)]
    pub lightmap_uv_rect: Vec4,  // (scale_x, scale_y, offset_x, offset_y)
    #[uniform(3)]
    pub time: f32,               // elapsed seconds, updated each frame
}
```

Bindings:
- 0/1: lightmap texture + sampler (from RC pipeline)
- 2: lightmap UV transform (same format as tile/sprite)
- 3: animation time

### Vertex Attributes

The mesh builder (`build_fluid_mesh`) provides these vertex attributes:

| Attribute | Shader Location | Content |
|-----------|----------------|---------|
| `POSITION` | 0 | World-space quad corners (x, y, z=0.5) |
| `COLOR` | 1 | Fluid RGBA from `FluidDef.color`, alpha scaled by fill |
| `UV_0` | 2 | `(fill_level, flags)` — fill 0..1, flags packed as float |
| `CUSTOM_0` | 3 | `(emission_r, emission_g, emission_b, depth_in_fluid)` |

**Flags encoding** (UV.y): bit-packed as float, decoded in shader:
- bit 0 (value += 1.0): is_surface (cell has air/empty above for liquid, below for gas)
- bit 1 (value += 2.0): is_gas

**depth_in_fluid**: normalized 0..1 value. 0 = at surface, 1 = deepest. Computed in mesh builder by scanning upward (for liquid) or downward (for gas) from each cell to find the surface.

### WGSL Shader (`assets/engine/shaders/fluid.wgsl`)

**Vertex shader:**
- Standard `mesh2d_position_local_to_clip` transform
- Pass world_pos, color, uv, custom_0 to fragment
- If is_surface: offset top vertices by `sin(world_x * 3.0 + time * 2.0) * 0.15 * tile_size`. Wave amplitude ~15% of a pixel, subtle.

**Fragment shader — 5 effects applied in order:**

1. **Base color**: from interpolated vertex color
2. **Internal shimmer**: `brightness *= 1.0 + 0.06 * sin(world_pos.x * 5.0 + world_pos.y * 3.0 + time * 1.5)` — subtle +-6% brightness modulation
3. **Depth darkening**: `brightness *= 1.0 - depth_in_fluid * 0.35` — up to 35% darker at maximum depth
4. **Lightmap**: `color.rgb *= textureSample(lightmap, sampler, lightmap_uv).rgb`
5. **Glow/emission**: `color.rgb = max(color.rgb, emission.rgb)` — emission overrides darkness, unaffected by lightmap

### Surface Detection (mesh builder)

For each non-empty fluid cell at `(x, y)`:
- **Liquid**: is_surface = true if cell at `(x, y+1)` is empty or out of bounds
- **Gas**: is_surface = true if cell at `(x, y-1)` is empty or out of bounds

For depth_in_fluid:
- **Liquid**: count consecutive non-empty same-fluid cells upward from current cell to surface. `depth = distance_to_surface / max_depth`. Simple integer scan, capped at some max (e.g., 16 cells).
- **Gas**: same but downward.

### Integration with Lightmap Pipeline

Add `FluidMaterial` to `update_tile_lightmap()` in `rc_lighting.rs`:
- Update `lightmap` handle and `lightmap_uv_rect` for the shared fluid material, same as tiles/sprites.

### System Changes

**`init_fluid_material`** — change from `ColorMaterial` to `FluidMaterial`:
- Create with fallback lightmap (1x1 white) and default UV rect
- Register `Material2dPlugin::<FluidMaterial>::default()` in plugin

**`fluid_rebuild_meshes`** — use `MeshMaterial2d<FluidMaterial>` instead of `MeshMaterial2d<ColorMaterial>`

**New system: `update_fluid_time`** — runs each frame, updates `material.time` from `Time::elapsed_secs()`. Can be combined with the lightmap update.

### Files Changed/Created

- **New**: `assets/engine/shaders/fluid.wgsl` — the shader
- **Modified**: `src/fluid/render.rs` — add UV_0, CUSTOM_0 attributes, surface/depth detection
- **Modified**: `src/fluid/systems.rs` — FluidMaterial, SharedFluidMaterial, time update
- **Modified**: `src/fluid/mod.rs` — register Material2dPlugin, init FluidMaterial
- **Modified**: `src/world/rc_lighting.rs` — add fluid material to lightmap update

### Performance

- No extra draw calls (same mesh count)
- 2 extra vertex attributes (UV_0 + CUSTOM_0) — negligible memory
- Fragment shader: 2 sin(), 1 texture sample (lightmap), 1 max — very cheap
- Surface/depth detection in mesh builder: O(cells * max_scan_depth) ≈ O(n * 16)

### Testing

- Unit tests in `render.rs` for new vertex attributes (UV values, surface detection, depth values)
- Visual testing: place water, lava, steam in-game and observe effects
- Verify lightmap darkens fluids in unlit areas
- Verify lava glows in darkness
