# Radiance Cascades 2D Lighting — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace BFS flood-fill lighting with GPU Radiance Cascades for screenspace 2D GI with bounce light.

**Architecture:** CPU extracts visible tiles into density/emissive/albedo textures each frame, GPU compute shaders run RC raymarch+merge across cascades, finalize pass produces lightmap, tile.wgsl samples lightmap by screen UV.

**Tech Stack:** Bevy 0.18, WGSL compute shaders, wgpu bind groups, Bevy render graph

---

### Task 1: Add albedo to tile registry

**Files:**
- Modify: `src/registry/tile.rs`
- Modify: `assets/world/tiles.registry.ron`
- Modify: `src/test_helpers.rs`

**Step 1: Add albedo field to TileDef**

In `src/registry/tile.rs`, add `albedo` field to `TileDef` and accessor to `TileRegistry`:

```rust
// In TileDef struct, after light_opacity:
#[serde(default = "default_albedo")]
pub albedo: [u8; 3],

// Add default function:
fn default_albedo() -> [u8; 3] {
    [128, 128, 128]
}

// In TileRegistry impl, add accessor:
pub fn albedo(&self, id: TileId) -> [u8; 3] {
    self.defs[id.0 as usize].albedo
}
```

**Step 2: Update tiles.registry.ron**

```ron
( id: "air",   ..., albedo: (0, 0, 0) ),
( id: "grass", ..., albedo: (34, 139, 34) ),
( id: "dirt",  ..., albedo: (139, 90, 43) ),
( id: "stone", ..., albedo: (128, 128, 128) ),
( id: "torch", ..., albedo: (200, 160, 80) ),
```

**Step 3: Update test_helpers.rs**

Add `albedo` field to all `TileDef` instances in `test_helpers::fixtures`.

**Step 4: Update tests in tile.rs**

Add `albedo` field to all `TileDef` instances in `tile.rs` tests. Add test:

```rust
#[test]
fn albedo_properties() {
    let reg = test_registry();
    assert_eq!(reg.albedo(TileId::AIR), [0, 0, 0]);
    assert_eq!(reg.albedo(TileId(3)), [128, 128, 128]); // stone
}
```

**Step 5: Run tests**

Run: `cargo test`
Expected: All tests pass.

**Step 6: Commit**

```bash
git add -A && git commit -m "feat: add albedo field to tile registry for bounce light"
```

---

### Task 2: Remove old lighting system

**Files:**
- Delete: `src/world/lighting.rs`
- Modify: `src/world/mod.rs` — remove `pub mod lighting`
- Modify: `src/world/chunk.rs` — remove `light_levels` from `ChunkData`, remove `use crate::world::lighting`, remove lighting calls from `spawn_chunk` and `rebuild_dirty_chunks`
- Modify: `src/world/mesh_builder.rs` — remove `ATTRIBUTE_LIGHT`, `corner_light`, `corner_shadow`, `get_light`, `lights` buffer, light params from `build_chunk_mesh`
- Modify: `src/world/tile_renderer.rs` — remove `ATTRIBUTE_LIGHT` from vertex layout
- Modify: `src/interaction/block_action.rs` — remove `use crate::world::lighting`, remove `relight_around` call and `light_dirty` variable
- Modify: `assets/shaders/tile.wgsl` — remove light attribute, output white light temporarily

**Step 1: Delete lighting.rs**

```bash
rm src/world/lighting.rs
```

**Step 2: Remove `pub mod lighting` from mod.rs**

In `src/world/mod.rs`, remove line:
```rust
pub mod lighting;
```

**Step 3: Clean up chunk.rs**

Remove from imports:
```rust
use crate::world::lighting;
```

Remove `light_levels` from `ChunkData`:
```rust
// Remove this field:
pub light_levels: Vec<[u8; 3]>,
```

Remove from `get_or_generate_chunk`:
```rust
// Remove:
light_levels: vec![[0, 0, 0]; len],
```

Remove from `spawn_chunk` (lines 347-351):
```rust
// Remove these lines:
let light_levels = lighting::compute_chunk_lighting(&*world_map, data_chunk_x, chunk_y, ctx);
if let Some(chunk) = world_map.chunks.get_mut(&(data_chunk_x, chunk_y)) {
    chunk.light_levels = light_levels;
}
```

