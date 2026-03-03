#import bevy_sprite::mesh2d_functions as mesh_functions
#import bevy_sprite::mesh2d_view_bindings::view

struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,          // [fill_level, depth_in_fluid]
    @location(3) fluid_data: vec4<f32>,  // [emission_r, emission_g, emission_b, flags]
    @location(4) wave_height: f32,
    @location(5) wave_params: vec2<f32>, // [amplitude_multiplier, speed_multiplier]
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world_pos: vec2<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) fluid_data: vec4<f32>,
    @location(4) wave_params: vec2<f32>,
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

    // No sine-wave vertex displacement — waves are drawn in the fragment shader
    // to avoid geometry appearing above the fill level.
    // Physics-based wave_height is also applied in the fragment shader.
    let world_pos = (world_from_local * vec4<f32>(in.position, 1.0)).xy;

    out.clip_position = mesh_functions::mesh2d_position_local_to_clip(
        world_from_local,
        vec4<f32>(in.position, 1.0),
    );
    out.color = in.color;
    out.world_pos = world_pos;
    out.uv = in.uv;
    out.fluid_data = in.fluid_data;
    out.wave_params = in.wave_params;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = in.color;

    let flags      = in.fluid_data.w;
    let is_surface = (flags % 2.0) >= 0.5;
    let is_gas     = flags >= 1.5;
    let emission   = in.fluid_data.xyz;
    let world_x    = in.world_pos.x;
    let world_y    = in.world_pos.y;
    let fill       = in.uv.x;
    let amp        = in.wave_params.x;
    let speed      = in.wave_params.y;

    // ------------------------------------------------------------------ //
    // 1. Animated wave edge — liquid surface only.
    //
    // UV_0.y carries a per-vertex local gradient interpolated by the GPU:
    //   bottom vertices → 0.0
    //   top vertices    → fill
    // This gives a smooth 0→fill ramp with no discontinuity (unlike fract).
    //
    // Two low-frequency octaves shift the water line up/down. Fragments
    // above the threshold are discarded, carving a wavy edge into the quad
    // without moving any geometry.
    //
    // Frequencies chosen so each wave has a period of 25–63 world units
    // (3–8 tiles at tile_size=8), giving visible rolling swells rather than
    // a high-frequency zigzag.
    // ------------------------------------------------------------------ //
    if is_surface && !is_gas {
        let local_y = in.uv.y; // 0.0 at bottom vertex, fill at top vertex (per-vertex UV)

        // Two-octave wave — low spatial frequencies to avoid sawtooth
        // w1: period ≈ 63 world units (8 tiles), slow
        // w2: period ≈ 29 world units (4 tiles), slightly faster
        let w1 = sin(world_x * 0.10 + uniforms.time * 1.2 * speed) * 0.5 + 0.5;
        let w2 = sin(world_x * 0.22 - uniforms.time * 1.7 * speed) * 0.3 + 0.3;
        let wave_f = clamp((w1 + w2) / 1.6, 0.0, 1.0); // 0 = trough, 1 = crest

        // Max dip: 25% of fill height × amplitude multiplier.
        // Since local_y is in fill-space (0..fill), dip is also in fill-space.
        let max_dip   = 0.25 * amp * fill;
        let threshold = fill - max_dip * (1.0 - wave_f);

        // Discard fragments above the animated water line.
        if local_y > threshold {
            discard;
        }

        // Surface glint: thin bright band just below the wave crest.
        let glint_zone = max(max_dip * 0.35, 0.002);
        let dist = threshold - local_y;
        if dist < glint_zone {
            let glint = (1.0 - dist / glint_zone) * 0.35 * amp;
            color = vec4<f32>(min(color.rgb + glint, vec3(1.0)), color.a);
        }
    }

    // ------------------------------------------------------------------ //
    // 2. Subtle shimmer — very low frequency to avoid diagonal stripe banding.
    // Amplitude 0.06 (was 0.18), spatial frequency 0.5 (was 4.0).
    // ------------------------------------------------------------------ //
    let shimmer = 1.0 + 0.06 * sin(world_x * 0.5 + uniforms.time * 0.8);
    color = vec4<f32>(color.rgb * shimmer, color.a);

    // ------------------------------------------------------------------ //
    // 3. Lightmap
    // ------------------------------------------------------------------ //
    let lm_scale  = uniforms.lightmap_uv_rect.xy;
    let lm_offset = uniforms.lightmap_uv_rect.zw;
    let lm_uv     = in.world_pos * lm_scale + lm_offset;
    let light     = textureSample(lightmap_texture, lightmap_sampler, lm_uv).rgb;
    color = vec4<f32>(color.rgb * light, color.a);

    // ------------------------------------------------------------------ //
    // 4. Emission glow
    // ------------------------------------------------------------------ //
    color = vec4<f32>(max(color.rgb, emission), color.a);

    return color;
}
