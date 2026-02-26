# Autotile Integration: Custom Tile Renderer

**Date:** 2026-02-26
**Status:** Approved

## Summary

Custom mesh-per-chunk tile renderer replacing bevy_ecs_tilemap. Each chunk (32x32) = one Entity with Mesh2d + Material2d. Combined horizontal atlas built at load time from per-type PNGs. Blob47 autotiling with 8-bit bitmask, deterministic variant selection, eager point updates.

## Decisions

| Decision | Choice | Alternatives considered |
|----------|--------|----------------------|
| Renderer | Custom mesh-per-chunk + Material2d | bevy_ecs_tilemap (removed), data texture hybrid, full custom render node |
| Atlas layout | Horizontal combined (types as columns) | Vertical (hits GPU limits), separate textures (more draw calls), texture arrays |
| Bitmask strategy | Any solid = "filled" | Per-type matching |
| Neighbor updates | Eager point (9 tiles on change) | Lazy/deferred |
| Variant selection | Deterministic hash(x,y,seed) + weighted | Random, sequential |
| Tile config | `autotile: Option<String>` in TileDef | texture_index field |
| Entity model | Entity-per-chunk (~20 entities) | Entity-per-tile (~20k), no entities |
| Tile properties | Static in TileDef, dynamic in ChunkData arrays | Entity-per-tile components |

## 1. Architecture Overview

```
LOADING:
  Per-type PNGs (dirt.png, stone.png...) → AtlasBuilder → combined atlas (Nx16 wide, 752 tall)
  tiles.registry.ron → TileRegistry (TileDef with properties)
  *.autotile.ron → AutotileRegistry (bitmask → sprite + variants)

RUNTIME:
  WorldMap.chunks → ChunkData (tiles[], damage[], bitmasks[])
  ChunkMeshBuilder: bitmask → autotile lookup → UV → 4 vertices per tile
  Entity per chunk: [ChunkCoord, Mesh2d, MeshMaterial2d, Transform]
  One shared TileMaterial with combined atlas texture

UPDATE:
  Block change → set_tile → recompute 9 bitmasks → mark 1-2 chunks dirty → rebuild mesh
```

## 2. Atlas Pipeline

### On disk (modder-facing)

```
assets/world/terrain/
  sources/              <- 5x4 templates for autotile47.py
    dirt.png
    stone.png
  dirt.png              <- generated spritesheet (16x752, 47 rows)
  stone.png
  dirt.autotile.ron     <- bitmask → sprite mapping + variants
  stone.autotile.ron
```

### Generation (Python, offline)

```bash
python scripts/autotile47.py sources/crystal.png -o crystal.png --ron crystal.autotile.ron
```

Existing pipeline, no changes needed.

### Loading (Rust, at startup)

1. Load `tiles.registry.ron` → know all tile types and autotile names
2. For each type with `autotile`: load `{name}.png` + `{name}.autotile.ron`
3. AtlasBuilder: stitch horizontally
   - Width = N_types x 16px (200 types = 3200px)
   - Height = 752px (fixed, 47 rows x 16px)
   - Result → `Handle<Image>` (GPU texture, nearest filtering)
4. Build AutotileRegistry: column_index per type + bitmask_map[256]

### Autotile RON format

```ron
(
  tile_size: 16,
  mappings: [
    (bitmask: 0,   variants: [(row: 0, weight: 1.0)]),
    (bitmask: 255, variants: [(row: 46, weight: 0.7), (row: 45, weight: 0.3)]),
  ]
)
```

## 3. Data Model

### TileDef (tiles.registry.ron)

```ron
(tiles: [
    (id: 0, name: "air"),
    (id: 1, name: "dirt", autotile: "dirt",
     hardness: 1.0, damage_on_contact: 0.0, viscosity: 0.0, effects: []),
    (id: 2, name: "stone", autotile: "stone",
     hardness: 3.0, damage_on_contact: 0.0, viscosity: 0.0, effects: []),
])
```

### Rust structures

```rust
struct TileDef {
    id: u16,
    name: String,
    autotile: Option<String>,
    hardness: f32,
    damage_on_contact: f32,
    viscosity: f32,
    effects: Vec<String>,
}

struct ChunkData {
    tiles: [TileId; 1024],
    damage: [u8; 1024],
    bitmasks: [u8; 1024],       // cached, recomputed on change
}

struct AutotileEntry {
    column_index: u32,
    bitmask_map: [Vec<SpriteVariant>; 256],  // O(1) lookup, most empty
}

struct SpriteVariant { row: u8, weight: f32 }
```

### Bitmask computation

```
Neighbors (8 bits):
  NW N NE      128  1  2
  W  .  E  →    64  .  4
  SW S SE       32 16  8

bit = 1 if neighbor is solid (TileId != AIR)
```

### Variant selection

```rust
fn select_variant(variants: &[SpriteVariant], x: i32, y: i32, seed: u64) -> u8 {
    let h = xxhash(x, y, seed);
    let t = (h as f32) / (u64::MAX as f32);  // 0.0-1.0
    let mut acc = 0.0;
    for v in variants {
        acc += v.weight;
        if t <= acc { return v.row; }
    }
    variants.last().unwrap().row
}
```

## 4. Mesh Building

### Mesh structure

```rust
struct TileVertex {
    position: [f32; 2],  // world position
    uv: [f32; 2],        // atlas UV
}
// 16 bytes per vertex, 4 vertices per tile
// Max per chunk: 4096 vertices (65KB) + 6144 indices (24KB) = ~89KB
```

### Algorithm

For each tile in chunk:
1. Skip if AIR or no autotile
2. Read cached bitmask → lookup variants → select by hash
3. Compute world position (chunk_coord * CHUNK_SIZE + local) * TILE_SIZE
4. Compute atlas UV with half-pixel inset (anti-bleeding)
5. Emit 4 vertices + 6 indices

