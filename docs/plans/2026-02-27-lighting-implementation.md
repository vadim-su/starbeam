# Lighting System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement Starbound-style RGB colored lighting with sunlight propagation, point light BFS, and per-vertex smooth interpolation.

**Architecture:** Three-layer system: Simulation (light propagation in `lighting.rs`) → Mesh (per-vertex corner averaging in `mesh_builder.rs`) → GPU (custom vertex+fragment shader in `tile.wgsl`). Light data flows as `Vec<[u8; 3]>` per chunk, converted to `[f32; 3]` per vertex at mesh build time.

**Tech Stack:** Bevy 0.18, WGSL shaders, Material2d with custom vertex layout.

**Design doc:** `docs/plans/2026-02-27-lighting-design.md`

**Verification after every task:** `cargo test && cargo clippy -- -D warnings`

---

## Task 1: TileDef Light Properties

Add `light_emission: [u8; 3]` and `light_opacity: u8` to tile definitions.

**Files:**
- Modify: `src/registry/tile.rs:14-27` (TileDef struct)
- Modify: `src/registry/tile.rs:36-65` (TileRegistry — add accessor methods)
- Modify: `src/registry/tile.rs:71-114` (tests — update TileDef constructions)
- Modify: `assets/world/tiles.registry.ron` (add fields to all tiles)
- Modify: `src/test_helpers.rs:66-109` (test_tile_registry — update TileDef constructions)
- Modify: `src/world/mesh_builder.rs:142-165` (test TileDef constructions)

**Step 1: Add fields to TileDef**

In `src/registry/tile.rs`, add a default function and two new fields:

```rust
fn default_light_opacity() -> u8 {
    15
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct TileDef {
    pub id: String,
    pub autotile: Option<String>,
    pub solid: bool,
    pub hardness: f32,
    pub friction: f32,
    pub viscosity: f32,
    pub damage_on_contact: f32,
    #[serde(default)]
    pub effects: Vec<String>,
    #[serde(default)]
    pub light_emission: [u8; 3],
    #[serde(default = "default_light_opacity")]
    pub light_opacity: u8,
}
```

Default: emission `[0,0,0]`, opacity `15` (solid blocks light by default; air must explicitly set `0`).

**Step 2: Add TileRegistry accessors**

In `src/registry/tile.rs`, add to `impl TileRegistry`:

```rust
pub fn light_emission(&self, id: TileId) -> [u8; 3] {
    self.defs[id.0 as usize].light_emission
}

pub fn light_opacity(&self, id: TileId) -> u8 {
    self.defs[id.0 as usize].light_opacity
}
```

**Step 3: Update tiles.registry.ron**

```ron
(
  tiles: [
    ( id: "air",   autotile: None,          solid: false, hardness: 0.0, friction: 0.0, viscosity: 0.0, damage_on_contact: 0.0, effects: [], light_emission: (0, 0, 0), light_opacity: 0 ),
    ( id: "grass", autotile: Some("grass"),  solid: true,  hardness: 1.0, friction: 0.8, viscosity: 0.0, damage_on_contact: 0.0, effects: [], light_emission: (0, 0, 0), light_opacity: 15 ),
    ( id: "dirt",  autotile: Some("dirt"),   solid: true,  hardness: 2.0, friction: 0.7, viscosity: 0.0, damage_on_contact: 0.0, effects: [], light_emission: (0, 0, 0), light_opacity: 15 ),
    ( id: "stone", autotile: Some("stone"),  solid: true,  hardness: 5.0, friction: 0.6, viscosity: 0.0, damage_on_contact: 0.0, effects: [], light_emission: (0, 0, 0), light_opacity: 15 ),
  ]
)
```

**Step 4: Update all test TileDef constructions**

Every place that constructs `TileDef` needs the new fields. Add to ALL TileDef literals:

```rust
light_emission: [0, 0, 0],
light_opacity: 0, // for air
// or
light_opacity: 15, // for solid tiles
```

Files to update:
- `src/test_helpers.rs:66-109` — `test_tile_registry()`: 4 TileDef literals
- `src/registry/tile.rs:71-114` — `test_registry()`: 4 TileDef literals
- `src/world/mesh_builder.rs:142-165` — `test_registry()`: 2 TileDef literals

**Step 5: Add tests for new accessors**

In `src/registry/tile.rs` tests:

```rust
#[test]
fn light_properties() {
    let reg = test_registry();
    assert_eq!(reg.light_emission(TileId::AIR), [0, 0, 0]);
    assert_eq!(reg.light_opacity(TileId::AIR), 0);
    assert_eq!(reg.light_opacity(TileId(1)), 15); // grass
    assert_eq!(reg.light_opacity(TileId(3)), 15); // stone
}
```

**Step 6: Verify**

```bash
cargo test && cargo clippy -- -D warnings
```

**Step 7: Commit**

```bash
git add -A && git commit -m "feat(lighting): add light_emission and light_opacity to TileDef"
```

---

## Task 2: ChunkData RGB Light Levels

Change `light_levels` from `Vec<u8>` to `Vec<[u8; 3]>`. Keep default at `[255, 255, 255]` (bright) so the game still looks correct before the lighting system is wired in.

