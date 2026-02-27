# Background Tile Layer Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a Starbound-style background tile layer with independent fg/bg grids, dual-layer lighting, fg→bg shadow, and right-click bg interaction.

**Architecture:** ChunkData splits into `fg: TileLayer` / `bg: TileLayer` with shared `light_levels`. Two meshes per chunk (bg z=-1, fg z=0). Shader gains `bg_dim` uniform. Lighting uses `max(fg_opacity, bg_opacity)`.

**Tech Stack:** Bevy 0.18, WGSL shaders, RON assets, custom Material2d with AsBindGroup uniforms.

**Design doc:** `docs/plans/2026-02-27-background-tiles-design.md`

**Verification after each task:** `cargo test && cargo clippy -- -D warnings`

---

### Task 1: TileLayer struct, Layer enum, ChunkData refactor

**Files:**
- Modify: `src/world/chunk.rs` (ChunkData, WorldMap methods, coordinate helpers, tests)

**Context:** ChunkData currently has flat `tiles: Vec<TileId>` and `bitmasks: Vec<u8>`. We extract these into a `TileLayer` struct and add `fg`/`bg` fields to ChunkData.

**Step 1: Add TileLayer and Layer**

In `src/world/chunk.rs`, above `ChunkData`, add:

```rust
/// Identifies which tile layer to operate on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    Fg,
    Bg,
}

/// Tile and bitmask data for a single layer within a chunk.
pub struct TileLayer {
    pub tiles: Vec<TileId>,
    pub bitmasks: Vec<u8>,
}

impl TileLayer {
    pub fn new_air(len: usize) -> Self {
        Self {
            tiles: vec![TileId::AIR; len],
            bitmasks: vec![0; len],
        }
    }

    pub fn get(&self, local_x: u32, local_y: u32, chunk_size: u32) -> TileId {
        self.tiles[(local_y * chunk_size + local_x) as usize]
    }

    pub fn set(&mut self, local_x: u32, local_y: u32, tile: TileId, chunk_size: u32) {
        self.tiles[(local_y * chunk_size + local_x) as usize] = tile;
    }
}
```

**Step 2: Refactor ChunkData**

Replace ChunkData definition:

```rust
pub struct ChunkData {
    pub fg: TileLayer,
    pub bg: TileLayer,
    /// Per-tile RGB light level (shared across both layers).
    pub light_levels: Vec<[u8; 3]>,
    #[allow(dead_code)]
    pub damage: Vec<u8>,
}

impl ChunkData {
    /// Access a specific layer.
    pub fn layer(&self, layer: Layer) -> &TileLayer {
        match layer {
            Layer::Fg => &self.fg,
            Layer::Bg => &self.bg,
        }
    }

    /// Mutable access to a specific layer.
    pub fn layer_mut(&mut self, layer: Layer) -> &mut TileLayer {
        match layer {
            Layer::Fg => &mut self.fg,
            Layer::Bg => &mut self.bg,
        }
    }
}
```

Remove the old `get`/`set` methods from the old `impl ChunkData` block.

**Step 3: Update get_or_generate_chunk**

In `WorldMap::get_or_generate_chunk`, update the `or_insert_with` closure. `terrain_gen::generate_chunk_tiles` will be updated in Task 3 to return both layers, but for now generate bg as all AIR to keep compiling:

```rust
self.chunks.entry((chunk_x, chunk_y)).or_insert_with(|| {
    let tiles = terrain_gen::generate_chunk_tiles(chunk_x, chunk_y, ctx);
    let len = tiles.len();
    ChunkData {
        fg: TileLayer { tiles, bitmasks: vec![0; len] },
        bg: TileLayer::new_air(len),
        light_levels: vec![[0, 0, 0]; len],
        damage: vec![0; len],
    }
})
```

**Step 4: Update WorldMap get_tile / set_tile / is_solid / get_tile_mut**

Add `layer: Layer` parameter to `get_tile`, `get_tile_mut`, `set_tile`. Keep `is_solid` checking foreground only (movement physics).