Remove `light_levels` from `build_chunk_mesh` calls in `spawn_chunk` and `rebuild_dirty_chunks`:
```rust
// Remove this argument from both calls:
&chunk_data.light_levels,
```

**Step 4: Clean up mesh_builder.rs**

Remove `ATTRIBUTE_LIGHT` constant, `get_light`, `corner_light`, `corner_shadow` functions, `FG_SHADOW_DIM` constant.

Remove `lights` from `MeshBuildBuffers`:
```rust
// Remove:
pub lights: Vec<[f32; 3]>,
// And its capacity init and .clear()
```

Remove from `build_chunk_mesh` signature:
```rust
// Remove these params:
light_levels: &[[u8; 3]],
fg_tiles: Option<&[TileId]>,
```

Remove light computation and `buffers.lights.extend_from_slice` inside the tile loop.

Remove:
```rust
mesh.insert_attribute(ATTRIBUTE_LIGHT, buffers.lights.clone());
```

**Step 5: Clean up tile_renderer.rs**

Remove import:
```rust
use crate::world::mesh_builder::ATTRIBUTE_LIGHT;
```

Remove from `specialize`:
```rust
ATTRIBUTE_LIGHT.at_shader_location(2),
```

So vertex layout becomes:
```rust
let vertex_layout = layout.0.get_layout(&[
    Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
    Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
])?;
```

**Step 6: Clean up block_action.rs**

Remove:
```rust
use crate::world::lighting;
```

Remove lines 129-132:
```rust
// Remove:
let light_dirty = lighting::relight_around(&mut world_map, tile_x, tile_y, &ctx_ref);
let all_dirty: HashSet<(i32, i32)> = bitmask_dirty.union(&light_dirty).copied().collect();
```

Replace with:
```rust
let all_dirty = bitmask_dirty;
```

**Step 7: Update tile.wgsl — temporary full brightness**

```wgsl
#import bevy_sprite::mesh2d_functions as mesh_functions

struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vertex(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);
    out.clip_position = mesh_functions::mesh2d_position_local_to_clip(
        world_from_local,
        vec4<f32>(in.position, 1.0),
    );
    out.uv = in.uv;
    return out;
}

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
        if uniforms.dim < 1.0 {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
        discard;
    }
    // Temporary: full brightness until RC pipeline is connected
    return vec4<f32>(color.rgb * uniforms.dim, color.a);
}
```

**Step 8: Fix all tests**

Update mesh_builder tests to remove `light_levels` and `fg_tiles` params from `build_chunk_mesh` calls. Remove `lights` assertions. Remove lighting tests entirely (file deleted).

**Step 9: Run tests and build**

Run: `cargo test && cargo build`
Expected: All pass. Game runs with full brightness (no lighting).

**Step 10: Commit**

```bash
git add -A && git commit -m "refactor: remove old BFS lighting system

Clean slate for Radiance Cascades GPU lighting. Tiles render at full
brightness temporarily until RC pipeline is connected."
```

---

### Task 3: Create RC input extraction system (CPU side)

**Files:**
- Create: `src/world/rc_lighting.rs`
- Modify: `src/world/mod.rs` — add `pub mod rc_lighting`

**Step 1: Create rc_lighting.rs with plugin and resources**

