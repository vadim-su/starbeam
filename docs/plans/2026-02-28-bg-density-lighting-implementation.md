# Background Density Lighting Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add background layer density to RC lighting so light passes through holes where both FG and BG are empty.

**Architecture:** Add a separate `density_bg` texture alongside existing `density` (FG). Shader combines both with `max(fg, bg)` during raymarching.

**Tech Stack:** Bevy 0.18, WGSL compute shaders, wgpu textures

---

## Task 1: Add density_bg to RcInputData

**Files:**
- Modify: `src/world/rc_lighting.rs:59-73`

**Step 1: Add density_bg field to RcInputData**

```rust
#[derive(Resource, Clone, Default, ExtractResource)]
pub struct RcInputData {
    /// 0 = air, 255 = solid. One byte per tile. (FG layer)
    pub density: Vec<u8>,
    /// 0 = air, 255 = solid. One byte per tile. (BG layer)
    pub density_bg: Vec<u8>,
    /// RGBA float per tile. Emissive light sources.
    pub emissive: Vec<[f32; 4]>,
    /// RGBA u8 per tile. Surface albedo for bounce light.
    pub albedo: Vec<[u8; 4]>,
    /// Width of the input grid in tiles.
    pub width: u32,
    /// Height of the input grid in tiles.
    pub height: u32,
    /// Whether buffers were updated this frame.
    pub dirty: bool,
}
```

**Step 2: Verify code compiles**

Run: `cargo check`
Expected: No errors

**Step 3: Commit**

```bash
git add src/world/rc_lighting.rs
git commit -m "feat(lighting): add density_bg field to RcInputData"
```

---

## Task 2: Add get_bg_tile helper function

**Files:**
- Modify: `src/world/rc_lighting.rs:119-141`

**Step 1: Add get_bg_tile function after get_fg_tile**

```rust
/// Look up a background tile without requiring `WorldCtxRef`.
/// Returns stone (bedrock) for `tile_y < 0`, `None` for above-world or unloaded chunks.
fn get_bg_tile(
    world_map: &WorldMap,
    tile_x: i32,
    tile_y: i32,
    world_config: &WorldConfig,
    tile_registry: &TileRegistry,
) -> Option<TileId> {
    if tile_y < 0 {
        return Some(tile_registry.by_name("stone")); // bedrock below world
    }
    if tile_y >= world_config.height_tiles {
        return None; // above world, treat as air
    }
    let wrapped_x = world_config.wrap_tile_x(tile_x);
    let (cx, cy) = tile_to_chunk(wrapped_x, tile_y, world_config.chunk_size);
    let (lx, ly) = tile_to_local(wrapped_x, tile_y, world_config.chunk_size);
    world_map
        .chunk(cx, cy)
        .map(|chunk| chunk.bg.get(lx, ly, world_config.chunk_size))
}
```

**Step 2: Verify code compiles**

Run: `cargo check`
Expected: No errors

**Step 3: Commit**

```bash
git add src/world/rc_lighting.rs
git commit -m "feat(lighting): add get_bg_tile helper function"
```

---

## Task 3: Resize and clear density_bg buffer

**Files:**
- Modify: `src/world/rc_lighting.rs:244-256`

**Step 1: Add density_bg resize in extract_lighting_data**

Find the resize block and add density_bg:

```rust
    // --- Resize buffers if needed ---
    if input.width != input_w || input.height != input_h {
        input.density.resize(total, 0);
        input.density_bg.resize(total, 0);  // NEW
        input.emissive.resize(total, [0.0; 4]);
        input.albedo.resize(total, [0, 0, 0, 0]);
        input.width = input_w;
        input.height = input_h;
    }

    // Clear buffers
    input.density.fill(0);
    input.density_bg.fill(0);  // NEW
    input.emissive.fill([0.0; 4]);
    input.albedo.fill([0, 0, 0, 0]);
```

**Step 2: Verify code compiles**

Run: `cargo check`
Expected: No errors

**Step 3: Commit**

```bash
git add src/world/rc_lighting.rs
git commit -m "feat(lighting): resize and clear density_bg buffer"
```

---

## Task 4: Fill density_bg in tile loop

**Files:**
- Modify: `src/world/rc_lighting.rs:258-293`

**Step 1: Add BG tile lookup and density_bg fill in the tile loop**