```rust
pub fn get_tile(&self, tile_x: i32, tile_y: i32, layer: Layer, ctx: &WorldCtxRef) -> Option<TileId> {
    if tile_y < 0 {
        return match layer {
            Layer::Fg => Some(ctx.tile_registry.by_name("stone")),
            Layer::Bg => Some(ctx.tile_registry.by_name("stone")),
        };
    }
    if tile_y >= ctx.config.height_tiles {
        return Some(TileId::AIR);
    }
    let wrapped_x = ctx.config.wrap_tile_x(tile_x);
    let (cx, cy) = tile_to_chunk(wrapped_x, tile_y, ctx.config.chunk_size);
    let (lx, ly) = tile_to_local(wrapped_x, tile_y, ctx.config.chunk_size);
    self.chunks
        .get(&(cx, cy))
        .map(|chunk| chunk.layer(layer).get(lx, ly, ctx.config.chunk_size))
}

pub fn get_tile_mut(&mut self, tile_x: i32, tile_y: i32, layer: Layer, ctx: &WorldCtxRef) -> TileId {
    if tile_y < 0 {
        return ctx.tile_registry.by_name("stone");
    }
    if tile_y >= ctx.config.height_tiles {
        return TileId::AIR;
    }
    let wrapped_x = ctx.config.wrap_tile_x(tile_x);
    let (cx, cy) = tile_to_chunk(wrapped_x, tile_y, ctx.config.chunk_size);
    let (lx, ly) = tile_to_local(wrapped_x, tile_y, ctx.config.chunk_size);
    self.get_or_generate_chunk(cx, cy, ctx)
        .layer(layer)
        .get(lx, ly, ctx.config.chunk_size)
}

pub fn set_tile(&mut self, tile_x: i32, tile_y: i32, layer: Layer, tile: TileId, ctx: &WorldCtxRef) {
    if tile_y < 0 || tile_y >= ctx.config.height_tiles {
        return;
    }
    let wrapped_x = ctx.config.wrap_tile_x(tile_x);
    let (cx, cy) = tile_to_chunk(wrapped_x, tile_y, ctx.config.chunk_size);
    let (lx, ly) = tile_to_local(wrapped_x, tile_y, ctx.config.chunk_size);
    self.get_or_generate_chunk(cx, cy, ctx);
    self.chunks
        .get_mut(&(cx, cy))
        .unwrap()
        .layer_mut(layer)
        .set(lx, ly, tile, ctx.config.chunk_size);
}

/// Read-only: returns whether fg tile is solid (false for unloaded chunks).
/// Only checks foreground — background tiles do not block movement.
pub fn is_solid(&self, tile_x: i32, tile_y: i32, ctx: &WorldCtxRef) -> bool {
    self.get_tile(tile_x, tile_y, Layer::Fg, ctx)
        .is_some_and(|tile| ctx.tile_registry.is_solid(tile))
}
```

**Step 5: Update update_bitmasks_around and init_chunk_bitmasks**

These need a `layer: Layer` parameter to operate on the correct layer:

```rust
pub fn update_bitmasks_around(
    world_map: &mut WorldMap,
    center_x: i32,
    center_y: i32,
    layer: Layer,
    ctx: &WorldCtxRef,
) -> HashSet<(i32, i32)> {
    // ... same logic but use world_map.get_tile_mut(bx, by, layer, ctx) in compute_bitmask
    // and chunk.layer_mut(layer).bitmasks[idx] = new_mask;
}

pub fn init_chunk_bitmasks(
    world_map: &mut WorldMap,
    chunk_x: i32,
    chunk_y: i32,
    layer: Layer,
    ctx: &WorldCtxRef,
) -> Vec<u8> {
    // ... same logic but use world_map.get_tile_mut(x, y, layer, ctx)
}
```

**Step 6: Update spawn_chunk to call bitmasks for both layers**

In `spawn_chunk`, compute bitmasks for both fg and bg:

```rust
let fg_bitmasks = init_chunk_bitmasks(world_map, data_chunk_x, chunk_y, Layer::Fg, ctx);
let bg_bitmasks = init_chunk_bitmasks(world_map, data_chunk_x, chunk_y, Layer::Bg, ctx);
if let Some(chunk) = world_map.chunks.get_mut(&(data_chunk_x, chunk_y)) {
    chunk.fg.bitmasks = fg_bitmasks;
    chunk.bg.bitmasks = bg_bitmasks;
}
```

Note: The mesh building in spawn_chunk will be updated in Task 8 (ChunkEntities). For now, update references from `chunk_data.tiles` to `chunk_data.fg.tiles` and `chunk_data.bitmasks` to `chunk_data.fg.bitmasks` to keep compiling.

**Step 7: Update rebuild_dirty_chunks**

Change `chunk_data.tiles` → `chunk_data.fg.tiles`, `chunk_data.bitmasks` → `chunk_data.fg.bitmasks`. Bg mesh rebuild will be added in Task 8.

**Step 8: Update all callers that use get_tile/set_tile/get_tile_mut**

- `src/interaction/block_action.rs`: `get_tile(x, y, Layer::Fg, ctx)`, `set_tile(x, y, Layer::Fg, tile, ctx)`
- `src/world/lighting.rs`: `get_tile(x, y, Layer::Fg, ctx)` — will be updated properly in Task 4

**Step 9: Update tests in chunk.rs**

All `get_tile`, `set_tile`, `get_tile_mut` calls need `Layer::Fg` parameter. Update all test functions.

**Step 10: Run tests and clippy**

```bash
cargo test && cargo clippy -- -D warnings
```

**Step 11: Commit**

```bash
git add -A && git commit -m "refactor: extract TileLayer struct with fg/bg in ChunkData"
```

---

### Task 2: Terrain generation — background tiles

**Files:**
- Modify: `src/world/terrain_gen.rs` (generate_tile, generate_chunk_tiles)
- Modify: `src/world/chunk.rs` (get_or_generate_chunk)