```rust
use bevy::prelude::*;
use bevy::render::extract_resource::ExtractResource;

use crate::registry::tile::TileRegistry;
use crate::registry::world::WorldConfig;
use crate::sets::GameSet;
use crate::world::chunk::{world_to_tile, Layer, WorldMap};

/// Padding in tiles around viewport for RC upper cascades.
const RC_PADDING_TILES: i32 = 32;

/// Sun color emitted from the top edge when sky is visible.
const SUN_COLOR: [f32; 3] = [1.0, 0.98, 0.90];

/// Configuration for the RC lighting system, computed each frame.
#[derive(Resource, Clone, ExtractResource)]
pub struct RcLightingConfig {
    /// Size of the RC input textures in pixels (viewport + padding).
    pub input_size: UVec2,
    /// Size of the final lightmap (viewport only).
    pub viewport_size: UVec2,
    /// Offset from input texture origin to viewport origin (in pixels).
    pub viewport_offset: UVec2,
    /// Tile size in pixels.
    pub tile_size: f32,
    /// Number of cascades.
    pub cascade_count: u32,
}

/// Raw tile data extracted each frame for GPU upload.
#[derive(Resource, Clone, ExtractResource)]
pub struct RcInputData {
    /// Density map: 0 = air, 255 = solid. One byte per pixel.
    pub density: Vec<u8>,
    /// Emissive map: RGBA f16 packed as [f32; 4] per pixel.
    pub emissive: Vec<[f32; 4]>,
    /// Albedo map: RGB per pixel for bounce light.
    pub albedo: Vec<[u8; 4]>,
    /// Width of the input textures.
    pub width: u32,
    /// Height of the input textures.
    pub height: u32,
    /// Whether data was updated this frame.
    pub dirty: bool,
}

impl Default for RcInputData {
    fn default() -> Self {
        Self {
            density: Vec::new(),
            emissive: Vec::new(),
            albedo: Vec::new(),
            width: 0,
            height: 0,
            dirty: false,
        }
    }
}

impl Default for RcLightingConfig {
    fn default() -> Self {
        Self {
            input_size: UVec2::ZERO,
            viewport_size: UVec2::ZERO,
            viewport_offset: UVec2::ZERO,
            tile_size: 8.0,
            cascade_count: 5,
        }
    }
}

pub struct RcLightingPlugin;

impl Plugin for RcLightingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RcInputData>()
            .init_resource::<RcLightingConfig>()
            .add_systems(
                Update,
                extract_lighting_data.in_set(GameSet::WorldUpdate),
            );
    }
}

/// Each frame: read camera + visible tiles → fill density/emissive/albedo buffers.
fn extract_lighting_data(
    camera_query: Query<(&Camera, &GlobalTransform, &Projection), With<Camera2d>>,
    world_map: Res<WorldMap>,
    tile_registry: Res<TileRegistry>,
    world_config: Res<WorldConfig>,
    mut input: ResMut<RcInputData>,
    mut config: ResMut<RcLightingConfig>,
) {
    let Ok((camera, camera_gt, projection)) = camera_query.single() else {
        return;
    };

    // Get viewport size in pixels
    let viewport_size = camera
        .physical_viewport_size()
        .unwrap_or(UVec2::new(1280, 720));

    // Get camera scale from orthographic projection
    let scale = match projection {
        Projection::Orthographic(ortho) => ortho.scale,
        _ => 1.0,
    };

    // Viewport size in world units
    let viewport_world_w = viewport_size.x as f32 * scale;
    let viewport_world_h = viewport_size.y as f32 * scale;

    let camera_pos = camera_gt.translation().truncate();
    let tile_size = world_config.tile_size;

    // Visible tile range (with padding)
    let pad_world = RC_PADDING_TILES as f32 * tile_size;
    let left = camera_pos.x - viewport_world_w / 2.0 - pad_world;
    let bottom = camera_pos.y - viewport_world_h / 2.0 - pad_world;
    let right = camera_pos.x + viewport_world_w / 2.0 + pad_world;
    let top = camera_pos.y + viewport_world_h / 2.0 + pad_world;

    let (tile_left, tile_bottom) = world_to_tile(left, bottom, tile_size);
    let (tile_right, tile_top) = world_to_tile(right, top, tile_size);

    let tiles_w = (tile_right - tile_left + 1) as u32;
    let tiles_h = (tile_top - tile_bottom + 1) as u32;

    // Input texture size = tiles visible (1 pixel per tile for now, scaled to screen res later)
    let input_w = tiles_w;
    let input_h = tiles_h;
    let total = (input_w * input_h) as usize;

    // Viewport tiles (without padding)
    let vp_tile_left = ((camera_pos.x - viewport_world_w / 2.0) / tile_size).floor() as i32;
    let vp_tile_bottom = ((camera_pos.y - viewport_world_h / 2.0) / tile_size).floor() as i32;

    let vp_offset_x = (vp_tile_left - tile_left) as u32;
    let vp_offset_y = (vp_tile_bottom - tile_bottom) as u32;

    // Viewport size in tiles
    let vp_tiles_w = (viewport_world_w / tile_size).ceil() as u32;
    let vp_tiles_h = (viewport_world_h / tile_size).ceil() as u32;

    // Update config
    config.input_size = UVec2::new(input_w, input_h);
    config.viewport_size = UVec2::new(vp_tiles_w, vp_tiles_h);
    config.viewport_offset = UVec2::new(vp_offset_x, vp_offset_y);
    config.tile_size = tile_size;
    config.cascade_count = compute_cascade_count(input_w.max(input_h));

    // Resize buffers
    input.density.clear();
    input.density.resize(total, 0);
    input.emissive.clear();
    input.emissive.resize(total, [0.0; 4]);
    input.albedo.clear();
    input.albedo.resize(total, [0, 0, 0, 255]);
    input.width = input_w;
    input.height = input_h;

    // Fill buffers from world tiles
    for ty in tile_bottom..=tile_top {
        for tx in tile_left..=tile_right {
            let px = (tx - tile_left) as u32;
            let py = (ty - tile_bottom) as u32;
            let idx = (py * input_w + px) as usize;

            let wrapped_x = world_config.wrap_tile_x(tx);

            // Only fg layer participates in lighting
            let tile_id = match world_map.get_tile(wrapped_x, ty, Layer::Fg, 
                &crate::world::ctx::WorldCtxRef {
                    config: &world_config,
                    tile_registry: &tile_registry,
                    // We only need config + tile_registry for get_tile
                    biome_map: unsafe { &*std::ptr::null() }, // FIXME: need proper ctx
                    biome_registry: unsafe { &*std::ptr::null() },
                    player_config: unsafe { &*std::ptr::null() },
                    noise_cache: unsafe { &*std::ptr::null() },
                }) {
                Some(id) => id,
                None => continue, // unloaded chunk
            };

            // Density: solid = 255, air = 0
            if tile_registry.is_solid(tile_id) {
                input.density[idx] = 255;
            }

            // Emissive
            let emission = tile_registry.light_emission(tile_id);
            if emission != [0, 0, 0] {
                input.emissive[idx] = [
                    emission[0] as f32 / 255.0,
                    emission[1] as f32 / 255.0,
                    emission[2] as f32 / 255.0,
                    1.0,
                ];
            }

            // Albedo (for bounce light)
            let albedo = tile_registry.albedo(tile_id);
            input.albedo[idx] = [albedo[0], albedo[1], albedo[2], 255];
        }
    }

    // Sun: emit from top row if sky is visible
    let top_row_y = tile_top;
    for tx in tile_left..=tile_right {
        let px = (tx - tile_left) as u32;
        let idx = ((tiles_h - 1) * input_w + px) as usize;

        let wrapped_x = world_config.wrap_tile_x(tx);
        // Check if this column has open sky at the top
        if top_row_y >= world_config.height_tiles
            || world_map
                .get_tile(wrapped_x, top_row_y, Layer::Fg,
                    &crate::world::ctx::WorldCtxRef {
                        config: &world_config,
                        tile_registry: &tile_registry,
                        biome_map: unsafe { &*std::ptr::null() },
                        biome_registry: unsafe { &*std::ptr::null() },
                        player_config: unsafe { &*std::ptr::null() },
                        noise_cache: unsafe { &*std::ptr::null() },
                    })
                .is_some_and(|t| !tile_registry.is_solid(t))
        {
            input.emissive[idx] = [SUN_COLOR[0], SUN_COLOR[1], SUN_COLOR[2], 1.0];
        }
    }

    input.dirty = true;
}

/// Compute number of cascades needed for given max dimension.
fn compute_cascade_count(max_dim: u32) -> u32 {
    let mut count = 1u32;
    let mut size = 4u32; // cascade 0 interval length
    while size < max_dim {
        size *= 4;
        count += 1;
    }
    count.min(8) // cap at 8 cascades
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cascade_count_small() {
        assert_eq!(compute_cascade_count(16), 2);
        assert_eq!(compute_cascade_count(64), 3);
        assert_eq!(compute_cascade_count(256), 4);
        assert_eq!(compute_cascade_count(1024), 5);
        assert_eq!(compute_cascade_count(1920), 6);
    }
}
```

