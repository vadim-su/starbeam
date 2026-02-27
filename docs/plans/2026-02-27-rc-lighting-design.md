# Radiance Cascades 2D Lighting — Design

**Date:** 2026-02-27
**Status:** Approved
**Replaces:** 2026-02-27-lighting-design.md (BFS flood-fill)

## Summary

Replace the CPU-based BFS flood-fill lighting system with GPU-based Radiance Cascades (RC) for screenspace 2D global illumination with bounce light. Full removal of old lighting code.

## Decisions

| Question | Decision |
|----------|----------|
| Computation area | Screenspace + padding (~32 tiles per side) |
| Sunlight | Emitter from top edge of viewport |
| Lightmap application | Sample in tile.wgsl via screen UV |
| Resolution | 1:1 with screen pixels |
| Blockers/emitters | Foreground layer only |
| Migration strategy | Clean replacement, delete old code |
| Algorithm | Classic RC with density map raymarching (branching factor 4) |
| Bounce light | Temporal feedback + albedo map |
| Worldspace | Not now, architecture allows future addition |

## Architecture — Data Flow

```
Each frame:

┌─────────────────────────────────────────────────────────┐
│ CPU (Rust, Bevy systems)                                │
│                                                         │
│  1. Determine visible area (camera transform)           │
│  2. Write visible tiles into staging buffers:            │
│     - density_map: f32 per pixel (0.0=air, 1.0=solid)   │
│     - emissive_map: vec4<f32> (RGB + intensity)          │
│     - albedo_map: vec4<u8> (surface color for bounce)    │
│     - sun_edge: top row = SUN_COLOR if sky visible       │
│  3. Upload textures to GPU                               │
└──────────────────────┬──────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────┐
│ GPU Compute (WGSL)                                      │
│                                                         │
│  Pass 1: RC Raymarch + Merge (per cascade, top-down)    │
│    - Cascade N (highest): long rays, few probes          │
│    - ...                                                 │
│    - Cascade 0 (lowest): short rays, many probes         │
│    - Each cascade: raymarch density_map,                  │
│      collect radiance from emissive_map,                 │
│      on hit: add reflected = lightmap_prev * albedo,     │
│      merge with cascade above                            │
│    → Result: cascade_0 texture                           │
│                                                         │
│  Pass 2: Finalize                                       │
│    - From cascade_0 extract irradiance (sum over         │
│      directions) → lightmap texture                      │
│    - Swap lightmap ↔ lightmap_prev                       │
└──────────────────────┬──────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────┐
│ GPU Fragment (tile.wgsl)                                │
│                                                         │
│  - ATTRIBUTE_LIGHT removed                              │
│  - Added lightmap sampler binding                        │
│  - fragment: color.rgb * lightmap_sample.rgb * dim       │
└─────────────────────────────────────────────────────────┘
```

## Cascade Parameters

Branching factor 4 (standard for 2D RC):

| Cascade | Probes per row | Directions | Ray length (px) | Interval start |
|---------|---------------|------------|-----------------|----------------|
| 0 | W×H (every pixel) | 4 | 1-4 px | 0 |
| 1 | W/2 × H/2 | 16 | 4-16 px | 4 |
| 2 | W/4 × H/4 | 64 | 16-64 px | 16 |
| 3 | W/8 × H/8 | 256 | 64-256 px | 64 |
| 4 | W/16 × H/16 | 1024 | 256-1024 px | 256 |
| 5 | W/32 × H/32 | 4096 | 1024-4096 px | 1024 |

Number of cascades: `ceil(log4(max(W, H)))` — for 1920×1080 this is ~5-6.

## Raymarching (per ray)

```
step_size = 1.0 pixel
for step in 0..max_steps:
    sample_pos = ray_origin + ray_dir * (offset + step * step_size)
    density = textureSample(density_map, sample_pos)
    if density > 0.5:
        // Hit solid surface
        emissive = textureSample(emissive_map, sample_pos).rgb
        albedo = textureSample(albedo_map, sample_pos).rgb
        prev_light = textureSample(lightmap_prev, sample_pos).rgb
        reflected = prev_light * albedo * BOUNCE_DAMPING  // 0.3-0.5
        radiance = emissive + reflected
        hit = true
        break
    if step * step_size >= interval_end:
        break  // end of interval — merge with upper cascade
```

## Bounce Light

Temporal feedback approach:
- On ray hit: `reflected = lightmap_prev[hit_pos] * albedo * BOUNCE_DAMPING`
- `lightmap_prev` = lightmap from previous frame (double-buffered swap)
- Converges in 2-3 frames
- `BOUNCE_DAMPING = 0.3-0.5` prevents infinite amplification

Requires `albedo: [u8; 3]` field in `TileDef` (tile registry).

## Sunlight

CPU-side, before filling emissive_map:
- For each column x in viewport: if top visible tile is AIR and camera can see sky → emit `SUN_COLOR [255, 250, 230]` at top row of emissive_map
- Check: `camera_y + viewport_height/2 >= surface_height[x]`
- Cost: O(viewport_width)

## Edge Padding