**Context:** Currently `generate_chunk_tiles` returns one `Vec<TileId>`. We need it to return both fg and bg.

**Step 1: Add generate_bg_tile function**

In `src/world/terrain_gen.rs`:

```rust
/// Generate a background tile at the given position.
/// Below surface: always fill_block (including caves). Above surface: AIR.
pub fn generate_bg_tile(tile_x: i32, tile_y: i32, ctx: &WorldCtxRef) -> TileId {
    let wc = ctx.config;
    if tile_y < 0 || tile_y >= wc.height_tiles {
        return TileId::AIR;
    }

    let tile_x = wc.wrap_tile_x(tile_x);

    let surface_y = surface_height(
        ctx.noise_cache,
        tile_x,
        wc,
        ctx.planet_config.layers.surface.terrain_frequency,
        ctx.planet_config.layers.surface.terrain_amplitude,
    );

    if tile_y > surface_y {
        return TileId::AIR;
    }

    // Below (or at) surface: always fill_block — including caves
    let layer = WorldLayer::from_tile_y(tile_y, ctx.planet_config);
    let biome_id = match layer {
        WorldLayer::Surface => ctx.biome_map.biome_at(tile_x as u32),
        WorldLayer::Underground => ctx.biome_registry.id_by_name(
            ctx.planet_config.layers.underground.primary_biome.as_deref().unwrap_or("underground_dirt"),
        ),
        WorldLayer::DeepUnderground => ctx.biome_registry.id_by_name(
            ctx.planet_config.layers.deep_underground.primary_biome.as_deref().unwrap_or("underground_rock"),
        ),
        WorldLayer::Core => ctx.biome_registry.id_by_name(
            ctx.planet_config.layers.core.primary_biome.as_deref().unwrap_or("core_magma"),
        ),
    };
    let biome = ctx.biome_registry.get(biome_id);
    biome.fill_block
}
```

**Step 2: Return struct from generate_chunk_tiles**

```rust
pub struct ChunkTiles {
    pub fg: Vec<TileId>,
    pub bg: Vec<TileId>,
}

pub fn generate_chunk_tiles(chunk_x: i32, chunk_y: i32, ctx: &WorldCtxRef) -> ChunkTiles {
    let chunk_size = ctx.config.chunk_size;
    let base_x = chunk_x * chunk_size as i32;
    let base_y = chunk_y * chunk_size as i32;
    let cap = (chunk_size * chunk_size) as usize;
    let mut fg = Vec::with_capacity(cap);
    let mut bg = Vec::with_capacity(cap);

    for local_y in 0..chunk_size as i32 {
        for local_x in 0..chunk_size as i32 {
            let x = base_x + local_x;
            let y = base_y + local_y;
            fg.push(generate_tile(x, y, ctx));
            bg.push(generate_bg_tile(x, y, ctx));
        }
    }

    ChunkTiles { fg, bg }
}
```

**Step 3: Update get_or_generate_chunk in chunk.rs**

```rust
self.chunks.entry((chunk_x, chunk_y)).or_insert_with(|| {
    let chunk_tiles = terrain_gen::generate_chunk_tiles(chunk_x, chunk_y, ctx);
    let len = chunk_tiles.fg.len();
    ChunkData {
        fg: TileLayer { tiles: chunk_tiles.fg, bitmasks: vec![0; len] },
        bg: TileLayer { tiles: chunk_tiles.bg, bitmasks: vec![0; len] },
        light_levels: vec![[0, 0, 0]; len],
        damage: vec![0; len],
    }
})
```

**Step 4: Add tests for bg generation**

```rust
#[test]
fn above_surface_bg_is_air() {
    let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
    let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
    let h = surface_height(&nc, 500, &wc, pc.layers.surface.terrain_frequency, pc.layers.surface.terrain_amplitude);
    assert_eq!(generate_bg_tile(500, h + 1, &ctx), TileId::AIR);
}

#[test]
fn below_surface_bg_is_fill_block() {
    let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
    let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
    let h = surface_height(&nc, 500, &wc, pc.layers.surface.terrain_frequency, pc.layers.surface.terrain_amplitude);
    let bg = generate_bg_tile(500, h - 5, &ctx);
    assert_ne!(bg, TileId::AIR, "bg below surface should be fill_block, not air");
}

#[test]
fn cave_has_bg_but_no_fg() {
    // Find a cave tile (fg=AIR below surface) and verify bg is NOT air
    let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
    let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
    let h = surface_height(&nc, 500, &wc, pc.layers.surface.terrain_frequency, pc.layers.surface.terrain_amplitude);
    // Scan below surface for a cave (fg=AIR)
    for y in 0..h {
        if generate_tile(500, y, &ctx) == TileId::AIR {
            let bg = generate_bg_tile(500, y, &ctx);
            assert_ne!(bg, TileId::AIR, "cave at y={y} should have bg wall");
            return;
        }
    }
    // No cave found in this column — test is inconclusive, skip
}
```