**Note:** The `WorldCtxRef` usage above is a placeholder. The actual implementation should use `WorldCtx` SystemParam or pass individual resources. The engineer should refactor `get_tile` to accept `(&WorldConfig, &TileRegistry)` directly, or use the existing `WorldCtx` pattern. The unsafe null pointers are NOT acceptable — this is a sketch showing the data flow. The real implementation must either:
1. Add a `get_tile_simple(&self, x, y, layer, config: &WorldConfig, registry: &TileRegistry)` method to WorldMap, or
2. Use the full `WorldCtx` SystemParam in the system signature.

**Step 2: Register in mod.rs**

Add to `src/world/mod.rs`:
```rust
pub mod rc_lighting;
```

And in `WorldPlugin::build`, add:
```rust
app.add_plugins(rc_lighting::RcLightingPlugin);
```

**Step 3: Run tests**

Run: `cargo test && cargo build`
Expected: Pass. RcInputData fills each frame but isn't used yet.

**Step 4: Commit**

```bash
git add -A && git commit -m "feat: add RC lighting input extraction system

CPU system reads visible tiles each frame and fills density/emissive/albedo
buffers for GPU upload. No GPU pipeline yet."
```

---

### Task 4: Write RC compute shaders (WGSL)

**Files:**
- Create: `assets/shaders/radiance_cascades.wgsl`
- Create: `assets/shaders/rc_finalize.wgsl`

