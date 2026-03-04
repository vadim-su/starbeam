#import bevy_sprite::mesh2d_functions as mesh_functions
#import bevy_sprite::mesh2d_view_bindings::view

struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world_pos: vec2<f32>,
    @location(2) color: vec4<f32>,
    // fill level (0..1) is encoded in uv.y range — the quad is already
    // sized to the fill height, so we use uv for surface wave clipping.
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
    out.color = in.color;
    return out;
}

struct FluidUniforms {
    time: f32,
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
    // uv.y: 0.0 = bottom of fluid quad, 1.0 = top of fluid quad
    // The quad height is already set to fill_level * tile_size on the CPU,
    // so we only need surface wave clipping near the top edge.

    // Surface wave: gentle sine displacement near the top
    let surface_band = 0.08; // fraction of quad height for wave effect
    let near_top = 1.0 - in.uv.y;
    if near_top < surface_band {
        let wave_freq = 0.15;
        let wave_speed = 2.5;
        let wave_amp = surface_band * 0.6;
        let wave = sin(in.world_pos.x * wave_freq + uniforms.time * wave_speed) * wave_amp;
        let threshold = surface_band + wave;
        if near_top < surface_band - threshold {
            discard;
        }
    }

    // Lightmap sampling (matching tile.wgsl approach)
    let lightmap_uv = in.world_pos * lm_xform.scale + lm_xform.offset;
    let light = textureSample(lightmap_texture, lightmap_sampler, lightmap_uv).rgb;

    let base_color = in.color;
    return vec4<f32>(base_color.rgb * light, base_color.a);
}
