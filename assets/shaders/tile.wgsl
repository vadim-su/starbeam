#import bevy_sprite::mesh2d_functions as mesh_functions
#import bevy_sprite::mesh2d_view_bindings::view

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

// Pre-computed affine transform: world position → lightmap UV.
// lightmap_uv = world_pos * lm_xform.xy + lm_xform.zw
// Computed on CPU from grid_origin, input_size, tile_size.
// Stable: only changes when the RC grid snaps (every max_spacing tiles).
struct LightmapXform {
    scale: vec2<f32>,   // (1/(ts*input_w), -1/(ts*input_h))
    offset: vec2<f32>,  // (-grid_origin_x/input_w, 1+grid_origin_y/input_h)
}

@group(2) @binding(0) var atlas_texture: texture_2d<f32>;
@group(2) @binding(1) var atlas_sampler: sampler;
@group(2) @binding(2) var<uniform> uniforms: TileUniforms;
@group(2) @binding(3) var lightmap_texture: texture_2d<f32>;
@group(2) @binding(4) var lightmap_sampler: sampler;
@group(2) @binding(5) var<uniform> lm_xform: LightmapXform;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(atlas_texture, atlas_sampler, in.uv);
    if color.a < 0.01 {
        if uniforms.dim < 1.0 {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
        discard;
    }

    // --- Compute lightmap UV from world position ---
    // Derive world position from clip_position via inverse view-projection
    // (guaranteed current-frame camera data, no lag).
    let screen_uv = (in.clip_position.xy - view.viewport.xy) / view.viewport.zw;
    let ndc = screen_uv * vec2(2.0, -2.0) + vec2(-1.0, 1.0);
    let world_h = view.world_from_clip * vec4(ndc, 0.0, 1.0);
    let world_pos = world_h.xy / world_h.w;

    // Affine transform: world_pos → lightmap UV (pre-computed on CPU)
    let lightmap_uv = world_pos * lm_xform.scale + lm_xform.offset;

    let light = textureSample(lightmap_texture, lightmap_sampler, lightmap_uv).rgb;

    return vec4<f32>(color.rgb * light * uniforms.dim, color.a);
}
