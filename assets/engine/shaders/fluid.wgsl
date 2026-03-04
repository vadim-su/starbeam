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
    debug_mode: u32,    // 0=off, 1=mass, 2=surface, 3=fluid_type, 4=depth
    show_grid: u32,     // 0=no grid, 1=show grid
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
// Debug helpers
// ------------------------------------------------------------------ //

/// Heat-map: 0=black, 0.5=green, 1.0=yellow, >1.0=red
fn heat_color(t: f32) -> vec3<f32> {
    if t <= 0.0 {
        return vec3<f32>(0.05, 0.05, 0.05);
    }
    if t <= 0.5 {
        let s = t / 0.5;
        return mix(vec3<f32>(0.0, 0.1, 0.0), vec3<f32>(0.0, 0.9, 0.0), s);
    }
    if t <= 1.0 {
        let s = (t - 0.5) / 0.5;
        return mix(vec3<f32>(0.0, 0.9, 0.0), vec3<f32>(1.0, 1.0, 0.0), s);
    }
    // t > 1.0: pressurized → red
    let s = clamp((t - 1.0) / 0.5, 0.0, 1.0);
    return mix(vec3<f32>(1.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), s);
}

/// Distinct colour per fluid type (hash fluid_id from color).
fn fluid_type_color(base_color: vec4<f32>) -> vec3<f32> {
    // Use the original vertex colour but fully saturated and bright
    let c = base_color.rgb;
    let mx = max(c.r, max(c.g, c.b));
    if mx < 0.01 {
        return vec3<f32>(0.5, 0.5, 0.5);
    }
    return c / mx; // normalize to max brightness
}

