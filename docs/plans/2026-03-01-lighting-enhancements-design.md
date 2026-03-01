# Lighting Enhancements: Torch Flickering + Light Opacity

**Date:** 2026-03-01
**Branch:** feature/day-night-cycle
**Status:** Approved

## Overview

Two enhancements to the Radiance Cascades lighting pipeline:

1. **Torch flickering** — per-tile independent flickering with configurable speed/amplitude
2. **Light opacity in RC** — partial light transmission through tiles (dirt transparent, stone opaque)

## Feature 1: Torch Flickering

### Goal

Torches (and future emissive tiles) pulsate in brightness independently, giving a
"living fire" feel. Since RC propagates emitter brightness into radius naturally,
modulating intensity also modulates effective light radius.

### Design

**New TileDef fields** (all `#[serde(default)]`):

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `flicker_speed` | f32 | 0.0 | Base oscillation frequency (Hz). 0 = no flicker. |
| `flicker_strength` | f32 | 0.0 | Amplitude of variation (0.0–1.0). |
| `flicker_min` | f32 | 1.0 | Floor multiplier so torch never goes fully dark. |

**Torch RON example:**
```ron
light_emission: (240, 180, 80), flicker_speed: 3.0, flicker_strength: 0.3, flicker_min: 0.7
```
Result: brightness oscillates between 70% and 100%.

**Per-tile phase:** Hash of `(tx, ty)` gives each torch a unique phase offset
so adjacent torches don't pulse in sync.

**Noise function:** Sum of three sine harmonics for organic feel:
```
base = sin(t * speed + phase)     * 0.5
     + sin(t * speed * 2.3 + phase) * 0.3
     + sin(t * speed * 4.1 + phase) * 0.2
noise = base * 0.5 + 0.5   // normalize to [0, 1]
multiplier = flicker_min + noise * flicker_strength
```

**Where:** In `extract_lighting_data` (CPU-side), when writing emissive for a
tile with `light_emission != [0,0,0]` and `flicker_speed > 0`. Requires adding
`Res<Time>` to the system parameters.

### Changes

- `src/registry/tile.rs` — add 3 fields to `TileDef`
- `assets/world/tiles.registry.ron` — add flicker params to torch
- `src/world/rc_lighting.rs` — add `Res<Time>`, compute flicker multiplier in emissive loop
- `src/test_helpers.rs`, `src/world/mesh_builder.rs` — add default flicker fields to test TileDefs

## Feature 2: Light Opacity in RC Pipeline

### Goal

Replace binary density (0/255) with per-tile opacity so light partially
penetrates through different materials. Dirt lets light seep through;
stone blocks almost completely.

### Design

**CPU side** (`extract_lighting_data`): Write normalized `light_opacity` to density map:
```rust
// Before: input.density[idx] = 255;
// After:
let opacity = tile_registry.light_opacity(tile_id);
input.density[idx] = (opacity as f32 / 15.0 * 255.0) as u8;
```

**GPU side** (`radiance_cascades.wgsl`): Volumetric raymarching with transmittance:
```wgsl
var transmittance = 1.0;
// In raymarch loop:
let opacity = textureLoad(density_map, sample_px, 0).r;
if opacity > 0.01 {
    let emissive = textureLoad(emissive_map, sample_px, 0).rgb;
    // ... bounce light ...
    let surface_light = emissive + reflected;
    radiance += surface_light * opacity * transmittance;
    transmittance *= (1.0 - opacity);
    if transmittance < 0.01 { hit = true; break; }
}
```

**Transmission per tile:**

| Tile | light_opacity | Transmission | Through 3 tiles |
|------|:---:|:---:|:---:|
| air | 0 | 100% | 100% |
| grass | 4 | 73% | 39% |
| dirt | 5 | 67% | 30% |
| stone | 8 | 47% | 10% |
| default (15) | 15 | 0% | 0% |

### Changes

- `src/world/rc_lighting.rs` — write normalized opacity instead of binary 255
- `assets/shaders/radiance_cascades.wgsl` — volumetric raymarch with transmittance accumulation

## Out of Scope

- Dynamic light from held items (player torch in hand)
- New tile types (crystals, lava, mushrooms)
- Stars/moon visuals
- Debug sliders for flicker params (existing debug panel suffices)
