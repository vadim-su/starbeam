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
    // Pass world position directly to avoid precision loss from
    // clip->NDC->world round-trip that causes subpixel shimmer.
    out.world_pos = (world_from_local * vec4<f32>(in.position, 1.0)).xy;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let field = textureSample(field_texture, field_sampler, in.uv);

    let lo = uniforms.threshold - uniforms.smoothing;
    let hi = uniforms.threshold + uniforms.smoothing;

    let water_a = smoothstep(lo, hi, field.r);
    let lava_a  = smoothstep(lo, hi, field.g);
    let oil_a   = smoothstep(lo, hi, field.b);

    // Composite back-to-front by density
    var color = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    color = mix(color, uniforms.water_color, water_a);
    color = mix(color, uniforms.lava_color, lava_a);
    color = mix(color, uniforms.oil_color, oil_a);

    if color.a < 0.01 {
        discard;
    }

    // Apply lightmap
    let lm_uv = in.world_pos * lm_xform.scale + lm_xform.offset;
    let light = textureSample(lightmap_texture, lightmap_sampler, lm_uv).rgb;
    color = vec4<f32>(color.rgb * light, color.a);

    return color;
}
