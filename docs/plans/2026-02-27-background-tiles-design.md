# Background Tile Layer — Design

**Date**: 2026-02-27
**Status**: Approved
**Inspiration**: Starbound wall/background system

## Overview

Add a second tile grid behind the foreground layer — Starbound-style background walls. Background tiles render behind foreground, interact with lighting, and create visual depth through dimming and shadow effects.

## Data Structures

### TileLayer struct

```rust
pub struct TileLayer {
    pub tiles: Vec<TileId>,
    pub bitmasks: Vec<u8>,
}
```

### ChunkData changes

```rust
pub struct ChunkData {
    pub fg: TileLayer,
    pub bg: TileLayer,
    pub light_levels: Vec<[u8; 3]>,  // shared across both layers
    pub damage: Vec<u8>,             // foreground only
}
```

### Layer enum

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Layer { Fg, Bg }
```

### WorldMap API

`get_tile`, `set_tile` gain a `Layer` parameter where needed. `is_solid` checks foreground only (only fg tiles block movement).

## World Generation

- **Above surface** (tile_y > surface_y): bg = AIR (parallax sky visible)
- **Surface and below** (tile_y <= surface_y): bg = biome's fill_block (dirt/stone)
- **Caves** (where cave_threshold produced AIR in fg): bg = fill_block (cave has walls but no foreground)

Rule: `bg = if tile_y <= surface_y { biome.fill_block } else { AIR }`

`generate_chunk_tiles` returns both fg and bg tile vectors.

## Lighting

Both layers contribute to light blocking. Effective opacity per tile position:

```
effective_opacity = max(fg_opacity, bg_opacity)
```

- Solid foreground → light blocked (unchanged)
- Air fg + solid bg → light blocked (cave stays dark)
- Air fg + air bg → light passes through (hole in wall, sky visible)

Applies to both `compute_chunk_sunlight` (column scan) and `bfs_from_emitter` (point lights).

## Rendering

### Two meshes per chunk

- **Background mesh**: z=-1.0, uses bg TileLayer data
- **Foreground mesh**: z=0.0, uses fg TileLayer data (unchanged behavior)

Both built by `build_chunk_mesh` with shared `light_levels`.

### Background dimming

Uniform `bg_dim` (0.6) in a second `SharedTileMaterial`. Shader: `color.rgb * light * bg_dim`.

### Foreground → background shadow

Per-vertex effect on the bg mesh. For each bg tile, check if foreground tiles exist at the same position or adjacent positions. If yes, apply additional dimming (×0.5). Implemented via corner averaging — shadow from 4 neighboring fg tile presences, baked into the light attribute of the bg mesh.

Result: bg tiles near fg tiles appear darker, creating depth like Starbound.

### Sprite variant differentiation

`select_variant` hash includes `Layer` parameter. Same tile type on fg vs bg gets different sprite variant, adding visual variety.

## Interaction (block_action)

### Controls

- **Left click**: break/place foreground (unchanged)
- **Right click**: break/place background
  - Solid bg tile → break it (bg = AIR)
  - Air bg tile → place bg tile (type from future inventory, hardcoded for now)

### Placement rule

Background tile can only be placed adjacent to an existing tile (fg or bg), like Starbound. No floating walls.

### On tile change (fg or bg)

1. Recompute bitmasks for affected layer (`update_bitmasks_around`)
2. Recompute lighting (`relight_around` — already accounts for both layers)
3. Mark chunks dirty → rebuild both meshes (fg change affects bg shadow, bg change affects lighting)

## Chunk Lifecycle

### ChunkEntities

```rust
pub struct ChunkEntities {
    pub fg: Entity,
    pub bg: Entity,
}
```

`LoadedChunks` becomes `HashMap<(i32, i32), ChunkEntities>`.

### Spawn order

1. Generate tiles (fg + bg)
2. Compute bitmasks for both layers
3. Compute lighting (once, accounts for both layers)
4. Build bg mesh (z=-1, with fg→bg shadow, bg_dim material)
5. Build fg mesh (z=0, standard material)

### Despawn

Remove both entities.

### Rebuild (ChunkDirty)

Rebuild both meshes — any tile change can affect shadow/lighting across layers.

### MeshBuildBuffers

Single resource, reused: build bg mesh → clone data → clear → build fg mesh.

## Parallax Sky

No changes needed. Parallax renders at farthest z. Background mesh (z=-1) covers sky where bg tiles exist. Where both fg and bg are AIR, no quads generated → sky visible through gaps.

## Summary Table

| Aspect | Decision |
|--------|----------|
| Storage | `TileLayer { tiles, bitmasks }`, ChunkData has `fg`/`bg`, shared `light_levels` |
| Generation | bg = fill_block below surface (including caves), AIR above |
| Lighting | `max(fg_opacity, bg_opacity)` for sunlight and point lights |
| Rendering | Two meshes per chunk (bg z=-1, fg z=0), bg_dim uniform ×0.6, fg→bg shadow per-vertex |
| Sprite variants | Layer included in select_variant hash |
| Controls | Left = fg, Right = bg (break/place) |
| Placement rule | bg only adjacent to existing tile |
| Chunk lifecycle | `ChunkEntities { fg, bg }`, spawn/despawn/rebuild both |
| Parallax | No changes — works automatically |
