# Fluid Shader Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace flat ColorMaterial fluid rendering with a custom FluidMaterial + WGSL shader providing Starbound-style visual effects (wavy surface, depth darkening, shimmer, lightmap, glow).

**Architecture:** Custom `FluidMaterial` (Material2d) with WGSL shader `fluid.wgsl`. Mesh builder enriched with UV_0 and a custom `FLUID_DATA` vertex attribute. Lightmap integrated via `update_tile_lightmap()`.

**Tech Stack:** Bevy 0.18, WGSL, Material2d + AsBindGroup, MeshVertexAttribute

---

### Task 1: Define FluidMaterial and register Material2dPlugin

**Files:**
- Modify: `src/fluid/systems.rs` — replace `SharedFluidMaterial` type, add `FluidMaterial` struct
- Modify: `src/fluid/mod.rs` — register `Material2dPlugin::<FluidMaterial>`, update `init_fluid_material`

**Step 1: Define FluidMaterial in systems.rs**

Add to top of `src/fluid/systems.rs`:

```rust
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dKey, Material2dPlugin};

use crate::fluid::render::ATTRIBUTE_FLUID_DATA;
```

Replace `SharedFluidMaterial`:

```rust
#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct FluidMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub lightmap: Handle<Image>,
    #[uniform(2)]
    pub lightmap_uv_rect: Vec4,
    #[uniform(3)]
    pub time: f32,
}

impl Material2d for FluidMaterial {
    fn vertex_shader() -> ShaderRef {
        "engine/shaders/fluid.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "engine/shaders/fluid.wgsl".into()
    }
    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_COLOR.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
            ATTRIBUTE_FLUID_DATA.at_shader_location(3),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

#[derive(Resource)]
pub struct SharedFluidMaterial {
    pub handle: Handle<FluidMaterial>,
}
```

**Step 2: Update init_fluid_material in mod.rs**

Change `init_fluid_material` to create `FluidMaterial` with a fallback lightmap:

```rust
fn init_fluid_material(
    mut commands: Commands,
    mut fluid_materials: ResMut<Assets<FluidMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let white_lightmap = images.add(Image::new_fill(
        Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[0x00u8, 0x3C, 0x00, 0x3C, 0x00, 0x3C, 0x00, 0x3C],
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    ));
    commands.insert_resource(systems::SharedFluidMaterial {
        handle: fluid_materials.add(FluidMaterial {
            lightmap: white_lightmap,
            lightmap_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
            time: 0.0,
        }),
    });
}
```

Register plugin in `impl Plugin for FluidPlugin`:
```rust
app.add_plugins(Material2dPlugin::<systems::FluidMaterial>::default())
```

Also remove `use bevy::sprite_render::MeshMaterial2d;` from systems.rs imports and add it via the new import block. Update `fluid_rebuild_meshes` to use `MeshMaterial2d<FluidMaterial>` instead of `MeshMaterial2d<ColorMaterial>`.

**Step 3: Build and fix compile errors**

Run: `cargo build 2>&1 | head -40`

The shader file doesn't exist yet, so the game won't render fluids, but it should compile.

**Step 4: Commit**

```
feat(fluid): define FluidMaterial with lightmap and time bindings
```

---

### Task 2: Add custom vertex attribute and enrich mesh builder

**Files:**
- Modify: `src/fluid/render.rs` — add ATTRIBUTE_FLUID_DATA, UV_0, surface/depth detection

**Step 1: Define the custom vertex attribute**

Add at top of `render.rs`:

```rust
use bevy::mesh::MeshVertexAttribute;
use bevy::render::render_resource::VertexFormat;

/// Custom vertex attribute for fluid shader data.
/// xyz = emission RGB (0..1), w = flags (is_wave_vertex + is_gas*2)
pub const ATTRIBUTE_FLUID_DATA: MeshVertexAttribute =
    MeshVertexAttribute::new("FluidData", 982301567, VertexFormat::Float32x4);
```

**Step 2: Rewrite build_fluid_mesh to emit UV_0 and FLUID_DATA**

The function signature stays the same but now also needs access to the chunk's fluid array for surface/depth detection. Add a helper `is_cell_empty_or_oob` and update the function:

New parameters: the fluids array is already passed. For surface detection we need to check neighbors.