```rust
    // --- Fill tile data ---
    for ty in min_ty..=max_ty {
        for tx in min_tx..=max_tx {
            let buf_x = (tx - min_tx) as u32;
            // GPU textures have Y=0 at top; world Y increases upward.
            // Flip so that max_ty (top of world view) maps to texel row 0.
            let buf_y = (max_ty - ty) as u32;
            let idx = (buf_y * input_w + buf_x) as usize;

            // FG Density
            let Some(fg_tile_id) = get_fg_tile(&world_map, tx, ty, &world_config, &tile_registry)
            else {
                // Above world or unloaded chunk — leave as 0 (air)
                continue;
            };

            if tile_registry.is_solid(fg_tile_id) {
                input.density[idx] = 255;
            }

            // BG Density (NEW)
            if let Some(bg_tile_id) = get_bg_tile(&world_map, tx, ty, &world_config, &tile_registry)
            {
                if tile_registry.is_solid(bg_tile_id) {
                    input.density_bg[idx] = 255;
                }
            }

            // Emissive (FG only)
            let emission = tile_registry.light_emission(fg_tile_id);
            if emission != [0, 0, 0] {
                input.emissive[idx] = [
                    emission[0] as f32 / 255.0,
                    emission[1] as f32 / 255.0,
                    emission[2] as f32 / 255.0,
                    1.0,
                ];
            }

            // Albedo (FG only)
            let albedo = tile_registry.albedo(fg_tile_id);
            input.albedo[idx] = [albedo[0], albedo[1], albedo[2], 255];
        }
    }
```

**Step 2: Verify code compiles**

Run: `cargo check`
Expected: No errors

**Step 3: Commit**

```bash
git add src/world/rc_lighting.rs
git commit -m "feat(lighting): fill density_bg buffer in tile loop"
```

---

## Task 5: Add density_bg handle to RcGpuImages

**Files:**
- Modify: `src/world/rc_pipeline.rs:72-85`

**Step 1: Add density_bg field to RcGpuImages**

```rust
#[derive(Resource, Clone, ExtractResource)]
pub struct RcGpuImages {
    pub density: Handle<Image>,
    pub density_bg: Handle<Image>,   // NEW
    pub emissive: Handle<Image>,
    pub albedo: Handle<Image>,
    /// Double-buffer A for cascade storage.
    pub cascade_a: Handle<Image>,
    /// Double-buffer B for cascade storage.
    pub cascade_b: Handle<Image>,
    /// Current-frame lightmap output.
    pub lightmap: Handle<Image>,
    /// Previous-frame lightmap (for bounce light).
    pub lightmap_prev: Handle<Image>,
}
```

**Step 2: Verify code compiles**

Run: `cargo check`
Expected: No errors (may have unused warning)

**Step 3: Commit**

```bash
git add src/world/rc_pipeline.rs
git commit -m "feat(lighting): add density_bg handle to RcGpuImages"
```

---

## Task 6: Create density_bg texture in create_gpu_images

**Files:**
- Modify: `src/world/rc_pipeline.rs:710-721`

**Step 1: Add density_bg creation**

```rust
pub(crate) fn create_gpu_images(images: &mut Assets<Image>) -> RcGpuImages {
    let s = 64;
    RcGpuImages {
        density: make_gpu_texture(images, s, s, TextureFormat::R8Unorm),
        density_bg: make_gpu_texture(images, s, s, TextureFormat::R8Unorm),  // NEW
        emissive: make_gpu_texture(images, s, s, TextureFormat::Rgba16Float),
        albedo: make_gpu_texture(images, s, s, TextureFormat::Rgba8Unorm),
        cascade_a: make_gpu_texture(images, s * 2, s * 2, TextureFormat::Rgba16Float),
        cascade_b: make_gpu_texture(images, s * 2, s * 2, TextureFormat::Rgba16Float),
        lightmap: make_white_gpu_texture(images, s, s),
        lightmap_prev: make_white_gpu_texture(images, s, s),
    }
}
```

**Step 2: Verify code compiles**

Run: `cargo check`
Expected: No errors

**Step 3: Commit**

```bash
git add src/world/rc_pipeline.rs
git commit -m "feat(lighting): create density_bg texture in create_gpu_images"
```

---

## Task 7: Resize density_bg in resize_gpu_textures

**Files:**
- Modify: `src/world/rc_pipeline.rs:734-786`

**Step 1: Add density_bg recreation in resize_gpu_textures**