**Step 1: Create radiance_cascades.wgsl**

```wgsl
// Radiance Cascades 2D — Raymarch + Merge compute shader
//
// Dispatched once per cascade, from highest to lowest.
// Each invocation processes one probe and all its directions.

struct RcUniforms {
    input_size: vec2<u32>,       // density/emissive texture dimensions
    cascade_index: u32,          // current cascade being processed
    cascade_count: u32,          // total number of cascades
    viewport_offset: vec2<u32>,  // offset from input to viewport
    viewport_size: vec2<u32>,    // viewport dimensions in pixels
    bounce_damping: f32,         // bounce light attenuation (0.3-0.5)
    _pad: f32,
}

@group(0) @binding(0) var<uniform> uniforms: RcUniforms;
@group(0) @binding(1) var density_map: texture_2d<f32>;
@group(0) @binding(2) var emissive_map: texture_2d<f32>;
@group(0) @binding(3) var albedo_map: texture_2d<f32>;
@group(0) @binding(4) var lightmap_prev: texture_2d<f32>;
@group(0) @binding(5) var cascade_read: texture_2d<f32>;   // upper cascade (N+1)
@group(0) @binding(6) var cascade_write: texture_storage_2d<rgba16float, write>;

const PI: f32 = 3.14159265359;
const BRANCHING: u32 = 4u;

// Number of directions for a given cascade level
fn num_directions(cascade: u32) -> u32 {
    return BRANCHING << (cascade * 2u); // 4, 16, 64, 256, ...
    // Actually: 4 * 4^cascade, but cascade 0 = 4 dirs
}

// Wait — standard RC: cascade 0 has fewest dirs, highest has most.
// Correction: cascade 0 = 4 dirs, cascade 1 = 16, etc.
// Probe spacing: cascade 0 = 1px, cascade 1 = 2px, cascade 2 = 4px, etc.

fn probe_spacing(cascade: u32) -> u32 {
    return 1u << cascade; // 1, 2, 4, 8, 16, ...
}

fn interval_start(cascade: u32) -> f32 {
    if cascade == 0u { return 0.0; }
    // Sum of previous intervals: 4^0 + 4^1 + ... but simplified:
    // interval_start = branching^cascade - 1 (geometric sum)
    // Actually for branching=4: starts at 0, 4, 16, 64, 256
    var start = 0.0;
    for (var i = 0u; i < cascade; i++) {
        start += f32(BRANCHING << (i * 2u)); // 4, 16, 64, ...
    }
    return start;
}

fn interval_length(cascade: u32) -> f32 {
    return f32(BRANCHING << (cascade * 2u)); // 4, 16, 64, 256, ...
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let cascade = uniforms.cascade_index;
    let spacing = probe_spacing(cascade);
    let n_dirs = num_directions(cascade);
    let int_start = interval_start(cascade);
    let int_len = interval_length(cascade);

    let input_size = vec2<f32>(uniforms.input_size);

    // Probe grid position
    let probe_x = gid.x;
    let probe_y = gid.y;
    let probes_w = uniforms.input_size.x / spacing;
    let probes_h = uniforms.input_size.y / spacing;

    if probe_x >= probes_w || probe_y >= probes_h {
        return;
    }

    // Probe center in input texture coordinates
    let probe_center = vec2<f32>(
        (f32(probe_x) + 0.5) * f32(spacing),
        (f32(probe_y) + 0.5) * f32(spacing),
    );

    // Process each direction for this probe
    let dirs_side = u32(sqrt(f32(n_dirs)));
    for (var dir_idx = 0u; dir_idx < n_dirs; dir_idx++) {
        let angle = (f32(dir_idx) + 0.5) / f32(n_dirs) * 2.0 * PI;
        let ray_dir = vec2<f32>(cos(angle), sin(angle));

        // Raymarch along this direction within the interval
        var radiance = vec3<f32>(0.0);
        var hit = false;

        let max_steps = u32(int_len) + 1u;
        for (var step = 0u; step < max_steps; step++) {
            let dist = int_start + f32(step);
            if dist >= int_start + int_len { break; }

            let sample_pos = probe_center + ray_dir * dist;
            let sample_px = vec2<i32>(sample_pos);

            // Bounds check
            if sample_px.x < 0 || sample_px.y < 0 ||
               sample_px.x >= i32(uniforms.input_size.x) ||
               sample_px.y >= i32(uniforms.input_size.y) {
                break;
            }

            let density = textureLoad(density_map, sample_px, 0).r;
            if density > 0.5 {
                // Hit solid surface
                let emissive = textureLoad(emissive_map, sample_px, 0).rgb;
                let albedo = textureLoad(albedo_map, sample_px, 0).rgb;

                // Bounce: read previous frame's light at hit point
                let prev_light = textureLoad(lightmap_prev, sample_px, 0).rgb;
                let reflected = prev_light * albedo * uniforms.bounce_damping;

                radiance = emissive + reflected;
                hit = true;
                break;
            }

            // Check emissive even in air (for sun edge emitters)
            let air_emissive = textureLoad(emissive_map, sample_px, 0).rgb;
            if air_emissive.r > 0.0 || air_emissive.g > 0.0 || air_emissive.b > 0.0 {
                radiance = air_emissive;
                hit = true;
                break;
            }
        }

        // If no hit and not the highest cascade, merge with upper cascade
        if !hit && cascade < uniforms.cascade_count - 1u {
            // Bilinear interpolation from upper cascade
            let upper_spacing = probe_spacing(cascade + 1u);
            let upper_probes_w = uniforms.input_size.x / upper_spacing;
            let upper_n_dirs = num_directions(cascade + 1u);

            // Find corresponding probe in upper cascade
            let upper_probe_f = probe_center / f32(upper_spacing) - 0.5;
            let upper_probe = vec2<i32>(upper_probe_f);

            // Find matching direction in upper cascade
            // Upper cascade has more directions, find the one closest to our angle
            let upper_dir_idx = dir_idx * (upper_n_dirs / n_dirs);

            // Simple nearest-probe lookup (bilinear TODO)
            let ux = clamp(upper_probe.x, 0, i32(upper_probes_w) - 1);
            let uy = clamp(upper_probe.y, 0, i32(uniforms.input_size.y / upper_spacing) - 1);

            // Read from upper cascade texture
            // Upper cascade is stored with directions packed into probe tiles
            let upper_dirs_side = u32(sqrt(f32(upper_n_dirs)));
            let dir_x = upper_dir_idx % upper_dirs_side;
            let dir_y = upper_dir_idx / upper_dirs_side;
            let read_x = u32(ux) * upper_dirs_side + dir_x;
            let read_y = u32(uy) * upper_dirs_side + dir_y;

            radiance = textureLoad(cascade_read, vec2<i32>(i32(read_x), i32(read_y)), 0).rgb;
        }

        // Write to cascade storage
        // Pack: probe position × directions grid
        let write_dirs_side = u32(sqrt(f32(n_dirs)));
        let write_dir_x = dir_idx % write_dirs_side;
        let write_dir_y = dir_idx / write_dirs_side;
        let write_x = probe_x * write_dirs_side + write_dir_x;
        let write_y = probe_y * write_dirs_side + write_dir_y;

        textureStore(cascade_write, vec2<i32>(i32(write_x), i32(write_y)),
                     vec4<f32>(radiance, 1.0));
    }
}
```

