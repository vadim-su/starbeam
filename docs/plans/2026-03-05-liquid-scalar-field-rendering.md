# Liquid Scalar Field Rendering Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the current blocky per-tile quads with a smooth metaball-like liquid renderer using a scalar field texture + bilinear interpolation + smoothstep threshold.

**Architecture:** Each frame, liquid levels for the visible area are uploaded into a small RGBA8 texture (1 px per tile, ~200×120 pixels). R = water level, G = lava level, B = oil level. The texture uses `FilterMode::Linear` for free GPU bilinear interpolation. A single full-screen quad renders this field through a threshold shader: `smoothstep(lo, hi, field)` per channel, producing smooth blob contours. An optional Gaussian blur pass between upload and render widens the merging radius. The quad is lit by the existing lightmap.

**Tech Stack:** Rust, Bevy 0.18, WGSL shaders, `Material2d`

---

## Context

### Current system (to be replaced)
- `src/liquid/render.rs` — `build_liquid_mesh()` creates per-chunk meshes where each liquid tile is a rectangular quad with height proportional to `level`
- `assets/engine/shaders/liquid.wgsl` — vertex-colored quads with lightmap sampling
- Per-chunk `LiquidMeshEntity` entities spawned in `src/world/chunk.rs:508-547`
- `DirtyLiquidChunks` resource triggers per-chunk mesh rebuilds

### Key constants
- `chunk_size = 32` tiles, `tile_size = 8.0` world-pixels
- `chunk_load_radius = 3` → loaded area up to 224×224 tiles
- Visible area at 1280×720: ~160×90 tiles
- Liquid types: water (ID=1), lava (ID=2), oil (ID=3)
- Z-layers: bg=-1.0, **liquid=-0.5**, fg=0.0

### Relevant files
- `src/liquid/render.rs` — current mesh builder + material
- `src/liquid/mod.rs` — plugin registration
- `src/world/chunk.rs` — chunk spawning/despawning (liquid entity creation)
- `src/world/rc_lighting.rs` — lightmap update code that touches liquid material
- `assets/engine/shaders/liquid.wgsl` — current shader

---

## Task 1: Create `LiquidFieldTexture` resource and upload system

**Files:**
- Modify: `src/liquid/render.rs`

**What:** A GPU texture (RGBA8, `FilterMode::Linear`) that stores per-tile liquid levels for the visible area. One pixel per tile. R=water, G=lava, B=oil, A=max(R,G,B) for quick combined threshold.

**Step 1: Add the resource struct and initialization**

In `src/liquid/render.rs`, add:

```rust
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// GPU texture holding per-tile liquid levels for the visible area.
/// R = water level, G = lava level, B = oil level, A = max(R,G,B).
/// Size: (width_tiles × height_tiles), 1 pixel per tile.
#[derive(Resource)]
pub struct LiquidFieldTexture {
    pub handle: Handle<Image>,
    /// CPU-side pixel buffer (RGBA8 row-major, bottom-to-top).
    pub pixels: Vec<u8>,
    /// Tile coordinate of the bottom-left corner of the texture.
    pub origin_tx: i32,
    pub origin_ty: i32,
    /// Texture dimensions in tiles.
    pub width: u32,
    pub height: u32,
}
```

Initialize it in `init_liquid_material` with a small default size (e.g. 1×1). It will be resized each frame.

**Step 2: Add the upload system**

New system `upload_liquid_field` that runs after `liquid_simulation_system` and before `rebuild_liquid_meshes` (or replaces it). Each frame:

1. Compute visible tile rect from camera position + window size + padding (4 tiles).
2. Resize texture if needed (realloc `pixels` vec, create new `Image`).
3. Clear pixel buffer to 0.
4. For each tile in the visible rect, read `LiquidCell` from `WorldMap`:
   - If `liquid_type == 1` (water): `pixels[i*4 + 0] = (level * 255.0) as u8`
   - If `liquid_type == 2` (lava): `pixels[i*4 + 1] = (level * 255.0) as u8`
   - If `liquid_type == 3` (oil): `pixels[i*4 + 2] = (level * 255.0) as u8`
   - A channel = max of R,G,B