```rust
pub(crate) fn resize_gpu_textures(
    mut config: ResMut<RcLightingConfig>,
    mut gpu_images: ResMut<RcGpuImages>,
    mut images: ResMut<Assets<Image>>,
) {
    let input_w = config.input_size.x;
    let input_h = config.input_size.y;
    let vp_w = config.viewport_size.x;
    let vp_h = config.viewport_size.y;

    if input_w == 0 || input_h == 0 {
        return;
    }

    // Check if density texture needs resize by comparing with config
    let needs_resize = images.get(&gpu_images.density).is_none_or(|img| {
        img.texture_descriptor.size.width != input_w
            || img.texture_descriptor.size.height != input_h
    });

    if !needs_resize {
        return;
    }

    // Recreate input textures at new size
    gpu_images.density = make_gpu_texture(&mut images, input_w, input_h, TextureFormat::R8Unorm);
    gpu_images.density_bg = make_gpu_texture(&mut images, input_w, input_h, TextureFormat::R8Unorm);  // NEW
    gpu_images.emissive =
        make_gpu_texture(&mut images, input_w, input_h, TextureFormat::Rgba16Float);
    gpu_images.albedo = make_gpu_texture(&mut images, input_w, input_h, TextureFormat::Rgba8Unorm);

    // ... rest of function unchanged
}
```

**Step 2: Verify code compiles**

Run: `cargo check`
Expected: No errors

**Step 3: Commit**

```bash
git add src/world/rc_pipeline.rs
git commit -m "feat(lighting): resize density_bg in resize_gpu_textures"
```

---

## Task 8: Upload density_bg in prepare_rc_textures

**Files:**
- Modify: `src/world/rc_pipeline.rs:226-327`

**Step 1: Add density_bg upload after density upload in prepare_rc_textures**

Add this block after the density upload (around line 282):

```rust
    // Upload density_bg (R8Unorm — 1 byte per texel)
    if let Some(gpu_img) = gpu_images.get(&handles.density_bg) {
        let row_bytes = w;
        let (padded, aligned_bpr) = pad_rows(&input.density_bg, row_bytes, h);
        render_queue.write_texture(
            TexelCopyTextureInfo {
                texture: &gpu_img.texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &padded,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(aligned_bpr),
                rows_per_image: Some(h),
            },
            extent,
        );
    }
```

**Step 2: Verify code compiles**

Run: `cargo check`
Expected: No errors

**Step 3: Commit**

```bash
git add src/world/rc_pipeline.rs
git commit -m "feat(lighting): upload density_bg to GPU in prepare_rc_textures"
```

---

## Task 9: Add density_bg binding to cascade layout

**Files:**
- Modify: `src/world/rc_pipeline.rs:167-182`

**Step 1: Add density_bg to cascade bind group layout**

```rust
    let cascade_layout = BindGroupLayoutDescriptor::new(
        "rc_cascade_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                uniform_buffer::<RcUniformsGpu>(false), // @binding(0)
                texture_2d(TextureSampleType::Float { filterable: false }), // @binding(1) density
                texture_2d(TextureSampleType::Float { filterable: false }), // @binding(2) emissive
                texture_2d(TextureSampleType::Float { filterable: false }), // @binding(3) albedo
                texture_2d(TextureSampleType::Float { filterable: false }), // @binding(4) lightmap_prev
                texture_2d(TextureSampleType::Float { filterable: false }), // @binding(5) cascade_read
                texture_storage_2d(TextureFormat::Rgba16Float, StorageTextureAccess::WriteOnly), // @binding(6)
                texture_2d(TextureSampleType::Float { filterable: false }), // @binding(7) density_bg (NEW)
            ),
        ),
    );
```

**Step 2: Verify code compiles**

Run: `cargo check`
Expected: No errors

**Step 3: Commit**

```bash
git add src/world/rc_pipeline.rs
git commit -m "feat(lighting): add density_bg binding to cascade layout"
```

---

## Task 10: Bind density_bg in prepare_rc_bind_groups

**Files:**
- Modify: `src/world/rc_pipeline.rs:416-435` (resolve density_bg view)
- Modify: `src/world/rc_pipeline.rs:496-509` (add to bind group)

**Step 1: Resolve density_bg GPU image view**

Add to the tuple of resolved images:

```rust
    let (
        Some(density),
        Some(density_bg),  // NEW
        Some(emissive),
        Some(albedo),
        Some(cascade_a),
        Some(cascade_b),
        Some(lightmap),
        Some(lightmap_prev),
    ) = (
        gpu_images.get(&handles.density),
        gpu_images.get(&handles.density_bg),  // NEW
        gpu_images.get(&handles.emissive),
        gpu_images.get(&handles.albedo),
        gpu_images.get(&handles.cascade_a),
        gpu_images.get(&handles.cascade_b),
        gpu_images.get(&handles.lightmap),
        gpu_images.get(&handles.lightmap_prev),
    )
```