Air tiles produce no geometry — surface chunks with 70% air emit ~300 quads instead of 1024.

### UV with half-pixel inset

```rust
fn atlas_uv(col: u32, row: u32, params: &AtlasParams) -> (f32, f32, f32, f32) {
    let ts = params.tile_size as f32;
    let half = 0.5;
    let u_min = (col as f32 * ts + half) / params.atlas_width as f32;
    let u_max = (col as f32 * ts + ts - half) / params.atlas_width as f32;
    let v_min = (row as f32 * ts + half) / params.atlas_height as f32;
    let v_max = (row as f32 * ts + ts - half) / params.atlas_height as f32;
    (u_min, u_max, v_min, v_max)
}
```

### Optimization

Pre-allocated `MeshBuildBuffers` resource — clear() resets length to 0 but keeps capacity. Zero allocations after first rebuild.

### Dirty system

`ChunkDirty` marker component. `rebuild_dirty_chunks` system queries `With<ChunkDirty>`, builds new mesh, replaces handle, removes marker.

## 5. Rendering

### Material

```rust
#[derive(Asset, AsBindGroup, Clone, TypePath)]
struct TileMaterial {
    #[texture(0)]
    #[sampler(1)]
    atlas: Handle<Image>,
}

impl Material2d for TileMaterial { ... }
```

One material instance shared by all chunk entities.

### Shader (tile.wgsl)

Trivial — all logic is in mesh UVs:

```wgsl
@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(atlas_texture, atlas_sampler, in.uv);
    if color.a < 0.01 { discard; }
    return color;
}
```

### Atlas texture config

- `ImageFilterMode::Nearest` (pixel art)
- `ImageAddressMode::ClampToEdge`

### Chunk entity

```rust
commands.spawn((
    ChunkCoord(coord),
    Mesh2d(mesh_handle),
    MeshMaterial2d(material.clone()),
    Transform::from_translation(Vec3::ZERO),  // mesh already in world coords
    Visibility::default(),
))
```

Transform::ZERO because vertex positions are already world-space. Bevy frustum culling uses mesh AABB.

### Z-layers (future)

```
z = 0.0  background tiles
z = 1.0  foreground tiles (current)
z = 2.0  player, entities
z = 3.0  decorations
```

## 6. Update Flow

### Block break

1. `block_action_system`: world_pos → tile coord, apply damage based on hardness
2. damage >= 255 → `world_map.set_tile(x, y, AIR)`, reset damage
3. `update_bitmasks_around(x, y)`: recompute 9 bitmasks (3x3 around change)
4. `mark_dirty`: 1-2 chunks (2 if on chunk boundary)
5. `rebuild_dirty_chunks`: build new mesh, replace handle

### Block place

Same flow but `set_tile(x, y, selected_id)`, no damage step.

### Cross-chunk boundary

Tile (31, 7) in chunk A broken → neighbors (0,6), (0,7), (0,8) in chunk B need bitmask update. `update_bitmasks_around` handles this transparently. Both chunks A and B marked dirty.

### System ordering

```rust
app.add_systems(Update, (
    block_action_system,
    chunk_loading_system,
    rebuild_dirty_chunks,
).chain());
```

## 7. Chunk Lifecycle

### Spawn

1. `chunk_loading_system` detects chunk in visible range but not loaded
2. `world_map.get_or_generate()` → ChunkData (terrain_gen if new)
3. Compute bitmasks for all 1024 tiles (neighbors via world_map.get_tile() which auto-generates)
4. `build_chunk_mesh()` → Mesh handle
5. Spawn entity, register in `LoadedChunks`

### Despawn

1. Chunk outside visible range → despawn entity
2. Mesh handle dropped (Bevy ref counting)
3. ChunkData stays in WorldMap (player may return)
4. Future: LRU eviction for memory pressure

### Resources

```rust
#[derive(Resource)]
struct LoadedChunks { map: HashMap<IVec2, Entity> }

#[derive(Resource)]
struct TileAtlasMaterial { handle: Handle<TileMaterial> }

#[derive(Resource)]
struct MeshBuildBuffers { positions: Vec<[f32; 2]>, uvs: Vec<[f32; 2]>, indices: Vec<u32> }
```

### Performance budget

| Scenario | Time | Frame % |
|----------|------|---------|
| Idle frame | ~0.1ms | <1% |
| Block break | ~0.3ms | 2% |
| Fast movement (5 chunks) | ~1.5ms | 9% |
| Teleport (20 chunks) | ~6ms | 36% (one frame) |

Future optimization: chunk load queue (max N per frame) if teleport becomes an issue.

## What Gets Removed

- `bevy_ecs_tilemap` dependency from Cargo.toml
- All `TilemapBundle`, `TileBundle`, `TileStorage`, `TileTextureIndex` usage
- `texture_index` field from TileDef
- `load_tile_atlas` function (loads nonexistent `tiles.png`)

## What Gets Added

- `src/world/tile_renderer.rs` — TileMaterial, mesh building, rebuild system
- `src/world/atlas.rs` — AtlasBuilder, atlas loading pipeline
- `src/world/autotile.rs` — AutotileRegistry, bitmask computation, variant selection
- `assets/shaders/tile.wgsl` — trivial tile shader
- `*.autotile.ron` — rename from `*.ron` for autotile mappings

## References

- Starbound: mesh-per-chunk with OpenGL VBOs, per-frame geometry rebuild, planned chunk caching (confirmed by dev blog). Sub-blob quarter-tiles (4 quads per tile). XXHash for variants. 32x32 chunks.
- GPU texture limits: min 4096x4096 guaranteed, horizontal atlas (3200x752) fits any GPU.