**Step 2: Create rc_finalize.wgsl**

```wgsl
// Extract irradiance from cascade 0 → final lightmap
//
// For each pixel, sum radiance from all directions in cascade 0,
// normalize, and write to lightmap.

struct FinalizeUniforms {
    input_size: vec2<u32>,
    viewport_offset: vec2<u32>,
    viewport_size: vec2<u32>,
    _pad: vec2<u32>,
}

@group(0) @binding(0) var<uniform> uniforms: FinalizeUniforms;
@group(0) @binding(1) var cascade_0: texture_2d<f32>;
@group(0) @binding(2) var lightmap_out: texture_storage_2d<rgba16float, write>;

const N_DIRS: u32 = 4u;  // cascade 0 has 4 directions
const DIRS_SIDE: u32 = 2u; // sqrt(4)

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let px = gid.x;
    let py = gid.y;

    if px >= uniforms.viewport_size.x || py >= uniforms.viewport_size.y {
        return;
    }

    // Map viewport pixel to input texture probe
    let input_x = px + uniforms.viewport_offset.x;
    let input_y = py + uniforms.viewport_offset.y;

    // Sum radiance from all 4 directions in cascade 0
    // Cascade 0: 1 probe per pixel, 4 directions packed as 2×2
    var total_radiance = vec3<f32>(0.0);
    for (var d = 0u; d < N_DIRS; d++) {
        let dir_x = d % DIRS_SIDE;
        let dir_y = d / DIRS_SIDE;
        let read_x = input_x * DIRS_SIDE + dir_x;
        let read_y = input_y * DIRS_SIDE + dir_y;
        total_radiance += textureLoad(cascade_0, vec2<i32>(i32(read_x), i32(read_y)), 0).rgb;
    }

    // Normalize: divide by number of directions
    let irradiance = total_radiance / f32(N_DIRS);

    textureStore(lightmap_out, vec2<i32>(i32(px), i32(py)), vec4<f32>(irradiance, 1.0));
}
```