/// Grid line: darken pixels at cell edges.
fn grid_factor(world_pos: vec2<f32>, tile_size: f32) -> f32 {
    let frac_x = fract(world_pos.x / tile_size);
    let frac_y = fract(world_pos.y / tile_size);
    let edge_width = 0.04;
    let on_edge_x = step(frac_x, edge_width) + step(1.0 - edge_width, frac_x);
    let on_edge_y = step(frac_y, edge_width) + step(1.0 - edge_width, frac_y);
    return clamp(on_edge_x + on_edge_y, 0.0, 1.0);
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

    // Skip wave displacement in debug mode for stable cell boundaries
    if is_wave_vertex && uniforms.debug_mode == 0u {
        let world_x = (world_from_local * vec4<f32>(pos, 1.0)).x;
        pos.y += in.wave_height;
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

const DEBUG_CHUNK_BOUNDARIES: bool = false;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let flags      = in.fluid_data.w;
    let is_surface = (flags % 2.0) >= 0.5;
    let is_gas     = flags >= 1.5;
    let emission   = in.fluid_data.xyz;
    let amp        = in.wave_params.x;
    let speed      = in.wave_params.y;
    let fill       = in.uv.x;
    let depth      = in.uv.y;
    let edge       = u32(in.edge_flags);

    // ================================================================ //
    // DEBUG MODES
    // ================================================================ //
    if uniforms.debug_mode > 0u {
        var dbg_color: vec3<f32>;
        let tile_size: f32 = 8.0; // match tile_size for grid

        // Mode 1: Mass heat-map
        if uniforms.debug_mode == 1u {
            // fill = min(mass, 1.0), but we want to show >1.0 too.
            // We encode fill from the vertex as min(mass, 1.0).
            // For pressurized cells (mass > 1.0), fill == 1.0 but alpha is also 1.0.
            // Use alpha channel to detect: alpha = (def_alpha/255) * fill.
            // If fill = 1.0, we can check original color alpha.
            // Simpler: just use fill directly (0..1 range visible).
            dbg_color = heat_color(fill);
        }
        // Mode 2: Surface visualization
        else if uniforms.debug_mode == 2u {
            if is_surface {
                dbg_color = vec3<f32>(1.0, 1.0, 0.3); // bright yellow = surface
            } else {
                dbg_color = vec3<f32>(0.15, 0.15, 0.3); // dark blue = interior
            }
            if is_gas {
                dbg_color = dbg_color * vec3<f32>(0.7, 1.0, 0.7); // greenish tint for gas
            }
        }
        // Mode 3: Fluid type (distinct colour per fluid)
        else if uniforms.debug_mode == 3u {
            dbg_color = fluid_type_color(in.color);
        }
        // Mode 4: Depth grayscale
        else {
            let d = 1.0 - depth; // invert: bright = surface, dark = deep
            dbg_color = vec3<f32>(d, d, d);
            if is_surface {
                dbg_color = vec3<f32>(d, d * 1.2, d * 1.4); // slight blue tint at surface
            }
        }

        // Grid overlay
        if uniforms.show_grid == 1u {
            let g = grid_factor(in.world_pos, tile_size);
            dbg_color = mix(dbg_color, vec3<f32>(0.0, 0.0, 0.0), g * 0.7);
        }

        // Edge flag indicator: thin coloured border
        if uniforms.debug_mode == 2u {
            let frac_x = fract(in.world_pos.x / tile_size);
            let frac_y = fract(in.world_pos.y / tile_size);
            let bw: f32 = 0.08; // border width
            // Left solid (bit 0)
            if (edge & 1u) != 0u && frac_x < bw {
                dbg_color = mix(dbg_color, vec3<f32>(1.0, 0.3, 0.0), 0.8);
            }
            // Right solid (bit 1)
            if (edge & 2u) != 0u && frac_x > (1.0 - bw) {
                dbg_color = mix(dbg_color, vec3<f32>(1.0, 0.3, 0.0), 0.8);
            }
            // Above air (bit 2)
            if (edge & 4u) != 0u && frac_y > (1.0 - bw) {
                dbg_color = mix(dbg_color, vec3<f32>(0.3, 1.0, 1.0), 0.8);
            }
            // Below solid (bit 3)
            if (edge & 8u) != 0u && frac_y < bw {
                dbg_color = mix(dbg_color, vec3<f32>(1.0, 0.3, 0.0), 0.8);
            }
        }

        return vec4<f32>(dbg_color, 0.9);
    }

    // ================================================================ //
    // NORMAL RENDERING (debug_mode == 0)
    // ================================================================ //
    var color = in.color;

    // 1. Depth darkening (liquids only)
    if !is_gas {
        let darken = clamp(depth * 0.4, 0.0, 0.65);
        color = vec4<f32>(color.rgb * (1.0 - darken), color.a);
    }

    // 2. Caustics (liquids only, near surface)
    if !is_gas && depth < 0.5 {
        let tile_pixels: f32 = 32.0;
        let PIXEL_DENSITY: f32 = 8.0;
        let pix_uv = floor(in.world_pos / tile_pixels * PIXEL_DENSITY) / PIXEL_DENSITY;
        let c = caustic(pix_uv, uniforms.time);
        let caustic_strength = clamp(1.0 - depth * 2.0, 0.0, 0.35);
        color = vec4<f32>(color.rgb + c * caustic_strength * vec3<f32>(0.6, 0.8, 1.0), color.a);
    }

    // 3. Shimmer
    let shimmer = 1.0 + 0.05 * sin(in.world_pos.x * 0.5 + uniforms.time * 0.8);
    color = vec4<f32>(color.rgb * shimmer, color.a);

    // 4. Surface effects
    if is_surface && !is_gas {
        color = vec4<f32>(min(color.rgb + amp * 0.3, vec3<f32>(1.0)), color.a);

        let has_solid = (edge & 1u) != 0u || (edge & 2u) != 0u || (edge & 8u) != 0u;
        if has_solid {
            let foam_t = 0.15 + 0.05 * sin(in.world_pos.x * 2.0 + uniforms.time * 1.5);
            color = vec4<f32>(mix(color.rgb, vec3<f32>(0.9, 0.95, 1.0), foam_t), color.a);
        }
    }

    // 5. Lightmap
    let lm_scale  = uniforms.lightmap_uv_rect.xy;
    let lm_offset = uniforms.lightmap_uv_rect.zw;
    let lm_uv     = in.world_pos * lm_scale + lm_offset;
    let light     = textureSample(lightmap_texture, lightmap_sampler, lm_uv).rgb;
    color = vec4<f32>(color.rgb * light, color.a);

    // 6. Emission glow
    let em_strength = max(emission.r, max(emission.g, emission.b));
    if em_strength > 0.0 {
        let em_factor = clamp(em_strength, 0.0, 1.0);
        color = vec4<f32>(mix(color.rgb, emission, em_factor * 0.8), color.a);
    }

    // 7. Partial-fill alpha softening
    if fill < 0.3 {
        let fade = smoothstep(0.0, 0.3, fill);
        color = vec4<f32>(color.rgb, color.a * fade);
    }

    // DEBUG: chunk boundary visualizer (compile-time constant)
    if DEBUG_CHUNK_BOUNDARIES {
        let chunk_world: f32 = 256.0;
        let cx = floor(in.world_pos.x / chunk_world);
        let cy = floor(in.world_pos.y / chunk_world);
        let parity = (i32(cx) + i32(cy)) % 2;
        if parity == 0 {
            return vec4<f32>(0.2, 0.4, 1.0, 1.0);
        } else {
            return vec4<f32>(1.0, 0.3, 0.2, 1.0);
        }
    }

    return color;
}
