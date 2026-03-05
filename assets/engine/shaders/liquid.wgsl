#import bevy_sprite::mesh2d_functions as mesh_functions

struct LiquidMaterial {
    color: vec4<f32>,
};

struct LightmapXform {
    scale: vec2<f32>,
    offset: vec2<f32>,
}

@group(2) @binding(0) var<uniform> material: LiquidMaterial;
@group(2) @binding(1) var lightmap_texture: texture_2d<f32>;
@group(2) @binding(2) var lightmap_sampler: sampler;
@group(2) @binding(3) var<uniform> lm_xform: LightmapXform;

struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
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
    out.color = in.color * material.color;
    out.world_pos = (world_from_local * vec4<f32>(in.position, 1.0)).xy;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let lightmap_uv = in.world_pos * lm_xform.scale + lm_xform.offset;
    let light = textureSample(lightmap_texture, lightmap_sampler, lightmap_uv).rgb;
    return vec4<f32>(in.color.rgb * light, in.color.a);
}
