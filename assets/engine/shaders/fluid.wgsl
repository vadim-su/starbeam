#import bevy_sprite::mesh2d_functions as mesh_functions
#import bevy_sprite::mesh2d_view_bindings::view

struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world_pos: vec2<f32>,
    @location(2) uv: vec2<f32>,
}

struct FluidUniforms {
    lightmap_uv_rect: vec4<f32>,
    time: f32,
    debug_mode: u32,
    show_grid: u32,
    enable_caustics: u32,
    enable_shimmer: u32,
}

@group(2) @binding(0) var lightmap_texture: texture_2d<f32>;
@group(2) @binding(1) var lightmap_sampler: sampler;
@group(2) @binding(2) var<uniform> uniforms: FluidUniforms;

// ------------------------------------------------------------------ //
// Voronoi noise helpers for caustics
// ------------------------------------------------------------------ //

fn hash2(p: vec2<f32>) -> vec2<f32> {
    var q = p * vec2<f32>(0.3183099, 0.3678794) + vec2<f32>(0.3678794, 0.3183099);
    q = fract(q * 715.836) * 2.0 - 1.0;
    return fract(q * vec2<f32>(349.572, 574.213));
}

fn voronoi_dist(uv: vec2<f32>) -> f32 {
    let cell = floor(uv);
    let f = fract(uv);
    var min_dist: f32 = 8.0;
    for (var j: i32 = -1; j <= 1; j++) {
        for (var i: i32 = -1; i <= 1; i++) {
            let nb = vec2<f32>(f32(i), f32(j));
            let pt = hash2(cell + nb);
            let diff = nb + pt - f;
            min_dist = min(min_dist, dot(diff, diff));
        }
    }
    return sqrt(min_dist);
}

fn caustic(uv: vec2<f32>, t: f32) -> f32 {
    let PIXEL_DENSITY: f32 = 8.0;
    let puv = floor(uv * PIXEL_DENSITY) / PIXEL_DENSITY;
    let c1 = voronoi_dist(puv * 3.0 + vec2<f32>(t * 0.4, t * 0.3));
    let c2 = voronoi_dist(puv * 5.0 - vec2<f32>(t * 0.2, t * 0.5));
    return smoothstep(0.3, 0.0, min(c1, c2));
}

// ------------------------------------------------------------------ //
// Vertex shader
// ------------------------------------------------------------------ //

@vertex
fn vertex(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);

    let world_pos = (world_from_local * vec4<f32>(in.position, 1.0)).xy;
    out.clip_position = mesh_functions::mesh2d_position_local_to_clip(
        world_from_local, vec4<f32>(in.position, 1.0),
    );
    out.color = in.color;
    out.world_pos = world_pos;
    out.uv = in.uv;
    return out;
}

// ------------------------------------------------------------------ //
// Fragment shader
// ------------------------------------------------------------------ //

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = in.color;

    // Circle mask: discard pixels outside radius from quad center
    let center_offset = in.uv - vec2<f32>(0.5, 0.5);
    let dist_sq = dot(center_offset, center_offset);
    // Hard circle at radius 0.5, with soft edge for anti-aliasing
    let radius_sq = 0.25; // 0.5^2
    if dist_sq > radius_sq {
        discard;
    }
    // Soft edge (anti-alias the last 10% of radius)
    let edge_softness = smoothstep(radius_sq, radius_sq * 0.8, dist_sq);
    color = vec4<f32>(color.rgb, color.a * edge_softness);

    // In debug mode, skip visual effects (caustics, shimmer, lightmap)
    if uniforms.debug_mode != 0u {
        return color;
    }

    // 1. Caustics
    if uniforms.enable_caustics != 0u {
        let tile_pixels: f32 = 32.0;
        let PIXEL_DENSITY: f32 = 8.0;
        let pix_uv = floor(in.world_pos / tile_pixels * PIXEL_DENSITY) / PIXEL_DENSITY;
        let c = caustic(pix_uv, uniforms.time);
        color = vec4<f32>(color.rgb + c * 0.15 * vec3<f32>(0.6, 0.8, 1.0), color.a);
    }

    // 2. Shimmer
    if uniforms.enable_shimmer != 0u {
        let shimmer = 1.0 + 0.05 * sin(in.world_pos.x * 0.5 + uniforms.time * 0.8);
        color = vec4<f32>(color.rgb * shimmer, color.a);
    }

    // 3. Lightmap
    let lm_scale  = uniforms.lightmap_uv_rect.xy;
    let lm_offset = uniforms.lightmap_uv_rect.zw;
    let lm_uv     = in.world_pos * lm_scale + lm_offset;
    let light     = textureSample(lightmap_texture, lightmap_sampler, lm_uv).rgb;
    color = vec4<f32>(color.rgb * light, color.a);

    return color;
}