**Step 2: Add density_bg to cascade bind group creation**

```rust
        let bg = render_device.create_bind_group(
            "rc_cascade_bind_group",
            &cascade_layout,
            &BindGroupEntries::sequential((
                gpu_uniform.as_entire_binding(),
                &density.texture_view,
                &emissive.texture_view,
                &albedo.texture_view,
                &lightmap_prev.texture_view,
                cascade_read_view,
                write_tex,
                &density_bg.texture_view,  // NEW
            )),
        );
```

**Step 3: Verify code compiles**

Run: `cargo check`
Expected: No errors

**Step 4: Commit**

```bash
git add src/world/rc_pipeline.rs
git commit -m "feat(lighting): bind density_bg in prepare_rc_bind_groups"
```

---

## Task 11: Add density_bg binding in shader

**Files:**
- Modify: `assets/shaders/radiance_cascades.wgsl:27-33`

**Step 1: Add density_bg binding**

```wgsl
@group(0) @binding(0) var<uniform> uniforms: RcUniforms;
@group(0) @binding(1) var density_map: texture_2d<f32>;
@group(0) @binding(2) var emissive_map: texture_2d<f32>;
@group(0) @binding(3) var albedo_map: texture_2d<f32>;
@group(0) @binding(4) var lightmap_prev: texture_2d<f32>;
@group(0) @binding(5) var cascade_read: texture_2d<f32>;
@group(0) @binding(6) var cascade_write: texture_storage_2d<rgba16float, write>;
@group(0) @binding(7) var density_bg: texture_2d<f32>;  // NEW
```

**Step 2: Verify shader compiles (build check)**

Run: `cargo build`
Expected: No shader compilation errors

**Step 3: Commit**

```bash
git add assets/shaders/radiance_cascades.wgsl
git commit -m "feat(lighting): add density_bg binding in shader"
```

---

## Task 12: Combine densities in raymarch

**Files:**
- Modify: `assets/shaders/radiance_cascades.wgsl:188-189`

**Step 1: Combine FG and BG densities**

Replace:
```wgsl
            let density = textureLoad(density_map, sample_px, 0).r;

            if density > 0.5 {
```

With:
```wgsl
            let fg_density = textureLoad(density_map, sample_px, 0).r;
            let bg_density = textureLoad(density_bg, sample_px, 0).r;
            let density = max(fg_density, bg_density);

            if density > 0.5 {
```

**Step 2: Verify shader compiles and game runs**

Run: `cargo run`
Expected: Game starts, lighting works

**Step 3: Commit**

```bash
git add assets/shaders/radiance_cascades.wgsl
git commit -m "feat(lighting): combine FG and BG densities in raymarch"
```

---

## Task 13: Test and verify

**Step 1: Run the game**

Run: `cargo run`

**Step 2: Manual test**
1. Find or create a cave underground
2. Dig a hole through both FG and BG layers to the surface
3. Verify: Light enters through the hole and illuminates the cave interior
4. Verify: If only one layer is broken (FG or BG), light does NOT pass through

**Step 3: Run automated tests**

Run: `cargo test`
Expected: All tests pass

**Step 4: Final commit if any fixes needed**

```bash
git add -A
git commit -m "fix(lighting): verify bg density lighting works correctly"
```

---

## Summary

| Task | File | Description |
|------|------|-------------|
| 1 | rc_lighting.rs | Add density_bg field |
| 2 | rc_lighting.rs | Add get_bg_tile helper |
| 3 | rc_lighting.rs | Resize/clear density_bg |
| 4 | rc_lighting.rs | Fill density_bg in loop |
| 5 | rc_pipeline.rs | Add density_bg handle |
| 6 | rc_pipeline.rs | Create density_bg texture |
| 7 | rc_pipeline.rs | Resize density_bg |
| 8 | rc_pipeline.rs | Upload density_bg to GPU |
| 9 | rc_pipeline.rs | Add binding to layout |
| 10 | rc_pipeline.rs | Bind in bind groups |
| 11 | radiance_cascades.wgsl | Add shader binding |
| 12 | radiance_cascades.wgsl | Combine densities |
| 13 | - | Test and verify |