5. Upload pixels to the GPU `Image` via `images.get_mut(handle)`.

```rust
pub fn upload_liquid_field(
    mut field: ResMut<LiquidFieldTexture>,
    mut images: ResMut<Assets<Image>>,
    world_map: Res<WorldMap>,
    config: Res<ActiveWorld>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
) {
    let Ok((camera, cam_transform)) = camera_q.single() else { return };

    // Compute visible tile rect from camera.
    let cam_pos = cam_transform.translation().xy();
    let Some(viewport_size) = camera.logical_viewport_size() else { return };
    // Scale factor from OrthographicProjection
    let half_w = viewport_size.x * 0.5;
    let half_h = viewport_size.y * 0.5;

    let padding = 4.0 * config.tile_size;
    let min_x = cam_pos.x - half_w - padding;
    let min_y = cam_pos.y - half_h - padding;
    let max_x = cam_pos.x + half_w + padding;
    let max_y = cam_pos.y + half_h + padding;

    let tx_min = (min_x / config.tile_size).floor() as i32;
    let ty_min = (min_y / config.tile_size).floor() as i32;
    let tx_max = (max_x / config.tile_size).ceil() as i32;
    let ty_max = (max_y / config.tile_size).ceil() as i32;

    let w = (tx_max - tx_min).max(1) as u32;
    let h = (ty_max - ty_min).max(1) as u32;

    // Resize if needed
    if w != field.width || h != field.height {
        field.width = w;
        field.height = h;
        field.pixels = vec![0u8; (w * h * 4) as usize];

        // Recreate GPU image
        let mut image = Image::new_fill(
            Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            TextureDimension::D2,
            &[0u8; 4],
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler = bevy::image::ImageSampler::Descriptor(
            bevy::image::ImageSamplerDescriptor {
                mag_filter: bevy::image::ImageFilterMode::Linear,
                min_filter: bevy::image::ImageFilterMode::Linear,
                address_mode_u: bevy::image::ImageAddressMode::ClampToEdge,
                address_mode_v: bevy::image::ImageAddressMode::ClampToEdge,
                ..default()
            },
        );
        field.handle = images.add(image);
    }

    field.origin_tx = tx_min;
    field.origin_ty = ty_min;

    // Clear
    field.pixels.fill(0);

    // Fill from world map
    for ty in ty_min..ty_max {
        for tx in tx_min..tx_max {
            let cell = get_liquid_cell(&world_map, tx, ty, &config);
            if cell.is_empty() { continue; }

            let px = (tx - tx_min) as u32;
            // Flip Y: texture row 0 = bottom
            let py = (ty - ty_min) as u32;
            let i = ((py * w + px) * 4) as usize;

            let level_byte = (cell.level.clamp(0.0, 1.0) * 255.0) as u8;
            let channel = match cell.liquid_type.0 {
                1 => 0, // water → R
                2 => 1, // lava → G
                3 => 2, // oil → B
                _ => 0,
            };
            field.pixels[i + channel] = level_byte;
            // A = max for combined threshold
            field.pixels[i + 3] = field.pixels[i + 3].max(level_byte);
        }
    }

    // Upload to GPU
    if let Some(image) = images.get_mut(&field.handle) {
        image.data = field.pixels.clone().into();
    }
}
```

Note: `get_liquid_cell` is a helper that wraps tile X and reads from WorldMap (similar to `get_liquid_from_map` in system.rs but pub).

**Step 3: Register the resource and system in `mod.rs`**

Add `LiquidFieldTexture` init and `upload_liquid_field` system to plugin.

**Step 4: Build and verify no compile errors**

Run: `cargo build`

**Step 5: Commit**

```
feat(liquid): add scalar field texture resource and upload system
```

---

## Task 2: New `LiquidFieldMaterial` and WGSL shader

**Files:**
- Modify: `src/liquid/render.rs` — add new material type
- Create: `assets/engine/shaders/liquid_field.wgsl` — new shader