**Step 3: Commit**

```bash
git add -A && git commit -m "feat: add Radiance Cascades WGSL compute shaders

Two shaders: radiance_cascades.wgsl (raymarch + merge per cascade)
and rc_finalize.wgsl (extract irradiance from cascade 0 to lightmap).
Includes bounce light via temporal feedback from previous frame."
```

---

### Task 5: Create RC render pipeline (GPU side)

**Files:**
- Create: `src/world/rc_pipeline.rs`
- Modify: `src/world/mod.rs` — add `pub mod rc_pipeline`
- Modify: `src/world/rc_lighting.rs` — register render-side systems

This is the most complex task. It involves:
1. Creating GPU textures for density/emissive/albedo/cascade/lightmap
2. Setting up compute pipeline with bind groups
3. Adding a render graph node that dispatches compute shaders
4. Uploading CPU data to GPU textures each frame
5. Double-buffering lightmap for temporal bounce

**This task requires deep Bevy render internals knowledge.** The engineer should:
- Study `examples/shader/compute_shader_game_of_life.rs` in Bevy repo
- Study how `bevy_flatland_radiance_cascades` sets up its compute pipeline
- Reference Bevy 0.18 `RenderApp`, `RenderGraph`, `Node` APIs

The implementation should follow the Game of Life compute shader example pattern:
1. `RcPipeline` resource in render world with cached pipeline IDs
2. `RcNode` implementing `render_graph::Node` for compute dispatches
3. `prepare_rc_textures` system to upload CPU data → GPU textures
4. Bind group creation matching the WGSL layout

**Key implementation details:**
- Use `TextureDescriptor` with `TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING`
- Lightmap double-buffer: two textures, swap handles each frame
- Cascade dispatch: loop from highest cascade to 0, rebinding cascade_read/cascade_write
- Finalize dispatch: once after all cascades

**Step 1: Implement rc_pipeline.rs** (engineer writes full implementation)

**Step 2: Register in mod.rs and rc_lighting.rs**

**Step 3: Run: `cargo build`**
Expected: Compiles. Lightmap texture created but not yet connected to tile shader.

**Step 4: Commit**

```bash
git add -A && git commit -m "feat: add RC GPU compute pipeline

Render graph node dispatches RC compute shaders each frame.
Creates and manages GPU textures for density/emissive/albedo/cascade/lightmap.
Double-buffered lightmap for temporal bounce light."
```