**Step 5: Update existing terrain_gen tests that reference generate_chunk_tiles return type**

`chunk_generation_has_correct_size` and `chunk_generation_is_deterministic` — update to use `.fg`.

**Step 6: Run tests and clippy, commit**

```bash
cargo test && cargo clippy -- -D warnings
git add -A && git commit -m "feat: generate background tiles below surface (caves have bg walls)"
```

---

### Task 3: Lighting — dual-layer opacity

**Files:**
- Modify: `src/world/lighting.rs` (compute_chunk_sunlight, bfs_from_emitter, compute_point_lights)

**Context:** Currently lighting reads opacity from fg tile only. We need `max(fg_opacity, bg_opacity)`.

**Step 1: Add helper function for effective opacity**

```rust
/// Effective light opacity at a position, considering both fg and bg layers.
/// Uses max(fg_opacity, bg_opacity) so either layer can block light.
fn effective_opacity(world_map: &WorldMap, tile_x: i32, tile_y: i32, ctx: &WorldCtxRef) -> u8 {
    let fg_opacity = world_map
        .get_tile(tile_x, tile_y, Layer::Fg, ctx)
        .map(|t| ctx.tile_registry.light_opacity(t))
        .unwrap_or(0);
    let bg_opacity = world_map
        .get_tile(tile_x, tile_y, Layer::Bg, ctx)
        .map(|t| ctx.tile_registry.light_opacity(t))
        .unwrap_or(0);
    fg_opacity.max(bg_opacity)
}
```

**Step 2: Update compute_chunk_sunlight**

Replace the two places that read `light_opacity` from fg tile:

Above-chunk scan:
```rust
if let Some(tile) = world_map.get_tile(tile_x, y, Layer::Fg, ctx) {
    let opacity = ctx.tile_registry.light_opacity(tile);
```
becomes:
```rust
let opacity = effective_opacity(world_map, tile_x, y, ctx);
```

In-chunk scan — currently reads from `chunk.get(local_x, local_y, cs)` directly. Change to use `effective_opacity` with world coords:
```rust
let world_y = base_y + local_y as i32;
let opacity = effective_opacity(world_map, tile_x, world_y, ctx);
```

**Step 3: Update bfs_from_emitter**

Replace:
```rust
let opacity = world_map
    .get_tile(wrapped_x, y, ctx)
    .map(|t| ctx.tile_registry.light_opacity(t))
    .unwrap_or(0);
```
with:
```rust
let opacity = effective_opacity(world_map, wrapped_x, y, ctx);
```

**Step 4: Update compute_point_lights emitter scan**

The scan for emitters should check both layers (a torch could be on fg or bg... though currently only fg). For future-proofing, scan fg layer for emitters:
```rust
let tile = world_map.get_tile(wrapped_x, scan_y, Layer::Fg, ctx);
```

**Step 5: Add test**

```rust
#[test]
fn bg_tile_blocks_sunlight() {
    let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
    let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
    let cs = wc.chunk_size;
    let stone = tr.by_name("stone");
    let top_chunk_y = wc.height_chunks() - 1;
    let mut map = WorldMap::default();

    // Generate chunk, set fg=AIR everywhere, bg=stone everywhere
    map.get_or_generate_chunk(0, top_chunk_y, &ctx);
    let chunk = map.chunks.get_mut(&(0, top_chunk_y)).unwrap();
    for i in 0..(cs * cs) as usize {
        chunk.fg.tiles[i] = TileId::AIR;
        chunk.bg.tiles[i] = stone;
    }

    let result = compute_chunk_sunlight(&map, 0, top_chunk_y, &ctx);
    // Bottom should be dark — bg stone blocks light even with fg=AIR
    assert!(is_dark(result[0]), "bg stone should block sunlight, got {:?}", result[0]);
}
```

**Step 6: Run tests and clippy, commit**

```bash
cargo test && cargo clippy -- -D warnings
git add -A && git commit -m "feat: lighting uses max(fg, bg) opacity for dual-layer blocking"
```

---

### Task 4: Sprite variant with Layer in hash

**Files:**
- Modify: `src/world/autotile.rs` (position_hash, select_variant)
- Modify: `src/world/mesh_builder.rs` (build_chunk_mesh call to select_variant)
- Modify: `src/world/chunk.rs` (Layer re-export if needed)

**Context:** `select_variant` uses `position_hash(x, y, seed)`. We add `Layer` to the hash so fg/bg get different variants.

**Step 1: Update position_hash**

```rust
pub fn position_hash(x: i32, y: i32, seed: u32, layer: u32) -> f32 {
    let mut h: u32 = 2166136261;
    h ^= x as u32;
    h = h.wrapping_mul(16777619);
    h ^= y as u32;
    h = h.wrapping_mul(16777619);
    h ^= seed;
    h = h.wrapping_mul(16777619);
    h ^= layer;
    h = h.wrapping_mul(16777619);
    (h as f32) / (u32::MAX as f32)
}
```