**What:** A new `Material2d` that samples the scalar field texture, applies per-channel smoothstep threshold, composites liquid colors, and multiplies by lightmap.

**Step 1: Define the material**

```rust
/// Uniform data for the scalar field liquid shader.
#[derive(Clone, ShaderType)]
pub struct LiquidFieldUniforms {
    /// Colors for each liquid type: [water, lava, oil, unused].
    pub water_color: Vec4,
    pub lava_color: Vec4,
    pub oil_color: Vec4,
    /// Threshold and smoothing window for the metaball effect.
    pub threshold: f32,
    pub smoothing: f32,
    /// Padding
    pub _pad: Vec2,
}

#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct LiquidFieldMaterial {
    #[uniform(0)]
    pub uniforms: LiquidFieldUniforms,
    /// The scalar field texture (R=water, G=lava, B=oil).
    #[texture(1)]
    #[sampler(2)]
    pub field_texture: Handle<Image>,
    /// Lightmap
    #[texture(3)]
    #[sampler(4)]
    pub lightmap: Handle<Image>,
    #[uniform(5)]
    pub lightmap_uv_rect: Vec4,
}

impl Material2d for LiquidFieldMaterial {
    fn vertex_shader() -> ShaderRef {
        "engine/shaders/liquid_field.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "engine/shaders/liquid_field.wgsl".into()
    }
    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        // Alpha blending
        if let Some(target) = descriptor.fragment.as_mut()
            .and_then(|f| f.targets.get_mut(0))
            .and_then(|t| t.as_mut())
        {
            target.blend = Some(bevy::render::render_resource::BlendState::ALPHA_BLENDING);
        }
        Ok(())
    }
}
```

**Step 2: Write the WGSL shader**

`assets/engine/shaders/liquid_field.wgsl`:

```wgsl
#import bevy_sprite::mesh2d_functions as mesh_functions

struct LiquidFieldUniforms {
    water_color: vec4<f32>,
    lava_color: vec4<f32>,
    oil_color: vec4<f32>,
    threshold: f32,
    smoothing: f32,
    _pad: vec2<f32>,
};

struct LightmapXform {
    scale: vec2<f32>,
    offset: vec2<f32>,
};

@group(2) @binding(0) var<uniform> uniforms: LiquidFieldUniforms;
@group(2) @binding(1) var field_texture: texture_2d<f32>;
@group(2) @binding(2) var field_sampler: sampler;
@group(2) @binding(3) var lightmap_texture: texture_2d<f32>;
@group(2) @binding(4) var lightmap_sampler: sampler;
@group(2) @binding(5) var<uniform> lm_xform: LightmapXform;

struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world_pos: vec2<f32>,
};

@vertex
fn vertex(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);
    out.clip_position = mesh_functions::mesh2d_position_local_to_clip(
        world_from_local,
        vec4<f32>(in.position, 1.0),
    );
    out.uv = in.uv;
    out.world_pos = (world_from_local * vec4<f32>(in.position, 1.0)).xy;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let field = textureSample(field_texture, field_sampler, in.uv);

    let lo = uniforms.threshold - uniforms.smoothing;
    let hi = uniforms.threshold + uniforms.smoothing;

    // Per-liquid-type threshold
    let water_a = smoothstep(lo, hi, field.r);
    let lava_a  = smoothstep(lo, hi, field.g);
    let oil_a   = smoothstep(lo, hi, field.b);

    // Composite back-to-front: water (densest, bottom) → oil (lightest, top)
    var color = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    color = mix(color, uniforms.water_color, water_a);
    color = mix(color, uniforms.lava_color, lava_a);
    color = mix(color, uniforms.oil_color, oil_a);

    // Early discard for fully transparent pixels
    if color.a < 0.01 {
        discard;
    }

    // Apply lightmap
    let lm_uv = in.world_pos * lm_xform.scale + lm_xform.offset;
    let light = textureSample(lightmap_texture, lightmap_sampler, lm_uv).rgb;
    color = vec4<f32>(color.rgb * light, color.a);

    return color;
}
```