**Files:**
- Modify: `src/world/chunk.rs:27-34` (ChunkData struct)
- Modify: `src/world/chunk.rs:66-82` (get_or_generate_chunk — initialization)
- Modify: `src/world/mesh_builder.rs:39-52` (build_chunk_mesh — parameter type)
- Modify: `src/world/mesh_builder.rs:109-112` (light conversion logic)
- Modify: `src/world/mesh_builder.rs:192-308` (tests — light_levels type)

**Step 1: Change ChunkData field**

In `src/world/chunk.rs:27-34`:

```rust
pub struct ChunkData {
    pub tiles: Vec<TileId>,
    pub bitmasks: Vec<u8>,
    /// Per-tile light level RGB: [0,0,0] = full dark, [255,255,255] = full light.
    pub light_levels: Vec<[u8; 3]>,
    #[allow(dead_code)]
    pub damage: Vec<u8>,
}
```

**Step 2: Update initialization**

In `src/world/chunk.rs:72-81`, change:

```rust
light_levels: vec![[255, 255, 255]; len],
```

**Step 3: Update build_chunk_mesh parameter**

In `src/world/mesh_builder.rs:42`, change:

```rust
light_levels: &[[u8; 3]],
```

**Step 4: Update light conversion in build_chunk_mesh**

In `src/world/mesh_builder.rs:109-112`, change:

```rust
let light = [
    light_levels[idx][0] as f32 / 255.0,
    light_levels[idx][1] as f32 / 255.0,
    light_levels[idx][2] as f32 / 255.0,
];
buffers.lights.extend_from_slice(&[light, light, light, light]);
```

Note: `buffers.lights` type also changes — see Task 3.

Actually, for this task we keep `buffers.lights` as `Vec<f32>` and flatten the RGB. This will be changed in Task 3 to `Vec<[f32; 3]>`. For now, just make it compile:

```rust
let r = light_levels[idx][0] as f32 / 255.0;
let g = light_levels[idx][1] as f32 / 255.0;
let b = light_levels[idx][2] as f32 / 255.0;
// Temporary: use luminance until ATTRIBUTE_LIGHT becomes Float32x3 in Task 3
let lum = r * 0.299 + g * 0.587 + b * 0.114;
buffers.lights.extend_from_slice(&[lum, lum, lum, lum]);
```

**Step 5: Update tests**

In `src/world/mesh_builder.rs`, change all `light_levels` test data:

```rust
let light_levels = vec![[255u8, 255, 255]; 4];  // was: vec![255u8; 4]
```

**Step 6: Verify**

```bash
cargo test && cargo clippy -- -D warnings
```

**Step 7: Commit**

```bash
git add -A && git commit -m "feat(lighting): change ChunkData.light_levels to Vec<[u8; 3]> RGB"
```

---

## Task 3: Mesh Builder Float32x3 + Corner Averaging

Change ATTRIBUTE_LIGHT to Float32x3, update buffers, implement per-vertex corner averaging.

**Files:**
- Modify: `src/world/mesh_builder.rs:10-12` (ATTRIBUTE_LIGHT format)
- Modify: `src/world/mesh_builder.rs:14-32` (MeshBuildBuffers — lights type)
- Modify: `src/world/mesh_builder.rs:39-131` (build_chunk_mesh — corner averaging)
- Modify: `src/world/mesh_builder.rs:133-309` (tests)

**Step 1: Write corner averaging test**

Add to `src/world/mesh_builder.rs` tests:

```rust
#[test]
fn corner_averaging_uniform_light() {
    // All tiles same light → all vertices get same value
    let lights = vec![[200u8, 100, 50]; 4]; // 2x2 chunk, all same
    let chunk_size = 2u32;

    // For tile at (0,0), bottom-left vertex averages (0,0) and neighbors at (-1,-1), (-1,0), (0,-1)
    // Out-of-chunk neighbors fallback to edge value → all samples are [200, 100, 50]
    let bl = corner_light(&lights, chunk_size, 0, 0, -1, -1);
    assert!((bl[0] - 200.0 / 255.0).abs() < 0.01);
    assert!((bl[1] - 100.0 / 255.0).abs() < 0.01);
    assert!((bl[2] - 50.0 / 255.0).abs() < 0.01);
}

#[test]
fn corner_averaging_gradient() {
    // 2x2 chunk: top-left bright, bottom-right dark
    // Layout (row-major, y increases upward):
    //   idx 2 = (0,1) bright   idx 3 = (1,1) medium
    //   idx 0 = (0,0) medium   idx 1 = (1,0) dark
    let lights = vec![
        [128, 128, 128], // (0,0)
        [0, 0, 0],       // (1,0)
        [255, 255, 255], // (0,1)
        [128, 128, 128], // (1,1)
    ];
    let chunk_size = 2u32;

    // Shared corner between all 4 tiles: top-right of (0,0) = bottom-right of (0,1)
    //   = top-left of (1,0) = bottom-left of (1,1)
    // That vertex at local position (1, 1) averages tiles (0,0), (1,0), (0,1), (1,1)
    // = avg(128, 0, 255, 128) = 511/4 ≈ 127.75
    let tr = corner_light(&lights, chunk_size, 0, 0, 1, 1);
    let expected = (128.0 + 0.0 + 255.0 + 128.0) / (4.0 * 255.0);
    assert!((tr[0] - expected).abs() < 0.01);
}
```