**Step 2: Update select_variant**

```rust
pub fn select_variant(variants: &[SpriteVariant], x: i32, y: i32, seed: u32, layer: u32) -> u32 {
    // ... same but call position_hash(x, y, seed, layer)
}
```

**Step 3: Update callers**

In `mesh_builder.rs` `build_chunk_mesh`, add `layer: Layer` parameter:
```rust
pub fn build_chunk_mesh(
    tiles: &[TileId],
    bitmasks: &[u8],
    light_levels: &[[u8; 3]],
    display_chunk_x: i32,
    chunk_y: i32,
    chunk_size: u32,
    tile_size: f32,
    seed: u32,
    layer: Layer,
    tile_registry: &TileRegistry,
    autotile_registry: &AutotileRegistry,
    atlas_params: &AtlasParams,
    buffers: &mut MeshBuildBuffers,
) -> Mesh {
```

And inside:
```rust
let layer_val = match layer { Layer::Fg => 0, Layer::Bg => 1 };
let sprite_row = select_variant(variants, world_x, world_y, seed, layer_val);
```

Update all call sites of `build_chunk_mesh` to pass `Layer::Fg` (bg calls added in Task 8).

**Step 4: Update tests**

Update autotile tests that call `position_hash` and `select_variant` with extra `0` layer param. Add test verifying different layers produce different variants:

```rust
#[test]
fn position_hash_varies_by_layer() {
    let h_fg = position_hash(10, 20, 42, 0);
    let h_bg = position_hash(10, 20, 42, 1);
    assert_ne!(h_fg, h_bg);
}
```

**Step 5: Run tests and clippy, commit**

```bash
cargo test && cargo clippy -- -D warnings
git add -A && git commit -m "feat: include layer in sprite variant hash for fg/bg differentiation"
```

---

### Task 5: Shader — bg_dim uniform

**Files:**
- Modify: `src/world/tile_renderer.rs` (TileMaterial, SharedTileMaterial)
- Modify: `assets/shaders/tile.wgsl`
- Modify: `src/world/mod.rs` (if needed for second material)

**Context:** TileMaterial currently has only atlas texture. We add a `bg_dim` uniform float. Fg material uses dim=1.0, bg material uses dim=0.6.

**Step 1: Update TileMaterial**

```rust
#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct TileMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub atlas: Handle<Image>,
    #[uniform(2)]
    pub dim: f32,
}
```

**Step 2: Update SharedTileMaterial**

Replace with two handles:

```rust
#[derive(Resource)]
pub struct SharedTileMaterial {
    pub fg: Handle<TileMaterial>,
    pub bg: Handle<TileMaterial>,
}
```

**Step 3: Update shader**

Add uniform struct and binding, multiply in fragment:

```wgsl
struct TileUniforms {
    dim: f32,
}

@group(2) @binding(0) var atlas_texture: texture_2d<f32>;
@group(2) @binding(1) var atlas_sampler: sampler;
@group(2) @binding(2) var<uniform> uniforms: TileUniforms;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(atlas_texture, atlas_sampler, in.uv);
    if color.a < 0.01 {
        discard;
    }
    return vec4<f32>(color.rgb * in.light * uniforms.dim, color.a);
}
```

**Step 4: Update all SharedTileMaterial creation**

Find where `SharedTileMaterial` is created (likely in loading/setup system) and create two materials:
- fg: `TileMaterial { atlas: atlas_handle.clone(), dim: 1.0 }`
- bg: `TileMaterial { atlas: atlas_handle.clone(), dim: 0.6 }`

**Step 5: Update spawn_chunk material references**

Change `material.handle.clone()` to `material.fg.clone()` for foreground entities. Bg entities (Task 8) will use `material.bg.clone()`.

**Step 6: Run tests and clippy, commit**

```bash
cargo test && cargo clippy -- -D warnings
git add -A && git commit -m "feat: add bg_dim uniform to tile shader for background darkening"
```

---

### Task 6: Mesh builder — fg→bg shadow

**Files:**
- Modify: `src/world/mesh_builder.rs` (new function for shadow-adjusted light)

**Context:** When building the bg mesh, we want additional darkening where foreground tiles are present. This is baked into the light vertex attribute.

**Step 1: Add fg shadow computation**

Add a function that computes shadow-adjusted light for background mesh vertices:

