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
    @location(1) world_pos: vec2<f32>,
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
    out.world_pos = (world_from_local * vec4<f32>(in.position, 1.0)).xy;
    return out;
}

/// xyz = fluid base color, w = alpha.
struct FluidUniforms {
    color_alpha: vec4<f32>,
}

struct LightmapXform {
    scale: vec2<f32>,
    offset: vec2<f32>,
}

@group(2) @binding(0) var<uniform> uniforms: FluidUniforms;
@group(2) @binding(1) var lightmap_texture: texture_2d<f32>;
@group(2) @binding(2) var lightmap_sampler: sampler;
@group(2) @binding(3) var<uniform> lm_xform: LightmapXform;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // UV.y encodes fluid level: 0.0 at bottom, 1.0 at surface.
    // Slight vertical gradient for visual depth.
    let depth_factor = mix(0.7, 1.0, in.uv.y);

    let lightmap_uv = in.world_pos * lm_xform.scale + lm_xform.offset;
    let light = textureSample(lightmap_texture, lightmap_sampler, lightmap_uv).rgb;

    let color = uniforms.color_alpha.rgb;
    let alpha = uniforms.color_alpha.w;

    let lit_color = color * depth_factor * light;
    return vec4<f32>(lit_color, alpha);
}
