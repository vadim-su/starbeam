#import bevy_sprite::mesh2d_functions as mesh_functions
#import bevy_sprite::mesh2d_view_bindings::view

struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,        // [fill_level, depth_in_fluid]
    @location(3) fluid_data: vec4<f32>, // [emission_r, emission_g, emission_b, flags]
    @location(4) wave_height: f32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world_pos: vec2<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) fluid_data: vec4<f32>,
}

struct FluidUniforms {
    lightmap_uv_rect: vec4<f32>,
    time: f32,
}

@group(2) @binding(0) var lightmap_texture: texture_2d<f32>;
@group(2) @binding(1) var lightmap_sampler: sampler;
@group(2) @binding(2) var<uniform> uniforms: FluidUniforms;

@vertex
fn vertex(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);

    // Compute world position for wave displacement and lightmap stability
    var world_pos = (world_from_local * vec4<f32>(in.position, 1.0)).xy;

    // Wave effect: displace y for surface vertices
    // flags: is_wave_vertex * 1.0 + is_gas * 2.0
    let flags = in.fluid_data.w;
    let is_wave = (flags % 2.0) >= 0.5;
    if is_wave {
        // Multi-octave ripple: base (slow, large) + mid + detail (fast, small)
        let base   = sin(world_pos.x * 1.5 + uniforms.time * 1.0) * 1.2;
        let mid    = sin(world_pos.x * 4.0 + world_pos.y * 0.5 + uniforms.time * 1.8) * 0.5;
        let detail = sin(world_pos.x * 9.0 - world_pos.y * 1.2 + uniforms.time * 3.0) * 0.2;
        world_pos.y += base + mid + detail + in.wave_height;
    }

    // Reconstruct local position with wave offset applied
    var pos = in.position;
    let original_world = (world_from_local * vec4<f32>(in.position, 1.0)).xy;
    pos.y += world_pos.y - original_world.y;

    out.clip_position = mesh_functions::mesh2d_position_local_to_clip(
        world_from_local,
        vec4<f32>(pos, 1.0),
    );
    out.color = in.color;
    out.world_pos = world_pos;
    out.uv = in.uv;
    out.fluid_data = in.fluid_data;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = in.color;
    let depth = in.uv.y;
    let emission = in.fluid_data.xyz;
    let world_x = in.world_pos.x;
    let world_y = in.world_pos.y;

    // 1. Shimmer: subtle brightness oscillation based on world position and time
    let shimmer = 1.0 + 0.12 * sin(world_x * 5.0 + world_y * 3.0 + uniforms.time * 1.5);
    color = vec4<f32>(color.rgb * shimmer, color.a);

    // 2. Lightmap: multiply by lightmap sample at world position
    let lm_scale = uniforms.lightmap_uv_rect.xy;
    let lm_offset = uniforms.lightmap_uv_rect.zw;
    let lightmap_uv = in.world_pos * lm_scale + lm_offset;
    let light = textureSample(lightmap_texture, lightmap_sampler, lightmap_uv).rgb;
    color = vec4<f32>(color.rgb * light, color.a);

    // 4. Glow: emission overrides darkness (max of lit color and emission)
    color = vec4<f32>(max(color.rgb, emission), color.a);

    return color;
}
