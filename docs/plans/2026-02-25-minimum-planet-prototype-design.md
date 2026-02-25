# Minimum Planet Prototype — Design

**Date:** 2026-02-25
**Status:** Approved
**Scope:** First playable prototype — single planet, walk/jump/break/place blocks

## Overview

2D sandbox prototype inspired by Starbound. Single procedurally generated planet, player character with basic platforming and block interaction. Colored placeholders, no art assets. Built on Bevy 0.18 + `bevy_ecs_tilemap`.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Scope | Minimum planet (no space, no ship) | Focus on core loop first |
| World size | 2048x1024 tiles | Feels like a world, needs chunk loading |
| Terrain | Simple Perlin/Simplex, no biomes | Mechanics over variety |
| Player | Walk/jump + break/place blocks, no inventory | Core interaction without complexity |
| Fluid physics | Deferred | Not needed for core loop |
| Graphics | Colored placeholders (32x32) | Speed of development |
| Tilemap layers | Foreground only | Minimum viable |
| Architecture | Entity-per-tile via `bevy_ecs_tilemap` | Idiomatic Bevy ECS, crate handles heavy lifting |
| Wrap-around | Deferred | Doesn't affect chunk architecture |

## World & Chunks

- **World:** 2048x1024 tiles. Tile = 32x32 pixels.
- **Chunks:** 32x32 tiles each -> 64x32 chunk grid.
- **Visibility zone:** rectangle of chunks covering screen + 1 chunk buffer on each side.
- **Lifecycle:** chunks entering visibility are spawned, exiting are despawned. Modified chunk data persisted in `Resource` (`HashMap<ChunkPos, ChunkData>`).
- **Tile types:**
  - `Air` — empty
  - `Dirt` — brown
  - `Stone` — gray
  - `Grass` — green, surface layer

## Terrain Generation

1. **Surface line** — 1D Simplex noise along X axis. Amplitude ~30-50 tiles, base height ~70% from top.
2. **Layer fill (per column, top to bottom):**
   - Above surface -> `Air`
   - Surface (1 tile) -> `Grass`
   - 3-5 tiles below surface -> `Dirt`
   - Deeper -> `Stone`
3. **Caves** — 2D Simplex noise with threshold (~0.55). Applied only below surface. Above threshold -> `Air`.
4. **Seed:** numeric seed determines world. Hardcoded or random at launch for prototype.
5. **Lazy generation:** chunks generated on first visibility if not in saved data.

## Player & Physics

- **Sprite:** 1x2 tiles (32x64 px), colored rectangle (blue).
- **Spawn:** surface at world center (x=1024, y = surface height + 2).
- **Movement:** `A`/`D` or arrows for horizontal (constant speed, no inertia). `Space` for jump (impulse, only when grounded).
- **Gravity:** constant downward acceleration, tunable constant.
- **Collisions:** AABB between player rect and non-Air tiles. Check nearby tiles only. Resolve per-axis (X then Y).
- **No external physics crate** — custom minimal implementation, better fit for tile-based platforming.

## Block Interaction

- **Break (LMB):** cursor on tile -> tile becomes `Air`. Max range ~5 tiles from player center. Instant destruction.
- **Place (RMB):** cursor on `Air` tile -> tile becomes `Dirt` (hardcoded type). Same range limit. Cannot place where player AABB overlaps.
- **Coordinate translation:** screen mouse -> world coords (via camera) -> tile coords (divide by 32, floor).
- **Chunk update:** modify `bevy_ecs_tilemap` entity + persist change in `ChunkData`.

## Camera

- **Follow:** hard-locked to player center (no lerp/smoothing).
- **Zoom:** fixed, ~40-50 tiles visible horizontally. Tunable constant.
- **Bounds:** clamped to world edges (0..2048*32 horizontal, 0..1024*32 vertical).

## Module Structure

```
src/
  main.rs              -- entry point, assembles App from plugins
  world/
    mod.rs             -- WorldPlugin
    chunk.rs           -- ChunkPos, ChunkData, chunk load/unload systems
    terrain_gen.rs     -- landscape generation (noise, fill)
    tile.rs            -- TileType enum, placeholder colors
  player/
    mod.rs             -- PlayerPlugin
    movement.rs        -- input, velocity, gravity
    collision.rs       -- AABB collisions with tiles
  interaction/
    mod.rs             -- InteractionPlugin
    block_action.rs    -- break/place blocks, mouse coord translation
  camera/
    mod.rs             -- CameraPlugin
    follow.rs          -- follow player, clamp to bounds
```

## Bevy Plugins

- `WorldPlugin` — world resources, chunk load/unload systems, generation.
- `PlayerPlugin` — player spawn, movement and collision systems.
- `InteractionPlugin` — LMB/RMB block action systems.
- `CameraPlugin` — camera spawn, follow system.

## System Ordering

1. Input (built-in Bevy)
2. Player movement + gravity
3. Collision resolution (position correction)
4. Chunk load/unload (based on new camera position)
5. Block interaction (on click)
6. Camera follow (track player)

## Out of Scope (deferred)

- Space / ship / stations
- Biomes
- Fluid / gas physics
- Background & parallax layers
- Wrap-around world
- Inventory
- Pixel art assets & animations
- Multiplayer