**Step 3: Register `Material2dPlugin::<LiquidFieldMaterial>` in `src/world/mod.rs`**

**Step 4: Build**

Run: `cargo build`

**Step 5: Commit**

```
feat(liquid): add scalar field material and threshold shader
```

---

## Task 3: Spawn and manage the liquid field quad entity

**Files:**
- Modify: `src/liquid/render.rs` — add quad management systems
- Modify: `src/liquid/mod.rs` — register systems

**What:** A single entity with a full-viewport quad mesh that uses `LiquidFieldMaterial`. Positioned at z=-0.5. The quad's size and UV mapping match the scalar field texture coverage.

**Step 1: Add marker component and shared material resource**

```rust
#[derive(Component)]
pub struct LiquidFieldQuad;

#[derive(Resource)]
pub struct SharedLiquidFieldMaterial(pub Handle<LiquidFieldMaterial>);
```

**Step 2: Init system — create material and spawn quad**

Extend `init_liquid_material` (or create a new init system) to:
1. Create `LiquidFieldMaterial` with default uniforms, fallback textures
2. Insert `SharedLiquidFieldMaterial` resource
3. Spawn a quad entity with `LiquidFieldQuad` marker

**Step 3: Update system — resize quad and update material each frame**

New system `update_liquid_field_quad` running after `upload_liquid_field`:

```rust
pub fn update_liquid_field_quad(
    field: Res<LiquidFieldTexture>,
    config: Res<ActiveWorld>,
    liquid_registry: Res<LiquidRegistry>,
    shared_mat: Res<SharedLiquidFieldMaterial>,
    mut materials: ResMut<Assets<LiquidFieldMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut quad_q: Query<(&mut Mesh2d, &mut Transform), With<LiquidFieldQuad>>,
) {
    let Some(mat) = materials.get_mut(&shared_mat.0) else { return };

    // Update field texture handle
    mat.field_texture = field.handle.clone();

    // Update liquid colors from registry
    if let Some(water) = liquid_registry.get(LiquidId(1)) {
        mat.uniforms.water_color = Vec4::from(water.color);
    }
    if let Some(lava) = liquid_registry.get(LiquidId(2)) {
        mat.uniforms.lava_color = Vec4::from(lava.color);
    }
    if let Some(oil) = liquid_registry.get(LiquidId(3)) {
        mat.uniforms.oil_color = Vec4::from(oil.color);
    }

    // Rebuild quad mesh to match field coverage
    let world_w = field.width as f32 * config.tile_size;
    let world_h = field.height as f32 * config.tile_size;
    let origin_x = field.origin_tx as f32 * config.tile_size;
    let origin_y = field.origin_ty as f32 * config.tile_size;

    let quad = Mesh::from(Rectangle::new(world_w, world_h));
    // Need to set UVs correctly: (0,0) at bottom-left, (1,1) at top-right

    for (mut mesh_handle, mut transform) in &mut quad_q {
        *mesh_handle = Mesh2d(meshes.add(quad.clone()));
        transform.translation = Vec3::new(
            origin_x + world_w * 0.5,
            origin_y + world_h * 0.5,
            -0.5,
        );
    }
}
```

Note: Bevy's `Rectangle` mesh has UVs (0,0) at bottom-left which matches our texture layout.

**Step 4: Build and run**

Run: `cargo build && cargo run`

Visually verify: liquid should appear as smooth blobs instead of blocky rectangles.

**Step 5: Commit**

```
feat(liquid): spawn scalar field quad entity with auto-resize
```

---

## Task 4: Remove old per-chunk liquid mesh system

**Files:**
- Modify: `src/world/chunk.rs` — remove liquid mesh spawning from `spawn_chunk`
- Modify: `src/liquid/render.rs` — remove `build_liquid_mesh`, `rebuild_liquid_meshes`, old `LiquidMaterial`
- Modify: `src/liquid/mod.rs` — unregister old systems
- Modify: `src/world/mod.rs` — remove `Material2dPlugin::<LiquidMaterial>` if replaced
- Modify: `src/world/rc_lighting.rs` — update lightmap code to use new material type
- Delete: `assets/engine/shaders/liquid.wgsl` (old shader)