**Step 2: Change ATTRIBUTE_LIGHT format**

In `src/world/mesh_builder.rs:10-12`:

```rust
/// Custom vertex attribute for per-vertex light level RGB (0.0 = dark, 1.0 = full).
pub const ATTRIBUTE_LIGHT: MeshVertexAttribute =
    MeshVertexAttribute::new("Light", 988_540_917, VertexFormat::Float32x3);
```

**Step 3: Change MeshBuildBuffers**

In `src/world/mesh_builder.rs:14-32`:

```rust
pub struct MeshBuildBuffers {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub lights: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl Default for MeshBuildBuffers {
    fn default() -> Self {
        Self {
            positions: Vec::with_capacity(CHUNK_TILE_COUNT * 4),
            uvs: Vec::with_capacity(CHUNK_TILE_COUNT * 4),
            lights: Vec::with_capacity(CHUNK_TILE_COUNT * 4),
            indices: Vec::with_capacity(CHUNK_TILE_COUNT * 6),
        }
    }
}
```

**Step 4: Add corner averaging helpers**

Add before `build_chunk_mesh`:

```rust
/// Get light at a local chunk position, clamping out-of-bounds to nearest edge tile.
fn get_light(light_levels: &[[u8; 3]], chunk_size: u32, lx: i32, ly: i32) -> [u8; 3] {
    let cx = lx.clamp(0, chunk_size as i32 - 1) as u32;
    let cy = ly.clamp(0, chunk_size as i32 - 1) as u32;
    light_levels[(cy * chunk_size + cx) as usize]
}

/// Compute smoothed light for one vertex by averaging 4 tiles sharing that corner.
/// `dx`, `dy`: direction to the 3 neighbor tiles (-1 or +1).
fn corner_light(
    light_levels: &[[u8; 3]],
    chunk_size: u32,
    local_x: i32,
    local_y: i32,
    dx: i32,
    dy: i32,
) -> [f32; 3] {
    let s0 = get_light(light_levels, chunk_size, local_x, local_y);
    let s1 = get_light(light_levels, chunk_size, local_x + dx, local_y);
    let s2 = get_light(light_levels, chunk_size, local_x, local_y + dy);
    let s3 = get_light(light_levels, chunk_size, local_x + dx, local_y + dy);
    [
        (s0[0] as f32 + s1[0] as f32 + s2[0] as f32 + s3[0] as f32) / (4.0 * 255.0),
        (s0[1] as f32 + s1[1] as f32 + s2[1] as f32 + s3[1] as f32) / (4.0 * 255.0),
        (s0[2] as f32 + s1[2] as f32 + s2[2] as f32 + s3[2] as f32) / (4.0 * 255.0),
    ]
}
```

**Step 5: Update build_chunk_mesh light section**

Replace lines 109-112 with:

```rust
let lx = local_x as i32;
let ly = local_y as i32;
let bl = corner_light(light_levels, chunk_size, lx, ly, -1, -1);
let br = corner_light(light_levels, chunk_size, lx, ly, 1, -1);
let tr = corner_light(light_levels, chunk_size, lx, ly, 1, 1);
let tl = corner_light(light_levels, chunk_size, lx, ly, -1, 1);
buffers.lights.extend_from_slice(&[bl, br, tr, tl]);
```

**Step 6: Update existing tests**

Update `build_mesh_2x2_chunk` test — lights are now `Vec<[f32; 3]>`:

```rust
assert_eq!(buffers.lights.len(), 8, "2 quads × 4 vertices");

// With uniform [255,255,255] light and corner averaging, all vertices ≈ 1.0
for l in &buffers.lights {
    assert!((l[0] - 1.0).abs() < f32::EPSILON, "R should be 1.0, got {}", l[0]);
    assert!((l[1] - 1.0).abs() < f32::EPSILON, "G should be 1.0, got {}", l[1]);
    assert!((l[2] - 1.0).abs() < f32::EPSILON, "B should be 1.0, got {}", l[2]);
}
```

Update `build_mesh_all_air_produces_empty_mesh` — change `light_levels` type:

```rust
let light_levels = vec![[255u8, 255, 255]; 4];
```

Update `MeshBuildBuffers` construction in tests:

```rust
let mut buffers = MeshBuildBuffers {
    positions: Vec::new(),
    uvs: Vec::new(),
    lights: Vec::new(),
    indices: Vec::new(),
};
```

**Step 7: Verify**

```bash
cargo test && cargo clippy -- -D warnings
```

**Step 8: Commit**

```bash
git add -A && git commit -m "feat(lighting): ATTRIBUTE_LIGHT Float32x3 with per-vertex corner averaging"
```

---

## Task 4: Custom Shader + TileMaterial

Rewrite `tile.wgsl` with custom vertex shader that passes light to fragment. Add `vertex_shader()` and `specialize()` to TileMaterial.

**Files:**
- Modify: `assets/shaders/tile.wgsl` (full rewrite)
- Modify: `src/world/tile_renderer.rs` (add vertex_shader + specialize)

**Step 1: Rewrite shader**

Replace entire `assets/shaders/tile.wgsl`:

```wgsl
#import bevy_sprite::mesh2d_functions

struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) light: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light: vec3<f32>,
}

@vertex
fn vertex(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = mesh2d_position_local_to_clip(
        mesh2d_functions::get_world_from_local(in.instance_index),
        vec4<f32>(in.position, 1.0),
    );
    out.uv = in.uv;
    out.light = in.light;
    return out;
}

@group(2) @binding(0) var atlas_texture: texture_2d<f32>;
@group(2) @binding(1) var atlas_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(atlas_texture, atlas_sampler, in.uv);
    if color.a < 0.01 {
        discard;
    }
    return vec4<f32>(color.rgb * in.light, color.a);
}
```

> **Note for implementer:** The exact `#import` path and `mesh2d_position_local_to_clip` API may differ in Bevy 0.18. Use ExternalScout to verify Bevy 0.18 mesh2d shader imports if the shader fails to compile. The key pattern is: import mesh2d functions → transform position to clip space → pass light as varying.

**Step 2: Update TileMaterial**

Replace entire `src/world/tile_renderer.rs`:

```rust
use bevy::prelude::*;
use bevy::render::mesh::MeshVertexBufferLayoutRef;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dKey};

use crate::world::mesh_builder::ATTRIBUTE_LIGHT;

#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct TileMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub atlas: Handle<Image>,
}

impl Material2d for TileMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/tile.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/tile.wgsl".into()
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
            ATTRIBUTE_LIGHT.at_shader_location(2),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

/// Shared material handle for all chunk entities
#[derive(Resource)]
pub struct SharedTileMaterial {
    pub handle: Handle<TileMaterial>,
}
```

> **Note for implementer:** The `specialize` method signature and types may differ in Bevy 0.18. Use ExternalScout to verify `Material2d::specialize`, `MeshVertexBufferLayoutRef`, and `at_shader_location` APIs. The critical requirement is that the vertex layout declares all 3 attributes at locations 0, 1, 2 matching the WGSL `@location` annotations.

**Step 3: Verify compilation**

```bash
cargo test && cargo clippy -- -D warnings
```

**Step 4: Visual test**

```bash
cargo run
```

