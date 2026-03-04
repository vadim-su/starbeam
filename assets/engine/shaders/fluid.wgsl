#import bevy_sprite::mesh2d_functions as mesh_functions
#import bevy_sprite::mesh2d_view_bindings::view

struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,           // [fill_level, depth_in_fluid]
    @location(3) fluid_data: vec4<f32>,   // [emission_r, emission_g, emission_b, flags]
    @location(4) wave_height: f32,
    @location(5) wave_params: vec2<f32>,  // [amplitude_multiplier, speed_multiplier]
    @location(6) edge_flags: f32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world_pos: vec2<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) fluid_data: vec4<f32>,
    @location(4) wave_params: vec2<f32>,
    @location(5) edge_flags: f32,
}

struct FluidUniforms {
    lightmap_uv_rect: vec4<f32>,
    time: f32,
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

    let flags = in.fluid_data.w;
    let is_wave_vertex = (flags % 2.0) >= 0.5;
    let amp = in.wave_params.x;
    let speed = in.wave_params.y;

    var pos = in.position;

    if is_wave_vertex {
        let world_x = (world_from_local * vec4<f32>(pos, 1.0)).x;
        // Physics-driven displacement from wave propagation simulation
        pos.y += in.wave_height;
        // Procedural 2-octave sine waves (subtle ambient ripple)
        let w1 = sin(world_x * 0.10 + uniforms.time * 1.2 * speed) * 0.3;
        let w2 = sin(world_x * 0.22 - uniforms.time * 1.7 * speed) * 0.15;
        pos.y += (w1 + w2) * amp;
    }

    let world_pos = (world_from_local * vec4<f32>(pos, 1.0)).xy;
    out.clip_position = mesh_functions::mesh2d_position_local_to_clip(
        world_from_local, vec4<f32>(pos, 1.0),
    );
    out.color = in.color;
    out.world_pos = world_pos;
    out.uv = in.uv;
    out.fluid_data = in.fluid_data;
    out.wave_params = in.wave_params;
    out.edge_flags = in.edge_flags;
    return out;
}

// ------------------------------------------------------------------ //
// Fragment shader
// ------------------------------------------------------------------ //

// DEBUG: visualize chunk boundaries.
// chunk_size=32, tile_size=8 → chunk world size = 256 units.
// Each chunk gets a different shade so boundaries are clearly visible.
// Set to true to enable.
const DEBUG_CHUNK_BOUNDARIES: bool = false;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = in.color;

    let flags      = in.fluid_data.w;
    let is_surface = (flags % 2.0) >= 0.5;
    let is_gas     = flags >= 1.5;
    let emission   = in.fluid_data.xyz;
    let amp        = in.wave_params.x;
    let speed      = in.wave_params.y;
    let depth      = in.uv.y; // depth_in_fluid: 0=surface, up to 1.0
    let edge       = u32(in.edge_flags);

    // ------------------------------------------------------------------ //
    // 1. Depth darkening (liquids only, not gas)
    // ------------------------------------------------------------------ //
    if !is_gas {
        let darken = clamp(depth * 0.4, 0.0, 0.65);
        color = vec4<f32>(color.rgb * (1.0 - darken), color.a);
    }

    // ------------------------------------------------------------------ //
    // 2. Caustics (liquids only, depth < 0.5)
    // ------------------------------------------------------------------ //
    if !is_gas && depth < 0.5 {
        let tile_pixels: f32 = 32.0;
        let PIXEL_DENSITY: f32 = 8.0;
        let pix_uv = floor(in.world_pos / tile_pixels * PIXEL_DENSITY) / PIXEL_DENSITY;
        let c = caustic(pix_uv, uniforms.time);
        let caustic_strength = clamp(1.0 - depth * 2.0, 0.0, 0.35);
        color = vec4<f32>(color.rgb + c * caustic_strength * vec3<f32>(0.6, 0.8, 1.0), color.a);
    }

    // ------------------------------------------------------------------ //
    // 3. Shimmer (all fluids)
    // ------------------------------------------------------------------ //
    let shimmer = 1.0 + 0.05 * sin(in.world_pos.x * 0.5 + uniforms.time * 0.8);
    color = vec4<f32>(color.rgb * shimmer, color.a);

    // ------------------------------------------------------------------ //
    // 4. Surface effects (only surface && !gas)
    // ------------------------------------------------------------------ //
    if is_surface && !is_gas {
        // Surface glint
        color = vec4<f32>(min(color.rgb + amp * 0.3, vec3<f32>(1.0)), color.a);

        // Shore foam where solid neighbors exist (bit 0=left, bit 1=right, bit 3=below)
        let has_solid = (edge & 1u) != 0u || (edge & 2u) != 0u || (edge & 8u) != 0u;
        if has_solid {
            let foam_t = 0.15 + 0.05 * sin(in.world_pos.x * 2.0 + uniforms.time * 1.5);
            color = vec4<f32>(mix(color.rgb, vec3<f32>(0.9, 0.95, 1.0), foam_t), color.a);
        }
    }

    // ------------------------------------------------------------------ //
    // 5. Lightmap (all fluids)
    // ------------------------------------------------------------------ //
    let lm_scale  = uniforms.lightmap_uv_rect.xy;
    let lm_offset = uniforms.lightmap_uv_rect.zw;
    let lm_uv     = in.world_pos * lm_scale + lm_offset;
    let light     = textureSample(lightmap_texture, lightmap_sampler, lm_uv).rgb;
    color = vec4<f32>(color.rgb * light, color.a);

    // ------------------------------------------------------------------ //
    // 6. Emission glow (all fluids)
    // ------------------------------------------------------------------ //
    color = vec4<f32>(max(color.rgb, emission), color.a);

    // ------------------------------------------------------------------ //
    // DEBUG: chunk boundary visualizer
    // Shows alternating tints based on chunk_x / chunk_y parity.
    // chunk_size=32, tile_size=8 → 256 world units per chunk.
    // ------------------------------------------------------------------ //
    if DEBUG_CHUNK_BOUNDARIES {
        let chunk_world: f32 = 256.0; // 32 * 8
        let cx = floor(in.world_pos.x / chunk_world);
        let cy = floor(in.world_pos.y / chunk_world);
        let parity = (i32(cx) + i32(cy)) % 2;
        if parity == 0 {
            return vec4<f32>(0.2, 0.4, 1.0, 1.0); // blue
        } else {
            return vec4<f32>(1.0, 0.3, 0.2, 1.0); // red
        }
    }

    return color;
}