RC upper cascades cast rays beyond viewport. Padding = interval length of highest cascade / tile_size.
- ~32 tiles per side (+1 chunk each direction)
- density/emissive/albedo maps sized at (viewport + 2×padding)
- Lightmap cropped to viewport when sampling in tile.wgsl

## Window Resize

On `WindowResized` event: recreate all textures (density, emissive, albedo, cascade_storage, lightmap, lightmap_prev). Rare event, not performance-critical.

## GPU Textures

| Texture | Format | Size | Update |
|---------|--------|------|--------|
| `density_map` | R8Unorm | (W+pad)×(H+pad) | CPU → GPU every frame |
| `emissive_map` | Rgba16Float | (W+pad)×(H+pad) | CPU → GPU every frame |
| `albedo_map` | Rgba8Unorm | (W+pad)×(H+pad) | CPU → GPU every frame |
| `cascade_storage` | Rgba16Float | ~1.33× viewport | GPU compute every frame |
| `lightmap` | Rgba16Float | W×H | GPU compute → fragment |
| `lightmap_prev` | Rgba16Float | W×H | Swap with lightmap every frame |

## Cascade Memory Layout

All cascades in one texture, stacked vertically:

```
┌──────────────────┐  ← cascade 0: W × H
│   cascade 0      │     4 dirs = 2×2 subtexels per probe
├──────────────────┤  ← cascade 1: W/2 × H/2
│   cascade 1      │     16 dirs = 4×4 subtexels per probe
├──────────────────┤
│   cascade 2      │
├──────────────────┤
│   ...            │
└──────────────────┘
```

Total size: ~1.33× viewport (geometric sum 1 + 1/4 + 1/16 + ...).

## Files — Create

| File | Est. lines | Role |
|------|-----------|------|
| `src/world/rc_lighting.rs` | ~300 | Plugin, CPU extract system, resources |
| `src/world/rc_pipeline.rs` | ~400 | Render pipeline, bind groups, render graph node |
| `assets/shaders/radiance_cascades.wgsl` | ~150 | Compute: raymarch + merge cascades |
| `assets/shaders/rc_finalize.wgsl` | ~50 | Compute: cascade_0 → lightmap |

## Files — Delete/Modify

| File | Action |
|------|--------|
| `src/world/lighting.rs` | **Delete** entirely |
| `src/world/chunk.rs` | Remove `light_levels`, lighting calls |
| `src/world/mesh_builder.rs` | Remove `ATTRIBUTE_LIGHT`, corner_light, corner_shadow, lights buffer |
| `src/world/tile_renderer.rs` | Remove light from vertex layout, add lightmap binding |
| `src/world/mod.rs` | Replace `pub mod lighting` → `pub mod rc_lighting; pub mod rc_pipeline` |
| `src/interaction/block_action.rs` | Remove `relight_around` calls |
| `assets/shaders/tile.wgsl` | Remove light attribute, add lightmap sample |
| `src/registry/tile.rs` | Add `albedo: [u8; 3]` to `TileDef` |
| `assets/world/tiles.registry.ron` | Add `albedo` to each tile |

## Bevy Integration

### Plugin

```rust
pub struct RcLightingPlugin;
// Registers:
//   Update: extract_lighting_data (in GameSet::WorldUpdate)
//   RenderApp: RcPipeline resource, setup, prepare, render graph node
```

### TileMaterial (updated)

```rust
#[derive(AsBindGroup, Asset, TypePath, Clone)]
pub struct TileMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub atlas: Handle<Image>,
    #[uniform(2)]
    pub dim: f32,
    #[texture(3)]
    #[sampler(4)]
    pub lightmap: Handle<Image>,
}
```

### tile.wgsl (updated)

```wgsl
@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(atlas_texture, atlas_sampler, in.uv);
    if color.a < 0.01 {
        if uniforms.dim < 1.0 {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
        discard;
    }
    let screen_uv = in.position.xy / viewport_size;
    let light = textureSample(lightmap_texture, lightmap_sampler, screen_uv).rgb;
    return vec4<f32>(color.rgb * light * uniforms.dim, color.a);
}
```

## Performance Budget

| Stage | Time |
|-------|------|
| CPU extract | ~0.3ms |
| GPU upload | ~0.1ms |
| GPU RC (5-6 cascades) | ~1.5-2ms |
| GPU finalize | ~0.2ms |
| GPU bounce overhead | ~0.3ms |
| **Total** | **~2.5-3ms** |

## Out of Scope (v1)

- Colored glass / semi-transparent tiles (density map is binary)
- Worldspace lighting for offscreen (architecture allows future addition)
- SDF optimization (add later if needed)

## References

- [Radiance Cascades paper](https://radiance-cascades.com/)
- [GM Shaders: RC Part 1](https://mini.gmshaders.com/p/radiance-cascades)
- [GM Shaders: RC Part 2](https://mini.gmshaders.com/p/radiance-cascades2)
- [Holographic RC (2025)](https://arxiv.org/abs/2505.02041)
- [bevy_flatland_radiance_cascades](https://github.com/kornelski/bevy_flatland_radiance_cascades) — reference Bevy+WGSL
- [bevy-magic-light-2d](https://github.com/zaycev/bevy-magic-light-2d) — SDF raymarching reference