---

### Task 6: Connect lightmap to tile shader

**Files:**
- Modify: `src/world/tile_renderer.rs` — add lightmap texture binding to TileMaterial
- Modify: `assets/shaders/tile.wgsl` — sample lightmap instead of constant white
- Modify: `src/world/rc_lighting.rs` or `rc_pipeline.rs` — pass lightmap handle to SharedTileMaterial
- Modify: `src/registry/loading.rs` — update SharedTileMaterial creation

**Step 1: Update TileMaterial**

```rust
#[derive(Asset, AsBindGroup, Clone, TypePath)]
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

**Step 2: Update tile.wgsl**

```wgsl
#import bevy_sprite::mesh2d_functions as mesh_functions
#import bevy_sprite::mesh2d_view_bindings as view_bindings

struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vertex(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);
    out.clip_position = mesh_functions::mesh2d_position_local_to_clip(
        world_from_local,
        vec4<f32>(in.position, 1.0),
    );
    out.uv = in.uv;
    return out;
}

struct TileUniforms {
    dim: f32,
}

@group(2) @binding(0) var atlas_texture: texture_2d<f32>;
@group(2) @binding(1) var atlas_sampler: sampler;
@group(2) @binding(2) var<uniform> uniforms: TileUniforms;
@group(2) @binding(3) var lightmap_texture: texture_2d<f32>;
@group(2) @binding(4) var lightmap_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(atlas_texture, atlas_sampler, in.uv);
    if color.a < 0.01 {
        if uniforms.dim < 1.0 {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
        discard;
    }

    // Sample lightmap using screen UV
    let screen_uv = in.clip_position.xy / vec2<f32>(
        view_bindings::view.viewport.z,
        view_bindings::view.viewport.w
    );
    let light = textureSample(lightmap_texture, lightmap_sampler, screen_uv).rgb;

    return vec4<f32>(color.rgb * light * uniforms.dim, color.a);
}
```

**Step 3: Update SharedTileMaterial creation**

In `src/registry/loading.rs`, update material creation to include a default white lightmap texture (1×1 white pixel) that gets replaced by the RC pipeline output each frame.

**Step 4: Update lightmap handle each frame**

In `rc_pipeline.rs` or `rc_lighting.rs`, add a system that updates `SharedTileMaterial.fg.lightmap` and `SharedTileMaterial.bg.lightmap` handles to point to the current RC lightmap texture.

**Step 5: Run: `cargo run`**
Expected: Game runs with RC lighting! Tiles lit by sun from top, torches emit warm light, bounce light colors surfaces.

**Step 6: Commit**

```bash
git add -A && git commit -m "feat: connect RC lightmap to tile shader

Tile fragment shader samples lightmap by screen UV. Full RC pipeline
active: density/emissive extraction → compute cascades → lightmap → render."
```

---

### Task 7: Polish and tune

**Files:**
- Modify: `assets/shaders/radiance_cascades.wgsl` — tune parameters
- Modify: `src/world/rc_lighting.rs` — add debug controls
- Modify: `src/ui/debug_panel.rs` — add RC debug info

**Step 1: Add RC stats to debug panel**

Show in F3 panel:
- RC input size
- Number of cascades
- Lightmap size
- Frame time breakdown (if measurable)

**Step 2: Tune bounce damping**

Test values 0.3, 0.4, 0.5 for `BOUNCE_DAMPING`. Default to 0.4.

**Step 3: Test edge cases**

- Deep underground (no sun)
- Surface with mixed air/solid
- Multiple torches overlapping
- Window resize
- Camera at world wrap boundary

**Step 4: Commit**

```bash
git add -A && git commit -m "feat: polish RC lighting with debug panel and tuning"
```

---

## Task Dependencies

```
Task 1 (albedo) ──────────────────────────┐
                                           │
Task 2 (remove old) ──────────────────────┤
                                           ├── Task 5 (GPU pipeline) ── Task 6 (connect) ── Task 7 (polish)
Task 3 (CPU extract) ────────────────────┤
                                           │
Task 4 (WGSL shaders) ───────────────────┘
```

Tasks 1-4 can be done in parallel. Task 5 depends on all of them. Task 6 depends on 5. Task 7 depends on 6.