For each non-empty cell at (local_x, local_y):
- `is_gas` = def.is_gas
- Surface detection:
  - liquid: is_surface if cell at (local_x, local_y+1) is empty/oob or different fluid
  - gas: is_surface if cell at (local_x, local_y-1) is empty/oob or different fluid
- Depth: scan from cell toward surface, count non-empty same-fluid cells, normalize by max_depth (16)
- Emission: `[def.light_emission[0]/255.0, def.light_emission[1]/255.0, def.light_emission[2]/255.0]`
- UV per vertex: `(fill, depth_normalized)` — same for all 4 vertices
- FLUID_DATA per vertex:
  - For top 2 vertices (indices 2,3) of liquid surface: flags = 1.0 (is_wave)
  - For bottom 2 vertices (indices 0,1) of gas surface: flags = 1.0 + 2.0 = 3.0
  - For non-surface: flags = is_gas * 2.0
  - Non-wave gas vertices: flags = 2.0

Emit:
```rust
mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);       // Vec<[f32; 2]>
mesh.insert_attribute(ATTRIBUTE_FLUID_DATA, fluid_data); // Vec<[f32; 4]>
```

**Step 3: Update existing tests**

Tests that check mesh attributes need updating — they now expect UV_0 and FLUID_DATA.
Add new tests:
- `surface_cell_has_wave_flag` — liquid with empty cell above has is_wave on top vertices
- `deep_cell_has_higher_depth` — cell 3 below surface has depth > cell 1 below
- `lava_has_emission_data` — fluid with light_emission has non-zero emission in FLUID_DATA

**Step 4: Run tests**

Run: `cargo test --package starbeam fluid::render`

**Step 5: Commit**

```
feat(fluid): enrich fluid mesh with UV and FLUID_DATA vertex attributes
```

---

### Task 3: Write the WGSL shader

**Files:**
- Create: `assets/engine/shaders/fluid.wgsl`

**Step 1: Write the shader**

```wgsl
#import bevy_sprite::mesh2d_functions as mesh_functions
#import bevy_sprite::mesh2d_view_bindings::view

struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,         // (fill_level, depth_in_fluid)
    @location(3) fluid_data: vec4<f32>,  // (emission_r, emission_g, emission_b, flags)
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) fluid_data: vec4<f32>,
    @location(3) world_pos: vec2<f32>,
}

struct FluidUniforms {
    scale: vec2<f32>,
    offset: vec2<f32>,
}

@group(2) @binding(0) var lightmap_texture: texture_2d<f32>;
@group(2) @binding(1) var lightmap_sampler: sampler;
@group(2) @binding(2) var<uniform> lm_xform: FluidUniforms;
@group(2) @binding(3) var<uniform> time: f32;

@vertex
fn vertex(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);

    var pos = in.position;

    // Wave displacement for surface vertices
    let flags = in.fluid_data.w;
    let is_wave = (flags % 2.0) >= 0.5;  // bit 0
    if is_wave {
        let world_x = (world_from_local * vec4<f32>(pos, 1.0)).x;
        pos.y += sin(world_x * 3.0 + time * 2.0) * 0.12;
    }

    out.clip_position = mesh_functions::mesh2d_position_local_to_clip(
        world_from_local,
        vec4<f32>(pos, 1.0),
    );
    out.world_pos = (world_from_local * vec4<f32>(pos, 1.0)).xy;
    out.color = in.color;
    out.uv = in.uv;
    out.fluid_data = in.fluid_data;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = in.color;
    let depth = in.uv.y;  // 0 = surface, 1 = deep
    let emission = in.fluid_data.xyz;

    // 1. Internal shimmer — subtle brightness modulation
    let shimmer = 1.0 + 0.06 * sin(
        in.world_pos.x * 5.0 + in.world_pos.y * 3.0 + time * 1.5
    );
    color = vec4<f32>(color.rgb * shimmer, color.a);

    // 2. Depth darkening — up to 35% darker at max depth
    let depth_factor = 1.0 - depth * 0.35;
    color = vec4<f32>(color.rgb * depth_factor, color.a);

    // 3. Lightmap — integrate with RC lighting
    let lightmap_uv = in.world_pos * lm_xform.scale + lm_xform.offset;
    let light = textureSample(lightmap_texture, lightmap_sampler, lightmap_uv).rgb;
    color = vec4<f32>(color.rgb * light, color.a);

    // 4. Glow/emission — overrides darkness for lava etc.
    color = vec4<f32>(max(color.rgb, emission), color.a);

    return color;
}
```