The game should look identical to before (all tiles fully bright at `[255, 255, 255]`), but now the light data flows through the full GPU pipeline.

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(lighting): custom vertex shader passing RGB light to fragment"
```

---

## Task 5: Lighting Module — Sunlight + Point Lights

Create `src/world/lighting.rs` with sunlight propagation, point light BFS, and merge.

**Files:**
- Create: `src/world/lighting.rs`
- Modify: `src/world/mod.rs:1` (add `pub mod lighting;`)

**Step 1: Write unit tests first**

Create `src/world/lighting.rs` with tests at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::fixtures;
    use crate::world::chunk::WorldMap;

    #[test]
    fn merge_light_takes_max_per_channel() {
        assert_eq!(merge_light([100, 200, 50], [200, 100, 100]), [200, 200, 100]);
        assert_eq!(merge_light([0, 0, 0], [255, 255, 255]), [255, 255, 255]);
    }

    #[test]
    fn attenuate_clamps_to_zero() {
        assert_eq!(attenuate([100, 50, 10], 200), [0, 0, 0]);
        assert_eq!(attenuate([255, 255, 255], 0), [255, 255, 255]);
        assert_eq!(attenuate([100, 100, 100], 50), [50, 50, 50]);
    }

    #[test]
    fn sunlight_open_sky_chunk() {
        // Chunk at the top of the world, all air → full sunlight everywhere
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let mut map = WorldMap::default();

        // Generate a chunk near the top (all air above world height returns AIR)
        let top_chunk_y = wc.height_tiles / wc.chunk_size as i32 - 1;
        map.get_or_generate_chunk(0, top_chunk_y, &ctx);

        let sunlight = compute_chunk_sunlight(&map, 0, top_chunk_y, &ctx);

        // Top row of the chunk should have full (or near-full) sunlight
        let cs = wc.chunk_size;
        let top_row_idx = ((cs - 1) * cs) as usize; // first tile in top row
        // At minimum, the topmost air tiles should have SUN_COLOR
        // (exact values depend on terrain generation, but air tiles = full sun)
        for local_x in 0..cs {
            let idx = ((cs - 1) * cs + local_x) as usize;
            let tile_id = map.chunks[&(0, top_chunk_y)].tiles[idx];
            if tile_id == crate::registry::tile::TileId::AIR {
                assert_eq!(sunlight[idx], SUN_COLOR,
                    "Air tile at top of world should have full sunlight");
            }
        }
    }

    #[test]
    fn sunlight_blocked_by_solid() {
        // Manual test: create a chunk where top half is air, bottom half is stone
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let mut map = WorldMap::default();

        let cs = wc.chunk_size;
        let chunk_y = (wc.height_tiles / wc.chunk_size as i32) - 1;
        map.get_or_generate_chunk(0, chunk_y, &ctx);

        // Manually set: top half air, bottom half stone
        let stone_id = tr.by_name("stone");
        if let Some(chunk) = map.chunks.get_mut(&(0, chunk_y)) {
            for ly in 0..cs {
                for lx in 0..cs {
                    let idx = (ly * cs + lx) as usize;
                    if ly >= cs / 2 {
                        chunk.tiles[idx] = crate::registry::tile::TileId::AIR;
                    } else {
                        chunk.tiles[idx] = stone_id;
                    }
                }
            }
        }

        let sunlight = compute_chunk_sunlight(&map, 0, chunk_y, &ctx);

        // Air tiles in top half should have sunlight
        let air_idx = ((cs - 1) * cs) as usize;
        assert_eq!(sunlight[air_idx], SUN_COLOR);

        // Stone tile just below the air/stone boundary receives sunlight
        let boundary_idx = ((cs / 2 - 1) * cs) as usize;
        // But after passing through stone (opacity 15), next tile below is dark
        let below_boundary_idx = ((cs / 2 - 2) * cs) as usize;
        assert_eq!(sunlight[below_boundary_idx], [0, 0, 0],
            "Two tiles below boundary should be dark (stone fully absorbs)");
    }

    #[test]
    fn point_light_single_emitter() {
        // Test BFS with a single torch-like emitter in the center of an all-air chunk
        let (wc, bm, br, mut tr_defs, pc, nc) = {
            let br = fixtures::test_biome_registry();
            let bm = fixtures::test_biome_map(&br);
            (
                fixtures::test_world_config(),
                bm,
                br,
                vec![
                    crate::registry::tile::TileDef {
                        id: "air".into(), autotile: None, solid: false,
                        hardness: 0.0, friction: 0.0, viscosity: 0.0,
                        damage_on_contact: 0.0, effects: vec![],
                        light_emission: [0, 0, 0], light_opacity: 0,
                    },
                    crate::registry::tile::TileDef {
                        id: "torch".into(), autotile: None, solid: false,
                        hardness: 0.0, friction: 0.0, viscosity: 0.0,
                        damage_on_contact: 0.0, effects: vec![],
                        light_emission: [240, 180, 80], light_opacity: 0,
                    },
                ],
                fixtures::test_planet_config(),
                fixtures::test_noise_cache(),
            )
        };
        let tr = crate::registry::tile::TileRegistry::from_defs(tr_defs);
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let mut map = WorldMap::default();

        let cs = wc.chunk_size;
        let chunk_y = (wc.height_tiles / wc.chunk_size as i32) - 1;
        map.get_or_generate_chunk(0, chunk_y, &ctx);

        // Set all tiles to air, place torch at center
        let torch_id = tr.by_name("torch");
        if let Some(chunk) = map.chunks.get_mut(&(0, chunk_y)) {
            for i in 0..chunk.tiles.len() {
                chunk.tiles[i] = crate::registry::tile::TileId::AIR;
            }
            let center = (cs / 2 * cs + cs / 2) as usize;
            chunk.tiles[center] = torch_id;
        }

        let point = compute_point_lights(&map, 0, chunk_y, &ctx);

        // Torch position should have full emission
        let center = (cs / 2 * cs + cs / 2) as usize;
        assert_eq!(point[center], [240, 180, 80]);

        // Adjacent tile should have emission - LIGHT_FALLOFF
        let adjacent = (cs / 2 * cs + cs / 2 + 1) as usize;
        assert_eq!(point[adjacent][0], 240 - LIGHT_FALLOFF);
        assert_eq!(point[adjacent][1], 180 - LIGHT_FALLOFF);
        assert_eq!(point[adjacent][2], 80 - LIGHT_FALLOFF);

        // Far away tile (outside light radius) should be dark
        let far = 0usize; // corner (0,0)
        assert_eq!(point[far], [0, 0, 0]);
    }

    #[test]
    fn compute_lighting_merges_sun_and_point() {
        let a = vec![[200u8, 100, 50]; 4];
        let b = vec![[100u8, 200, 100]; 4];
        let merged = merge_chunk_lights(&a, &b);
        assert_eq!(merged[0], [200, 200, 100]);
    }
}
```

**Step 2: Implement the lighting module**

Full `src/world/lighting.rs`:

```rust
use std::collections::{HashMap, HashSet, VecDeque};

use crate::registry::tile::TileId;
use crate::world::chunk::{tile_to_chunk, WorldMap};
use crate::world::ctx::WorldCtxRef;

pub const SUN_COLOR: [u8; 3] = [255, 250, 230];
/// Base light attenuation per tile traversal (distance decay for point lights).
pub const LIGHT_FALLOFF: u8 = 17;
/// Multiplier for tile opacity → attenuation. opacity 15 × 17 = 255 (full block).
pub const OPACITY_SCALE: u16 = 17;
/// Maximum BFS radius for point lights and local recalculation.
pub const MAX_LIGHT_RADIUS: i32 = 16;

/// Subtract attenuation from a light value, clamping each channel to 0.
fn attenuate(light: [u8; 3], amount: u16) -> [u8; 3] {
    let a = amount.min(255) as u8;
    [
        light[0].saturating_sub(a),
        light[1].saturating_sub(a),
        light[2].saturating_sub(a),
    ]
}

/// Per-channel max of two light values.
pub fn merge_light(a: [u8; 3], b: [u8; 3]) -> [u8; 3] {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])]
}

fn is_dark(light: [u8; 3]) -> bool {
    light[0] == 0 && light[1] == 0 && light[2] == 0
}

/// Merge two chunk-sized light arrays per-channel max.
pub fn merge_chunk_lights(a: &[[u8; 3]], b: &[[u8; 3]]) -> Vec<[u8; 3]> {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| merge_light(*x, *y))
        .collect()
}

/// Compute sunlight for a chunk by scanning each column from top of world downward.
///
/// For each column: starts at SUN_COLOR, traces downward through all loaded tiles
/// above the chunk, then through the chunk itself. Solid tiles attenuate by
/// `opacity * OPACITY_SCALE`. Unloaded chunks above are treated as transparent.
pub fn compute_chunk_sunlight(
    world_map: &WorldMap,
    chunk_x: i32,
    chunk_y: i32,
    ctx: &WorldCtxRef,
) -> Vec<[u8; 3]> {
    let cs = ctx.config.chunk_size;
    let base_x = chunk_x * cs as i32;
    let base_y = chunk_y * cs as i32;
    let len = (cs * cs) as usize;
    let mut result = vec![[0u8; 3]; len];

    let chunk_data = match world_map.chunks.get(&(chunk_x, chunk_y)) {
        Some(data) => data,
        None => return result,
    };

    for local_x in 0..cs {
        let tile_x = base_x + local_x as i32;
        let mut light = SUN_COLOR;

        // Trace from top of world down to just above this chunk
        let chunk_top_y = base_y + cs as i32;
        for y in (chunk_top_y..ctx.config.height_tiles).rev() {
            if is_dark(light) {
                break;
            }
            if let Some(tile_id) = world_map.get_tile(tile_x, y, ctx) {
                let opacity = ctx.tile_registry.light_opacity(tile_id);
                light = attenuate(light, opacity as u16 * OPACITY_SCALE);
            }
            // None (unloaded chunk) → light passes through (conservative)
        }

        // Propagate through this chunk, top to bottom
        for local_y in (0..cs).rev() {
            let idx = (local_y * cs + local_x) as usize;
            // Tile receives current light level
            result[idx] = light;
            // Attenuate for next tile below
            let tile_id = chunk_data.tiles[idx];
            let opacity = ctx.tile_registry.light_opacity(tile_id);
            light = attenuate(light, opacity as u16 * OPACITY_SCALE);
        }
    }

    result
}

/// Compute point light contributions for a chunk via BFS flood-fill.
///
/// Scans for light-emitting tiles within MAX_LIGHT_RADIUS of the chunk,
/// then runs BFS from each emitter. Only writes results for tiles within
/// the target chunk.
pub fn compute_point_lights(
    world_map: &WorldMap,
    chunk_x: i32,
    chunk_y: i32,
    ctx: &WorldCtxRef,
) -> Vec<[u8; 3]> {
    let cs = ctx.config.chunk_size as i32;
    let base_x = chunk_x * cs;
    let base_y = chunk_y * cs;
    let len = (cs * cs) as usize;
    let mut result = vec![[0u8; 3]; len];

    // Scan area around chunk for emitters
    let scan_min_y = (base_y - MAX_LIGHT_RADIUS).max(0);
    let scan_max_y = (base_y + cs + MAX_LIGHT_RADIUS).min(ctx.config.height_tiles);

    for scan_y in scan_min_y..scan_max_y {
        for scan_dx in -MAX_LIGHT_RADIUS..(cs + MAX_LIGHT_RADIUS) {
            let scan_x = base_x + scan_dx;
            let wrapped_x = ctx.config.wrap_tile_x(scan_x);
            let Some(tile_id) = world_map.get_tile(wrapped_x, scan_y, ctx) else {
                continue;
            };
            let emission = ctx.tile_registry.light_emission(tile_id);
            if is_dark(emission) {
                continue;
            }

            bfs_from_emitter(
                world_map,
                scan_x,
                scan_y,
                emission,
                base_x,
                base_y,
                cs,
                ctx,
                &mut result,
            );
        }
    }

    result
}

#[allow(clippy::too_many_arguments)]
fn bfs_from_emitter(
    world_map: &WorldMap,
    start_x: i32,
    start_y: i32,
    emission: [u8; 3],
    target_base_x: i32,
    target_base_y: i32,
    chunk_size: i32,
    ctx: &WorldCtxRef,
    result: &mut [[u8; 3]],
) {
    let mut queue = VecDeque::new();
    let mut visited: HashMap<(i32, i32), [u8; 3]> = HashMap::new();

    queue.push_back((start_x, start_y, emission));

    while let Some((x, y, light)) = queue.pop_front() {
        if is_dark(light) {
            continue;
        }
        if (x - start_x).abs() > MAX_LIGHT_RADIUS || (y - start_y).abs() > MAX_LIGHT_RADIUS {
            continue;
        }

        let wrapped_x = ctx.config.wrap_tile_x(x);
        let key = (wrapped_x, y);

        let existing = visited.get(&key).copied().unwrap_or([0, 0, 0]);
        if light[0] <= existing[0] && light[1] <= existing[1] && light[2] <= existing[2] {
            continue;
        }

        let merged = merge_light(light, existing);
        visited.insert(key, merged);

        // Write to result if tile is within target chunk
        let lx = x - target_base_x;
        let ly = y - target_base_y;
        if lx >= 0 && lx < chunk_size && ly >= 0 && ly < chunk_size {
            let idx = (ly * chunk_size + lx) as usize;
            result[idx] = merge_light(result[idx], merged);
        }

        // Compute transmitted light (attenuated by this tile's opacity)
        let tile_id = world_map
            .get_tile(wrapped_x, y, ctx)
            .unwrap_or(TileId::AIR);
        let opacity = ctx.tile_registry.light_opacity(tile_id);
        let transmitted = attenuate(light, opacity as u16 * OPACITY_SCALE);
        if is_dark(transmitted) {
            continue;
        }

        // Spread to 4 neighbors with distance falloff
        let spread = attenuate(transmitted, LIGHT_FALLOFF as u16);
        if !is_dark(spread) {
            for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                if ny >= 0 && ny < ctx.config.height_tiles {
                    queue.push_back((nx, ny, spread));
                }
            }
        }
    }
}

/// Compute full lighting for a chunk (sunlight + point lights merged).
/// Returns the computed light_levels array. Caller writes it to ChunkData.
pub fn compute_chunk_lighting(
    world_map: &WorldMap,
    chunk_x: i32,
    chunk_y: i32,
    ctx: &WorldCtxRef,
) -> Vec<[u8; 3]> {
    let sunlight = compute_chunk_sunlight(world_map, chunk_x, chunk_y, ctx);
    let point_lights = compute_point_lights(world_map, chunk_x, chunk_y, ctx);
    merge_chunk_lights(&sunlight, &point_lights)
}

/// Recompute lighting for chunks affected by a tile change.
/// Recomputes a 3×3 chunk area around the changed tile.
/// Returns set of data-chunk coordinates whose light_levels were updated.
pub fn relight_around(
    world_map: &mut WorldMap,
    center_x: i32,
    center_y: i32,
    ctx: &WorldCtxRef,
) -> HashSet<(i32, i32)> {
    let wrapped_x = ctx.config.wrap_tile_x(center_x);
    let (center_cx, center_cy) = tile_to_chunk(wrapped_x, center_y, ctx.config.chunk_size);

    // Phase 1: Compute new light levels (immutable borrow)
    let updates: Vec<((i32, i32), Vec<[u8; 3]>)> = {
        let wm: &WorldMap = &*world_map;
        let mut results = Vec::new();
        for dy in -1..=1 {
            for dx in -1..=1 {
                let cy = center_cy + dy;
                if cy < 0 || cy >= ctx.config.height_chunks() {
                    continue;
                }
                let cx = ctx.config.wrap_chunk_x(center_cx + dx);
                if wm.chunks.contains_key(&(cx, cy)) {
                    let light = compute_chunk_lighting(wm, cx, cy, ctx);
                    results.push(((cx, cy), light));
                }
            }
        }
        results
    };

    // Phase 2: Write back (mutable borrow)
    let mut dirty = HashSet::new();
    for ((cx, cy), light) in updates {
        if let Some(chunk) = world_map.chunks.get_mut(&(cx, cy)) {
            chunk.light_levels = light;
            dirty.insert((cx, cy));
        }
    }
    dirty
}
```