```rust
/// Compute shadow factor for a bg tile based on neighboring fg tile presence.
/// Returns a multiplier in [0.0, 1.0] where 0.0 = full shadow, 1.0 = no shadow.
/// Uses corner averaging: checks 4 tiles sharing the vertex corner.
fn fg_shadow_at(
    fg_tiles: &[TileId],
    chunk_size: u32,
    local_x: i32,
    local_y: i32,
    tile_registry: &TileRegistry,
) -> f32 {
    let mut shadow_count = 0u32;
    let mut total = 0u32;
    // Check 2x2 area around this position (the tile itself + neighbors in dx,dy direction)
    for dy in 0..=1 {
        for dx in 0..=1 {
            let nx = local_x + dx;
            let ny = local_y + dy;
            total += 1;
            if nx >= 0 && nx < chunk_size as i32 && ny >= 0 && ny < chunk_size as i32 {
                let idx = (ny * chunk_size as i32 + nx) as usize;
                if tile_registry.is_solid(fg_tiles[idx]) {
                    shadow_count += 1;
                }
            }
        }
    }
    // 0 fg neighbors = 1.0 (no shadow), 4 fg neighbors = FG_SHADOW_DIM
    let ratio = shadow_count as f32 / total as f32;
    1.0 - ratio * (1.0 - FG_SHADOW_DIM)
}

const FG_SHADOW_DIM: f32 = 0.5;
```

**Step 2: Add build_bg_chunk_mesh or extend build_chunk_mesh**

Add optional `fg_tiles` parameter for shadow computation. When building bg mesh, pass `Some(&chunk.fg.tiles)`:

```rust
pub fn build_chunk_mesh(
    tiles: &[TileId],
    bitmasks: &[u8],
    light_levels: &[[u8; 3]],
    fg_tiles: Option<&[TileId]>,  // Some for bg mesh (shadow source), None for fg mesh
    display_chunk_x: i32,
    chunk_y: i32,
    chunk_size: u32,
    tile_size: f32,
    seed: u32,
    layer: Layer,
    tile_registry: &TileRegistry,
    autotile_registry: &AutotileRegistry,
    atlas_params: &AtlasParams,
    buffers: &mut MeshBuildBuffers,
) -> Mesh {
```

Inside the per-vertex light computation, after `corner_light()`, apply shadow if `fg_tiles` is Some:

```rust
let shadow = if let Some(fg) = fg_tiles {
    fg_shadow_at(fg, chunk_size, lx, ly, tile_registry)
} else {
    1.0
};
// Apply shadow to each corner light
let bl = corner_light(...);
let bl = [bl[0] * shadow, bl[1] * shadow, bl[2] * shadow];
// ... same for br, tr, tl
```

Actually, shadow should be per-vertex (corner-averaged) too. Better approach — compute shadow per corner:

```rust
fn corner_shadow(
    fg_tiles: &[TileId],
    chunk_size: u32,
    local_x: i32,
    local_y: i32,
    dx: i32,
    dy: i32,
    tile_registry: &TileRegistry,
) -> f32 {
    let positions = [
        (local_x, local_y),
        (local_x + dx, local_y),
        (local_x, local_y + dy),
        (local_x + dx, local_y + dy),
    ];
    let mut shadow_count = 0u32;
    for (nx, ny) in positions {
        let cx = nx.clamp(0, chunk_size as i32 - 1) as u32;
        let cy = ny.clamp(0, chunk_size as i32 - 1) as u32;
        let idx = (cy * chunk_size + cx) as usize;
        if tile_registry.is_solid(fg_tiles[idx]) {
            shadow_count += 1;
        }
    }
    let ratio = shadow_count as f32 / 4.0;
    1.0 - ratio * (1.0 - FG_SHADOW_DIM)
}
```

Then per vertex:
```rust
let shadow = if let Some(fg) = fg_tiles {
    corner_shadow(fg, chunk_size, lx, ly, dx, dy, tile_registry)
} else {
    1.0
};
let bl = corner_light(light_levels, chunk_size, lx, ly, -1, -1);
let bl = [bl[0] * shadow_bl, bl[1] * shadow_bl, bl[2] * shadow_bl];
```

**Step 3: Update all build_chunk_mesh call sites**

Pass `None` for fg_tiles in current calls (fg mesh). Bg calls added in Task 8.

**Step 4: Add test**

```rust
#[test]
fn corner_shadow_no_fg_returns_one() {
    let fg_tiles = vec![TileId::AIR; 4]; // 2x2 all air
    let reg = test_registry();
    let s = corner_shadow(&fg_tiles, 2, 0, 0, 1, 1, &reg);
    assert!((s - 1.0).abs() < f32::EPSILON);
}

#[test]
fn corner_shadow_full_fg_returns_dim() {
    let reg = test_registry();
    let fg_tiles = vec![TileId(1); 4]; // 2x2 all solid (dirt)
    let s = corner_shadow(&fg_tiles, 2, 0, 0, 1, 1, &reg);
    assert!((s - FG_SHADOW_DIM).abs() < 0.01);
}
```

**Step 5: Run tests and clippy, commit**

```bash
cargo test && cargo clippy -- -D warnings
git add -A && git commit -m "feat: fg->bg shadow via per-vertex corner averaging in bg mesh"
```

---

### Task 7: ChunkEntities — spawn, despawn, rebuild both meshes