**Step 1: Remove per-chunk liquid entity creation from `spawn_chunk`**

In `src/world/chunk.rs`, the liquid mesh entity spawning (lines 508-547) should be simplified. The `ChunkEntities.liquid` field can be removed or set to `Entity::PLACEHOLDER`.

Actually, keep a minimal placeholder entity for compatibility, or remove `liquid` from `ChunkEntities` entirely and update `despawn_chunk`.

**Step 2: Remove old mesh builder and material**

Remove from `render.rs`:
- `build_liquid_mesh()` function
- `rebuild_liquid_meshes()` system
- `LiquidMaterial` struct (replaced by `LiquidFieldMaterial`)
- `SharedLiquidMaterial` (replaced by `SharedLiquidFieldMaterial`)
- Old tests that test `build_liquid_mesh`

**Step 3: Update lightmap integration**

In `src/world/rc_lighting.rs`, the `update_tile_lightmap` function updates `LiquidMaterial` with lightmap data. Change it to update `LiquidFieldMaterial` instead:

```rust
// Before: liquid_materials: ResMut<Assets<LiquidMaterial>>
// After:  liquid_materials: ResMut<Assets<LiquidFieldMaterial>>
```

**Step 4: Build and test**

Run: `cargo build && cargo test`

All 266 tests should pass (minus removed liquid mesh tests, plus any new tests).

**Step 5: Commit**

```
refactor(liquid): remove old per-chunk liquid mesh system
```

---

## Task 5: Add optional blur pass

**Files:**
- Modify: `src/liquid/render.rs` — add blur system
- Modify: `src/liquid/mod.rs` — register blur system

**What:** A CPU-side box blur on the pixel buffer before GPU upload. Simple, no extra shaders needed. Runs between `upload_liquid_field` (fill pixels) and the GPU upload step. Split upload into fill + blur + upload.

**Step 1: Implement CPU blur**

```rust
/// Apply a simple box blur to the liquid field pixel buffer.
/// Operates on each RGBA channel independently.
/// `radius` is in pixels (tiles). 1 = 3×3 kernel.
fn blur_liquid_field(pixels: &mut [u8], width: u32, height: u32, radius: u32) {
    if radius == 0 || width == 0 || height == 0 { return; }

    let w = width as usize;
    let h = height as usize;
    let mut temp = vec![0u8; pixels.len()];

    // Horizontal pass
    for y in 0..h {
        for x in 0..w {
            let mut sums = [0u32; 4];
            let mut count = 0u32;
            let x_lo = x.saturating_sub(radius as usize);
            let x_hi = (x + radius as usize + 1).min(w);
            for sx in x_lo..x_hi {
                let si = (y * w + sx) * 4;
                for c in 0..4 {
                    sums[c] += pixels[si + c] as u32;
                }
                count += 1;
            }
            let di = (y * w + x) * 4;
            for c in 0..4 {
                temp[di + c] = (sums[c] / count) as u8;
            }
        }
    }

    // Vertical pass
    for y in 0..h {
        for x in 0..w {
            let mut sums = [0u32; 4];
            let mut count = 0u32;
            let y_lo = y.saturating_sub(radius as usize);
            let y_hi = (y + radius as usize + 1).min(h);
            for sy in y_lo..y_hi {
                let si = (sy * w + x) * 4;
                for c in 0..4 {
                    sums[c] += temp[si + c] as u32;
                }
                count += 1;
            }
            let di = (y * w + x) * 4;
            for c in 0..4 {
                pixels[di + c] = (sums[c] / count) as u8;
            }
        }
    }
}
```

Call this in `upload_liquid_field` after filling pixels, before GPU upload:

```rust
blur_liquid_field(&mut field.pixels, field.width, field.height, 1);
```

**Step 2: Make blur radius configurable**

