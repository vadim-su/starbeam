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
    // We compute a local Y fraction within the tile using fract().
    // For a liquid cell at tile-aligned base_y:
    //   fract(world_y / tile_size) → 0.0 at bottom vertex, fill at top vertex.
    //
    // A two-octave wave shifts the "water line" up/down within the top portion
    // of the surface quad. Fragments above the water line are discarded,
    // revealing the background (sky/air) and creating a wavy edge — all
    // without moving any geometry above the fill level.
    // ------------------------------------------------------------------ //
    let tile_size = 8.0; // must match generation.ron tile_size

    if is_surface && !is_gas {
        let local_y = fract(world_y / tile_size); // 0 = bottom, fill = top of quad

        // Two-octave wave (0..1 range combined)
        let w1 = sin(world_x * 1.5 + uniforms.time * 1.5 * speed) * 0.5 + 0.5;
        let w2 = sin(world_x * 3.5 - uniforms.time * 2.2 * speed) * 0.3 + 0.3;
        let wave_f = clamp((w1 + w2) / 1.6, 0.0, 1.0); // 0 = trough, 1 = crest

        // Physics ripple: convert wave_height to a fraction of tile height and
        // shift the threshold.  Clamped so it can only deepen the trough, never
        // push the surface above fill level.
        let physics_dip = clamp(-in.wave_height / tile_size, 0.0, 0.4);

        // Max dip: 28% of tile height × amplitude multiplier, plus physics.
        let max_dip   = 0.28 * amp + physics_dip;
        let threshold = fill - max_dip * (1.0 - wave_f);

        // Discard fragments that are above the animated water line.
        if local_y > threshold {
            discard;
        }

        // Surface glint: thin bright band just below the wave crest.
        let glint_zone = max(max_dip * 0.35, 0.01);
        let dist = threshold - local_y;
        if dist < glint_zone {
            let glint = (1.0 - dist / glint_zone) * 0.4 * amp;
            color = vec4<f32>(min(color.rgb + glint, vec3(1.0)), color.a);
        }
    }

    // ------------------------------------------------------------------ //
    // 2. Shimmer — animated diagonal brightness for the whole fluid body.
    // ------------------------------------------------------------------ //
    let shimmer = 1.0 + 0.18 * sin(world_x * 4.0 + world_y * 2.5 + uniforms.time * 1.4);
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
