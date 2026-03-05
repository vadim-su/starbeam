#import bevy_sprite::mesh2d_functions as mesh_functions

struct LiquidFieldUniforms {
    water_color: vec4<f32>,
    lava_color: vec4<f32>,
    oil_color: vec4<f32>,
    threshold: f32,
    smoothing: f32,
    tile_size: f32,
    time: f32,
    field_origin: vec2<f32>,
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
    out.world_pos = (world_from_local * vec4<f32>(in.position, 1.0)).xy;
    return out;
}

// ---------------------------------------------------------------------------
// Voronoi helpers
// ---------------------------------------------------------------------------

fn hash2(p: vec2<f32>) -> vec2<f32> {
    let k = vec2<f32>(127.1, 311.7);
    return fract(sin(vec2<f32>(dot(p, k), dot(p, k.yx))) * 43758.5453);
}

/// Animated Voronoi — returns (min_dist, second_min_dist).
fn voronoi(p: vec2<f32>, t: f32) -> vec2<f32> {
    let cell = floor(p);
    let frac = fract(p);

    var d1 = 8.0;
    var d2 = 8.0;

    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let neighbor = vec2<f32>(f32(i), f32(j));
            let point = hash2(cell + neighbor);
            let animated = neighbor + 0.5 + 0.4 * sin(t * 0.6 + 6.2831 * point) - frac;
            let dist = dot(animated, animated);
            if dist < d1 {
                d2 = d1;
                d1 = dist;
            } else if dist < d2 {
                d2 = dist;
            }
        }
    }

    return vec2<f32>(sqrt(d1), sqrt(d2));
}

// ---------------------------------------------------------------------------
// Fragment
// ---------------------------------------------------------------------------

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let ts = uniforms.tile_size;
    let tex_dims = vec2<f32>(textureDimensions(field_texture));
    let texel = 1.0 / tex_dims;

    // --- Fractional position within the current tile (0=bottom, 1=top) ---
    let frac_y = fract(in.world_pos.y / ts);
    let frac_x = fract(in.world_pos.x / ts);

    // --- Snap UV to tile/pixel centers for per-tile level lookup ---
    // in.uv maps the quad 1:1 to the texture, so pixel coords = uv * dims.
    let pixel_coord = floor(in.uv * tex_dims);
    let center_uv = (pixel_coord + 0.5) * texel;

    // Sample this tile and its neighbors.
    let field_c = textureSample(field_texture, field_sampler, center_uv);
    let field_l = textureSample(field_texture, field_sampler, center_uv + vec2<f32>(-texel.x, 0.0));
    let field_r = textureSample(field_texture, field_sampler, center_uv + vec2<f32>( texel.x, 0.0));
    // Tile above in world = one row UP in texture = -V direction (texture Y is flipped).
    let field_above = textureSample(field_texture, field_sampler, center_uv + vec2<f32>(0.0, -texel.y));

    // --- Per-liquid level with horizontal smoothing ---
    // Blend with left/right neighbors at tile edges for smooth transitions.
    let bl = smoothstep(0.5, 0.0, frac_x);  // weight for left neighbor
    let br = smoothstep(0.5, 1.0, frac_x);  // weight for right neighbor
    let bc = 1.0 - bl - br;                   // weight for center

    let water_lvl = field_c.r * bc + field_l.r * bl + field_r.r * br;
    let lava_lvl  = field_c.g * bc + field_l.g * bl + field_r.g * br;
    let oil_lvl   = field_c.b * bc + field_l.b * bl + field_r.b * br;

    // --- Determine if this tile is a surface tile or submerged ---
    // If the tile above has liquid, this tile is fully underwater — fill it
    // completely to avoid visible horizontal lines at tile boundaries.
    let above_water = field_above.r > 0.02;
    let above_lava  = field_above.g > 0.02;
    let above_oil   = field_above.b > 0.02;

    // Effective level: 1.0 for submerged tiles, actual level for surface.
    let eff_water = select(water_lvl, 1.0, above_water && water_lvl > 0.01);
    let eff_lava  = select(lava_lvl,  1.0, above_lava  && lava_lvl  > 0.01);
    let eff_oil   = select(oil_lvl,   1.0, above_oil   && oil_lvl   > 0.01);

    // --- Height test: show liquid below the surface line ---
    let edge = 0.04;
    let water_fill = (1.0 - smoothstep(eff_water - edge, eff_water + edge, frac_y))
                   * step(0.01, water_lvl);
    let lava_fill  = (1.0 - smoothstep(eff_lava  - edge, eff_lava  + edge, frac_y))
                   * step(0.01, lava_lvl);
    let oil_fill   = (1.0 - smoothstep(eff_oil   - edge, eff_oil   + edge, frac_y))
                   * step(0.01, oil_lvl);

    // --- Voronoi caustic highlights near the water surface ---
    let voronoi_scale = 0.25;
    let vp = in.world_pos * voronoi_scale;
    let vor = voronoi(vp, uniforms.time);
    let caustic = smoothstep(0.0, 0.35, vor.y - vor.x) * 0.55;

    // Surface highlight: visible in a thin band below the surface line.
    let surface_band = 0.18;
    let dist_to_surf_w = eff_water - frac_y;
    let dist_to_surf_l = eff_lava  - frac_y;

    let surf_w = smoothstep(surface_band, 0.0, dist_to_surf_w)
               * smoothstep(-edge, edge * 2.0, dist_to_surf_w);
    let surf_l = smoothstep(surface_band, 0.0, dist_to_surf_l)
               * smoothstep(-edge, edge * 2.0, dist_to_surf_l);

    let highlight_w = caustic * surf_w;
    let highlight_l = caustic * surf_l * 1.5;

    // --- Composite back-to-front by density ---
    var color = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    if water_fill > 0.001 {
        let wc = uniforms.water_color;
        let lit = wc.rgb + vec3<f32>(highlight_w);
        color = mix(color, vec4<f32>(lit, wc.a), water_fill);
    }

    if lava_fill > 0.001 {
        let lc = uniforms.lava_color;
        let lit = lc.rgb + vec3<f32>(highlight_l * 0.8, highlight_l * 0.4, 0.0);
        color = mix(color, vec4<f32>(lit, lc.a), lava_fill);
    }

    if oil_fill > 0.001 {
        color = mix(color, uniforms.oil_color, oil_fill);
    }

    if color.a < 0.01 {
        discard;
    }

    // --- Lightmap ---
    let lm_uv = in.world_pos * lm_xform.scale + lm_xform.offset;
    let light = textureSample(lightmap_texture, lightmap_sampler, lm_uv).rgb;
    color = vec4<f32>(color.rgb * light, color.a);

    return color;
}