**Files:**
- Modify: `src/world/chunk.rs` (ChunkEntities, spawn_chunk, despawn_chunk, chunk_loading_system, rebuild_dirty_chunks, LoadedChunks)

**Context:** Currently one Entity per chunk. We need two (fg + bg). This is the integration task.

**Step 1: Add ChunkEntities and ChunkLayer marker**

```rust
/// Identifies whether a chunk entity is foreground or background.
#[derive(Component)]
pub struct ChunkLayer(pub Layer);

/// Both entities for a loaded chunk.
pub struct ChunkEntities {
    pub fg: Entity,
    pub bg: Entity,
}
```

Update `LoadedChunks`:
```rust
#[derive(Resource, Default)]
pub struct LoadedChunks {
    pub(crate) map: HashMap<(i32, i32), ChunkEntities>,
}
```

**Step 2: Update spawn_chunk**

After computing bitmasks and lighting, build both meshes and spawn both entities:

```rust
// Build bg mesh
let bg_mesh = build_chunk_mesh(
    &chunk_data.bg.tiles,
    &chunk_data.bg.bitmasks,
    &chunk_data.light_levels,
    Some(&chunk_data.fg.tiles),  // shadow source
    display_chunk_x, chunk_y, ctx.config.chunk_size, ctx.config.tile_size,
    ctx.config.seed, Layer::Bg,
    ctx.tile_registry, autotile_registry, &atlas.params, buffers,
);
let bg_handle = meshes.add(bg_mesh);

// Build fg mesh
let fg_mesh = build_chunk_mesh(
    &chunk_data.fg.tiles,
    &chunk_data.fg.bitmasks,
    &chunk_data.light_levels,
    None,  // no shadow on fg
    display_chunk_x, chunk_y, ctx.config.chunk_size, ctx.config.tile_size,
    ctx.config.seed, Layer::Fg,
    ctx.tile_registry, autotile_registry, &atlas.params, buffers,
);
let fg_handle = meshes.add(fg_mesh);

let bg_entity = commands
    .spawn((
        ChunkCoord { x: display_chunk_x, y: chunk_y },
        ChunkLayer(Layer::Bg),
        Mesh2d(bg_handle),
        MeshMaterial2d(material.bg.clone()),
        Transform::from_translation(Vec3::new(0.0, 0.0, -1.0)),
        Visibility::default(),
    ))
    .id();

let fg_entity = commands
    .spawn((
        ChunkCoord { x: display_chunk_x, y: chunk_y },
        ChunkLayer(Layer::Fg),
        Mesh2d(fg_handle),
        MeshMaterial2d(material.fg.clone()),
        Transform::from_translation(Vec3::ZERO),
        Visibility::default(),
    ))
    .id();

loaded_chunks.map.insert((display_chunk_x, chunk_y), ChunkEntities { fg: fg_entity, bg: bg_entity });
```

**Step 3: Update despawn_chunk**

```rust
pub fn despawn_chunk(commands: &mut Commands, loaded_chunks: &mut LoadedChunks, chunk_x: i32, chunk_y: i32) {
    if let Some(entities) = loaded_chunks.map.remove(&(chunk_x, chunk_y)) {
        commands.entity(entities.fg).despawn();
        commands.entity(entities.bg).despawn();
    }
}
```

**Step 4: Update chunk_loading_system**

Change `loaded_chunks.map.contains_key` check — stays same (key is still `(i32,i32)`). The `to_remove` logic stays the same.

**Step 5: Update rebuild_dirty_chunks**

Query both fg and bg entities via `ChunkLayer` component. Rebuild both meshes:

```rust
pub fn rebuild_dirty_chunks(
    mut commands: Commands,
    query: Query<(Entity, &ChunkCoord, &ChunkLayer), With<ChunkDirty>>,
    mut meshes: ResMut<Assets<Mesh>>,
    world_map: Res<WorldMap>,
    wc: Res<WorldConfig>,
    registry: Res<TileRegistry>,
    autotile_registry: Res<AutotileRegistry>,
    atlas: Res<TileAtlas>,
    mut buffers: ResMut<MeshBuildBuffers>,
) {
    for (entity, coord, chunk_layer) in &query {
        let data_chunk_x = wc.wrap_chunk_x(coord.x);
        let Some(chunk_data) = world_map.chunks.get(&(data_chunk_x, coord.y)) else {
            continue;
        };

        let (tiles, bitmasks, fg_tiles_opt, layer) = match chunk_layer.0 {
            Layer::Fg => (&chunk_data.fg.tiles, &chunk_data.fg.bitmasks, None, Layer::Fg),
            Layer::Bg => (&chunk_data.bg.tiles, &chunk_data.bg.bitmasks, Some(chunk_data.fg.tiles.as_slice()), Layer::Bg),
        };

        let mesh = build_chunk_mesh(
            tiles, bitmasks, &chunk_data.light_levels, fg_tiles_opt,
            coord.x, coord.y, wc.chunk_size, wc.tile_size, wc.seed, layer,
            &registry, &autotile_registry, &atlas.params, &mut buffers,
        );

        let mesh_handle = meshes.add(mesh);
        commands.entity(entity).insert(Mesh2d(mesh_handle)).remove::<ChunkDirty>();
    }
}
```