**Step 3: Register module**

In `src/world/mod.rs`, add after line 1:

```rust
pub mod lighting;
```

**Step 4: Verify**

```bash
cargo test && cargo clippy -- -D warnings
```

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(lighting): sunlight propagation + point light BFS + merge"
```

---

## Task 6: Integration — Chunk Loading

Wire `compute_chunk_lighting` into `spawn_chunk` so newly loaded chunks get correct lighting. Change default light_levels to dark.

**Files:**
- Modify: `src/world/chunk.rs:66-82` (get_or_generate_chunk — change default to dark)
- Modify: `src/world/chunk.rs:240-299` (spawn_chunk — call compute_chunk_lighting)

**Step 1: Change default light_levels to dark**

In `src/world/chunk.rs:78`, change:

```rust
light_levels: vec![[0, 0, 0]; len],
```

The lighting system will fill in correct values.

**Step 2: Wire compute_chunk_lighting into spawn_chunk**

In `src/world/chunk.rs`, add import at top:

```rust
use crate::world::lighting;
```

In `spawn_chunk`, after `init_chunk_bitmasks` and before building the mesh, add lighting computation. The key is to:
1. Compute light levels (immutable borrow of world_map)
2. Write them to chunk data (mutable borrow)
3. Then build the mesh (immutable borrow of chunk data)

In `spawn_chunk` (around lines 260-267), after the bitmask assignment:

```rust
// Compute lighting
let light_levels = {
    let wm: &WorldMap = &*world_map;
    let ctx_ref_local = WorldCtxRef {
        config: ctx.config,
        biome_map: ctx.biome_map,
        biome_registry: ctx.biome_registry,
        tile_registry: ctx.tile_registry,
        planet_config: ctx.planet_config,
        noise_cache: ctx.noise_cache,
    };
    lighting::compute_chunk_lighting(wm, data_chunk_x, chunk_y, &ctx_ref_local)
};
if let Some(chunk) = world_map.chunks.get_mut(&(data_chunk_x, chunk_y)) {
    chunk.light_levels = light_levels;
}
```

> **Note for implementer:** The exact borrow dance depends on how `ctx` is structured. The goal is: compute with `&WorldMap`, write with `&mut WorldMap`. You may need to restructure the function slightly. If `ctx` is a reference that doesn't borrow `world_map`, you can simply do:
> ```rust
> let light = lighting::compute_chunk_lighting(&*world_map, data_chunk_x, chunk_y, ctx);
> world_map.chunks.get_mut(&(data_chunk_x, chunk_y)).unwrap().light_levels = light;
> ```

**Step 3: Verify**

```bash
cargo test && cargo clippy -- -D warnings
```

**Step 4: Visual test**

```bash
cargo run
```

You should now see:
- Surface tiles lit with warm sunlight color
- Underground tiles dark
- Light gradient at the air/solid boundary (from corner averaging)
- No point lights yet (no emitter tiles defined)

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(lighting): wire compute_chunk_lighting into chunk loading"
```