Add `blur_radius: u32` to `LiquidFieldTexture` or a separate config resource. Default = 1 (3×3). Can be toggled via debug panel (F8).

**Step 3: Build and run**

Run: `cargo build && cargo run`

Visually verify: blobs should merge more aggressively, small gaps disappear.

**Step 4: Commit**

```
feat(liquid): add CPU box blur pass for smoother metaball merging
```

---

## Task 6: Tune visual parameters and integrate debug controls

**Files:**
- Modify: `src/liquid/debug.rs` — add threshold/smoothing/blur sliders to egui panel
- Modify: `src/liquid/render.rs` — expose tuning params

**What:** Add sliders to the F8 debug panel for:
- `threshold` (0.0–1.0, default 0.4)
- `smoothing` (0.0–0.5, default 0.1)
- `blur_radius` (0–3, default 1)

**Step 1: Add tunables resource**

```rust
#[derive(Resource)]
pub struct LiquidRenderConfig {
    pub threshold: f32,   // default 0.4
    pub smoothing: f32,   // default 0.1
    pub blur_radius: u32, // default 1
}

impl Default for LiquidRenderConfig {
    fn default() -> Self {
        Self {
            threshold: 0.4,
            smoothing: 0.1,
            blur_radius: 1,
        }
    }
}
```

**Step 2: Wire into upload and material update systems**

`upload_liquid_field` reads `blur_radius` from config.
`update_liquid_field_quad` writes `threshold` and `smoothing` to material uniforms.

**Step 3: Add egui sliders in debug panel**

In `draw_liquid_debug_panel`, add:

```rust
ui.add(egui::Slider::new(&mut config.threshold, 0.0..=1.0).text("Threshold"));
ui.add(egui::Slider::new(&mut config.smoothing, 0.0..=0.5).text("Smoothing"));
ui.add(egui::Slider::new(&mut config.blur_radius, 0..=3).text("Blur radius"));
```

**Step 4: Build and run, tune values**

Run: `cargo build && cargo run`

Experiment with threshold/smoothing/blur to find the best ONI-like look.

**Step 5: Commit**

```
feat(liquid): add configurable render params with debug sliders
```

---

## Task 7: Final cleanup and test verification

**Files:**
- Modify: `src/liquid/render.rs` — clean up unused code, add doc comments
- Run: `cargo test` — all tests pass
- Run: `cargo clippy` — no warnings

**Step 1: Remove any remaining dead code from old liquid mesh system**

**Step 2: Update/add tests**

- Test that `LiquidFieldTexture` pixel writing works correctly (unit test: write a cell, check pixel buffer)
- Test that blur produces expected smoothing (unit test: sharp edge → blurred)

**Step 3: Build, test, clippy**

Run: `cargo build && cargo test && cargo clippy`

**Step 4: Commit**

```
chore(liquid): cleanup old code, add scalar field tests
```

---

## Summary of execution order

| Task | Description | Depends on |
|------|-------------|------------|
| 1 | Scalar field texture + upload | — |
| 2 | New material + WGSL shader | — |
| 3 | Quad entity management | 1, 2 |
| 4 | Remove old mesh system | 3 (verified working) |
| 5 | Blur pass | 1 |
| 6 | Debug sliders | 3, 5 |
| 7 | Cleanup + tests | all |

Tasks 1 and 2 can be done in parallel. Task 3 integrates them. Task 4 removes the old system only after the new one is visually verified. Task 5 is an enhancement that can come after initial visual verification.

## Key visual tuning notes

- **threshold = 0.4**: levels above 0.4 (after interpolation) become visible liquid. This means a tile at level=0.5 is fully visible, while at level=0.3 it's partially transparent at edges.
- **smoothing = 0.1**: creates a soft edge band of ±0.1 around the threshold.
- **blur_radius = 1**: a 3×3 box blur means each tile's level spreads to its 8 neighbors, making blobs merge within 1-tile gaps.
- Higher blur = more merging but also more "bloat" (liquid appears larger than its actual volume). Threshold compensates.