**Step 6: Update block_action dirty marking**

When marking chunks dirty in `block_action.rs`, mark **both** fg and bg entities:

```rust
for (cx, cy) in all_dirty {
    for (&(display_cx, display_cy), entities) in &loaded_chunks.map {
        if ctx_ref.config.wrap_chunk_x(display_cx) == cx && display_cy == cy {
            commands.entity(entities.fg).insert(ChunkDirty);
            commands.entity(entities.bg).insert(ChunkDirty);
        }
    }
}
```

**Step 7: Run tests and clippy, commit**

```bash
cargo test && cargo clippy -- -D warnings
git add -A && git commit -m "feat: spawn/despawn/rebuild both fg and bg chunk entities"
```

---

### Task 8: Block interaction — right-click background

**Files:**
- Modify: `src/interaction/block_action.rs`

**Context:** Currently left=break fg, right=place torch. Change right-click to break/place bg.

**Step 1: Update block_interaction_system**

Left click — unchanged (break fg):
```rust
if left_click {
    let Some(current) = world_map.get_tile(tile_x, tile_y, Layer::Fg, &ctx_ref) else { return };
    if !ctx_ref.tile_registry.is_solid(current) { return; }
    world_map.set_tile(tile_x, tile_y, Layer::Fg, TileId::AIR, &ctx_ref);
}
```

Right click — break or place bg:
```rust
else if right_click {
    let Some(current_bg) = world_map.get_tile(tile_x, tile_y, Layer::Bg, &ctx_ref) else { return };

    if current_bg != TileId::AIR {
        // Break bg tile
        world_map.set_tile(tile_x, tile_y, Layer::Bg, TileId::AIR, &ctx_ref);
    } else {
        // Place bg tile — check adjacency rule
        let has_neighbor = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| {
            let nx = tile_x + dx;
            let ny = tile_y + dy;
            // Adjacent fg or bg tile exists
            world_map.get_tile(nx, ny, Layer::Fg, &ctx_ref).is_some_and(|t| t != TileId::AIR)
                || world_map.get_tile(nx, ny, Layer::Bg, &ctx_ref).is_some_and(|t| t != TileId::AIR)
        });
        if !has_neighbor { return; }

        // TODO: replace with player's selected block type from inventory
        let place_id = ctx_ref.tile_registry.by_name("dirt");
        world_map.set_tile(tile_x, tile_y, Layer::Bg, place_id, &ctx_ref);
    }
}
```

**Step 2: Update bitmask recomputation**

After left click (fg change):
```rust
let bitmask_dirty = update_bitmasks_around(&mut world_map, tile_x, tile_y, Layer::Fg, &ctx_ref);
```

After right click (bg change):
```rust
let bitmask_dirty = update_bitmasks_around(&mut world_map, tile_x, tile_y, Layer::Bg, &ctx_ref);
```

**Step 3: Mark both fg and bg entities dirty**

Already handled by the updated dirty marking from Task 7.

**Step 4: Run tests and clippy, commit**

```bash
cargo test && cargo clippy -- -D warnings
git add -A && git commit -m "feat: right-click breaks/places background tiles with adjacency check"
```

---

### Task 9: SharedTileMaterial creation update

**Files:**
- Modify: wherever `SharedTileMaterial` is constructed (find via grep)

**Context:** Need to find and update the loading system that creates SharedTileMaterial to produce two handles (fg dim=1.0, bg dim=0.6).

**Step 1: Find SharedTileMaterial creation**

```bash
grep -rn "SharedTileMaterial" src/
```

**Step 2: Update creation**

Create two `TileMaterial` instances and insert both handles into `SharedTileMaterial { fg, bg }`.

**Step 3: Run tests and clippy, commit**

```bash
cargo test && cargo clippy -- -D warnings
git add -A && git commit -m "feat: create separate fg/bg tile materials with dim uniform"
```

---

### Task 10: Final integration test and cleanup

**Files:**
- Modify: `src/test_helpers.rs` (update test fixtures if needed)
- Modify: any remaining compilation issues

**Step 1: Run full test suite**

```bash
cargo test && cargo clippy -- -D warnings
```

**Step 2: Fix any remaining issues**

Address clippy warnings, dead code, missing imports.

**Step 3: Manual play-test**

```bash
cargo run
```

Verify:
- Background tiles visible behind foreground (slightly dimmed)
- Caves have background walls (dark without torches)
- Left click breaks foreground
- Right click breaks/places background
- Parallax sky visible only through double-air gaps
- Lighting correct: breaking bg wall lets light through

**Step 4: Final commit**

```bash
cargo test && cargo clippy -- -D warnings
git add -A && git commit -m "feat: background tile layer complete — Starbound-style walls"
```
