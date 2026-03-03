# Wave System Refactor Design

**Date:** 2026-03-03
**Branch:** `refactor/assets-restructuring`

## Problem

Three visual issues with the current wave system:

1. **Foam/waves cut off at chunk boundaries** — `reconcile_wave_boundaries` averages only `height`, not `velocity`, and splash impulse spread clips at chunk edges.
2. **Waves fly into space on splash** — `max_height=5.0` × `tile_size=8.0` = 40px displacement. Unclipped impulse from fast falls.
3. **Splashes lack visual punch** — particles exist but are drowned out by giant mesh displacement.

## Design

### 1. WaveBuffer double-buffer + wave equation cleanup

**Current:** `step()` clones the entire `height` Vec every tick. Damping applied to both velocity and height (double damping).

**Change:**
- Add `prev_height: Vec<f32>` field to `WaveBuffer`
- `step()` swaps `height`/`prev_height` via `std::mem::swap` — zero allocations
- Damping applied only to velocity: `vel *= damping`, `height = prev + vel`
- Remove double damping from height update

### 2. Horizontal boundary reconciliation fix

**Current:** Only `height` averaged at chunk boundaries. Splash impulse spread clipped at chunk edge.

**Change:**
- `reconcile_wave_boundaries`: average both `height` AND `velocity` at boundary cells
- `wave_consume_events`: when splash hits `local_x == 0`, propagate 50% impulse to right column of left neighbor chunk; when `local_x == chunk_size-1`, propagate to left column of right neighbor
- No vertical (top-bottom) reconciliation — waves are horizontal surface phenomena in a 2D platformer

### 3. Wave amplitude limiting

**Current:** `max_height=5.0` (40px displacement). No input impulse clamp. Linear damping.

**Change:**
- `WaveConfig.max_height`: 5.0 → **1.5** (12px max mesh displacement, ~1.5 tiles)
- New `WaveConfig.max_impulse: 2.0` — clamp impulse on input in `apply_impulse()`
- Nonlinear damping: when `|height| > max_height * 0.7`, apply extra damping factor (0.9 instead of 0.98) — large waves decay fast, small ambient waves persist
- Shader: visual effect comes from foam + particles, not giant mesh displacement

### 4. Enhanced splash particles

**Current:** 4-20 particles per splash, fixed size 4.0, lifetime 1.5s. 30% mass displacement.

**Change:**
- `particles_per_mass`: 15 → **25**
- Variable size: **2.0..6.0** random (mix of small and large drops)
- Two particle layers on splash:
  - **Drops** (existing) — fly up and sideways, gravity_scale=1.0
  - **Ripple ring** (new) — 4-8 particles, horizontal spread from impact point, small (1.5-2.5), fast fade (lifetime 0.4s), create expanding ring effect on water surface
- `splash_displacement`: 0.3 → **0.15** (less CA mass removal, fewer visual artifacts from empty cells)

## Files affected

| File | Changes |
|------|---------|
| `src/fluid/wave.rs` | Double-buffer, damping fix, boundary reconciliation fix |
| `src/fluid/systems.rs` | Cross-chunk impulse spread, impulse clamp |
| `src/fluid/splash.rs` | Particle tuning, ripple ring layer |
| `src/fluid/render.rs` | (none expected) |
| `assets/engine/shaders/fluid.wgsl` | (none expected) |

## Non-goals

- Vertical (top-bottom) wave boundary reconciliation
- Diagonal wave propagation (4-connected is standard for this equation)
- Wave interaction with solid tiles (reflection) — future work