---

## Task 7: Integration — Tile Change Recalculation

Wire `relight_around` into `block_action.rs` so breaking/placing tiles updates lighting.

**Files:**
- Modify: `src/interaction/block_action.rs:1-10` (add imports)
- Modify: `src/interaction/block_action.rs:99-109` (add light recalc after bitmask update)

**Step 1: Add import**

In `src/interaction/block_action.rs`, add:

```rust
use crate::world::lighting;
```

**Step 2: Add light recalculation after tile change**

Replace the dirty chunk marking section (lines 99-109) with:

```rust
// Update bitmasks
let bitmask_dirty = update_bitmasks_around(&mut world_map, tile_x, tile_y, &ctx_ref);

// Recompute lighting for affected area
let light_dirty = lighting::relight_around(&mut world_map, tile_x, tile_y, &ctx_ref);

// Merge dirty sets and mark chunks for mesh rebuild
let all_dirty: HashSet<(i32, i32)> = bitmask_dirty.union(&light_dirty).copied().collect();

for (cx, cy) in all_dirty {
    for (&(display_cx, display_cy), &entity) in &loaded_chunks.map {
        if ctx_ref.config.wrap_chunk_x(display_cx) == cx && display_cy == cy {
            commands.entity(entity).insert(ChunkDirty);
        }
    }
}
```

Add `HashSet` import if not present:

```rust
use std::collections::HashSet;
```

**Step 3: Verify**

```bash
cargo test && cargo clippy -- -D warnings
```

**Step 4: Visual test**

```bash
cargo run
```

Test by:
1. Breaking a surface tile → light should now penetrate deeper at that column
2. Placing a tile on the surface → should block sunlight below

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(lighting): recompute lighting on tile break/place"
```

---

## Task 8: Final Verification + Cleanup

Full test suite, clippy, visual verification, and any needed adjustments.

**Step 1: Full test suite**

```bash
cargo test
```

Expected: all tests pass (previous 77 + new lighting tests).

**Step 2: Clippy**

```bash
cargo clippy -- -D warnings
```

Expected: no warnings.

**Step 3: Visual verification**

```bash
cargo run
```

Verify:
- [ ] Surface tiles illuminated with warm sunlight `[255, 250, 230]`
- [ ] Underground is dark (black)
- [ ] Light gradient is smooth at surface/underground boundary (corner averaging)
- [ ] Breaking surface tiles lets sunlight penetrate down
- [ ] Placing tiles blocks sunlight
- [ ] No visual glitches at chunk borders
- [ ] Performance is acceptable (no lag when loading chunks or breaking tiles)

**Step 4: Count remaining `#[allow(...)]`**

```bash
grep -r '#\[allow(clippy::too_many_arguments)\]' src/ | wc -l
```

If new `too_many_arguments` were added (e.g., `bfs_from_emitter`), verify they're justified.

**Step 5: Commit any cleanup**

```bash
git add -A && git commit -m "feat(lighting): final cleanup and verification"
```

---

## Summary

| Task | Description | Key files | Est. time |
|------|------------|-----------|-----------|
| 1 | TileDef light properties | tile.rs, tiles.registry.ron, test_helpers.rs | 15 min |
| 2 | ChunkData RGB light_levels | chunk.rs, mesh_builder.rs | 10 min |
| 3 | Mesh builder Float32x3 + corner averaging | mesh_builder.rs | 20 min |
| 4 | Custom shader + TileMaterial | tile.wgsl, tile_renderer.rs | 15 min |
| 5 | Lighting module (sunlight + BFS + merge) | lighting.rs (new), mod.rs | 30 min |
| 6 | Integration: chunk loading | chunk.rs | 15 min |
| 7 | Integration: tile change recalc | block_action.rs | 10 min |
| 8 | Final verification + cleanup | — | 10 min |
| **Total** | | | **~2 hours** |

## Dependencies

```
Task 1 (TileDef) ←── Task 5 (Lighting module needs opacity/emission accessors)
Task 2 (ChunkData RGB) ←── Task 3 (Mesh builder needs [u8; 3] input)
Task 3 (Mesh builder) ←── Task 4 (Shader needs Float32x3 attribute)
Task 4 (Shader) ←── Task 6 (Visual verification needs shader working)
Task 5 (Lighting module) ←── Task 6 (Integration calls compute_chunk_lighting)
Task 6 (Chunk loading) ←── Task 7 (Tile change builds on chunk loading pattern)
Task 7 (Tile change) ←── Task 8 (Final verification)
```

Linear execution: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8