**Step 2: Verify it compiles with the game**

Run: `cargo build`

(Shader compile errors only appear at runtime in Bevy, so also run the game briefly if possible.)

**Step 3: Commit**

```
feat(fluid): add fluid.wgsl shader with waves, depth, shimmer, lightmap, glow
```

---

### Task 4: Update systems to use FluidMaterial

**Files:**
- Modify: `src/fluid/systems.rs` — update `fluid_rebuild_meshes` to use `FluidMaterial`, add time update

**Step 1: Update fluid_rebuild_meshes**

Change the query and spawn code:
- `existing_fluid_meshes: Query<(Entity, &ChunkCoord), With<FluidMeshEntity>>` stays the same
- `fluid_material: Res<SharedFluidMaterial>` now holds `Handle<FluidMaterial>`
- `MeshMaterial2d(fluid_material.handle.clone())` already works since type changed

Update imports: remove `ColorMaterial` usage, import `MeshMaterial2d` from `bevy::sprite_render`.

**Step 2: Add time update system**

```rust
pub fn update_fluid_time(
    time: Res<Time>,
    shared: Res<SharedFluidMaterial>,
    mut materials: ResMut<Assets<FluidMaterial>>,
) {
    if let Some(mat) = materials.get_mut(&shared.handle) {
        mat.time = time.elapsed_secs();
    }
}
```

Register in FluidPlugin:
```rust
.add_systems(
    Update,
    systems::update_fluid_time
        .run_if(resource_exists::<systems::SharedFluidMaterial>),
)
```

**Step 3: Build**

Run: `cargo build`

**Step 4: Commit**

```
feat(fluid): wire FluidMaterial into rebuild/time systems
```

---

### Task 5: Integrate with RC lightmap pipeline

**Files:**
- Modify: `src/world/rc_lighting.rs` — add FluidMaterial to `update_tile_lightmap`

**Step 1: Add fluid material update**

In `update_tile_lightmap()`, after the lit_sprite_materials loop, add:

```rust
// Update fluid material with current lightmap
use crate::fluid::systems::{FluidMaterial, SharedFluidMaterial};
if let Some(shared_fluid) = world.get_resource::<SharedFluidMaterial>() {
    if let Some(mat) = fluid_materials.get_mut(&shared_fluid.handle) {
        mat.lightmap = gpu_images.lightmap.clone();
        mat.lightmap_uv_rect = lm_params;
    }
}
```

This requires adding `SharedFluidMaterial` and `FluidMaterial` to the system's parameters. Since the fluid module may not always be loaded, use `Option<Res<SharedFluidMaterial>>` and `ResMut<Assets<FluidMaterial>>`.

**Step 2: Build and run tests**

Run: `cargo build && cargo test --package starbeam`

**Step 3: Commit**

```
feat(fluid): integrate fluid shader with RC lightmap pipeline
```

---

### Task 6: Visual testing and polish

**Step 1: Run the game and test**

- Place water (F5) — verify wavy surface, shimmer, depth darkening
- Place steam (F6) — verify gas fills top-down, wave on bottom edge
- Go underground / dark area — verify lightmap darkens fluid
- Check lava definition has light_emission, test glow in dark

**Step 2: Tune shader constants**

Adjust in `fluid.wgsl`:
- Wave amplitude: `0.12` — increase/decrease for taste
- Wave frequency: `3.0` — horizontal frequency
- Wave speed: `2.0` — animation speed
- Shimmer intensity: `0.06` — +-6%
- Depth darkening: `0.35` — max 35% darker

**Step 3: Run all tests**

Run: `cargo test --package starbeam`

All 297+ tests should still pass.

**Step 4: Commit**

```
feat(fluid): tune shader visual parameters
```

---

## Summary

| Task | Description | Files |
|------|-------------|-------|
| 1 | Define FluidMaterial, register plugin | systems.rs, mod.rs |
| 2 | Enrich mesh with UV + FLUID_DATA | render.rs |
| 3 | Write fluid.wgsl shader | fluid.wgsl |
| 4 | Wire material into systems | systems.rs, mod.rs |
| 5 | RC lightmap integration | rc_lighting.rs |
| 6 | Visual testing and polish | fluid.wgsl |
