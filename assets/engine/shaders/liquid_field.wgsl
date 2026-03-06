#import bevy_sprite::mesh2d_functions as mesh_functions

// ---------------------------------------------------------------------------
// Height-based per-tile liquid shader
// ---------------------------------------------------------------------------
//
// Renders liquid as height-proportional filled rectangles per tile.
// Each tile is filled from the bottom up to the liquid level, with:
//   - Smooth horizontal blending between adjacent liquid tiles
//   - Submerged tiles (tile above has liquid) are fully filled (no stripes)
//   - Animated Voronoi caustic highlights near the water surface
//   - All lookups use textureLoad (no bilinear filtering artifacts)
//   - All coordinates derived from world_pos (no UV/world_pos misalignment)
//
// Texture layout:
//   - 1 pixel per tile, RGBA8Unorm
//   - R = water level, G = lava level, B = oil level, A = max(R,G,B)
//   - Row 0 = top = highest tile Y (Y is flipped vs world space)

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

// Minimum liquid level to consider a tile as "has liquid"
const MIN_LEVEL: f32 = 0.004;

// Minimum neighbor level to participate in horizontal blending
const MIN_NEIGHBOR_LEVEL: f32 = 0.01;

// Minimum level for isolated tiles (no liquid neighbors) to be visible.
// Hides tiny scattered drops that look ugly as individual pixels.
const ISOLATED_MIN_LEVEL: f32 = 0.08;

// Smoothstep half-width for the surface edge transition
const SURFACE_EDGE: f32 = 0.04;

// Horizontal blend region: how far into the tile (from each edge) blending
// extends. 0.5 means the full tile participates; smaller values limit
// blending to tile edges only.
const H_BLEND_HALF: f32 = 0.5;

// Voronoi pattern spatial frequency (higher = smaller cells)
const VORONOI_FREQ: f32 = 0.25;

// Voronoi animation speed multiplier
const VORONOI_SPEED: f32 = 0.6;

// Voronoi caustic intensity (0 = off, 1 = very bright)
const CAUSTIC_INTENSITY: f32 = 0.55;

// Voronoi edge threshold for caustic pattern
const CAUSTIC_EDGE_LO: f32 = 0.0;
const CAUSTIC_EDGE_HI: f32 = 0.35;

// Width of the surface band where caustics are visible (in tile-fraction units)
const SURFACE_BAND: f32 = 0.18;

// ---------------------------------------------------------------------------
// Uniforms and bindings
// ---------------------------------------------------------------------------

struct LiquidFieldUniforms {
    water_color: vec4<f32>,
    lava_color: vec4<f32>,
    oil_color: vec4<f32>,
    threshold: f32,    // currently unused by height shader (kept for compat)
    smoothing: f32,    // currently unused by height shader (kept for compat)
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
@group(2) @binding(2) var field_sampler: sampler;  // unused, kept for binding compat
@group(2) @binding(3) var lightmap_texture: texture_2d<f32>;
@group(2) @binding(4) var lightmap_sampler: sampler;
@group(2) @binding(5) var<uniform> lm_xform: LightmapXform;

// ---------------------------------------------------------------------------
// Vertex
// ---------------------------------------------------------------------------

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
// Helpers: safe texture load
// ---------------------------------------------------------------------------

/// Load a pixel from the field texture, clamping coords to valid range.
fn load_field(px: i32, py: i32, tex_max: vec2<i32>) -> vec4<f32> {
    let clamped = clamp(vec2<i32>(px, py), vec2<i32>(0), tex_max);
    return textureLoad(field_texture, clamped, 0);
}

/// Return the maximum channel value (combined liquid level) for a field sample.
fn max_level(field: vec4<f32>) -> f32 {
    return max(field.r, max(field.g, field.b));
}

// ---------------------------------------------------------------------------
// Voronoi helpers
// ---------------------------------------------------------------------------

fn hash2(p: vec2<f32>) -> vec2<f32> {
    let k = vec2<f32>(127.1, 311.7);
    return fract(sin(vec2<f32>(dot(p, k), dot(p, k.yx))) * 43758.5453);
}

/// Returns (d1, d2) — distances to nearest and second-nearest Voronoi cell.
fn voronoi(p: vec2<f32>, t: f32) -> vec2<f32> {
    let cell = floor(p);
    let frac = fract(p);
    var d1 = 8.0;
    var d2 = 8.0;
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let neighbor = vec2<f32>(f32(i), f32(j));
            let point = hash2(cell + neighbor);
            let animated = neighbor + 0.5 + 0.4 * sin(t * VORONOI_SPEED + 6.2831 * point) - frac;
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
    let tex_max = vec2<i32>(tex_dims) - 1;

    // -----------------------------------------------------------------------
    // Step 1: Derive tile coordinate and sub-tile fraction from world_pos.
    //
    // CRITICAL: Both the integer tile coord and the fractional position MUST
    // come from the SAME source (world_pos → field_origin division). Using
    // UV for one and world_pos for the other causes off-by-one misalignment
    // at tile boundaries.
    // -----------------------------------------------------------------------

    let tile_in_field = (in.world_pos - uniforms.field_origin) / ts;
    let tile_coord = floor(tile_in_field);
    let frac = tile_in_field - tile_coord;  // sub-tile position, both x and y in [0, 1)

    // Texture pixel coordinate: X maps directly, Y is flipped.
    // Row 0 in texture = highest tile Y in world space.
    let px = i32(tile_coord.x);
    let py = i32(tex_dims.y - 1.0 - tile_coord.y);

    // -----------------------------------------------------------------------
    // Step 2: Load center tile and neighbors using textureLoad (no filtering).
    // -----------------------------------------------------------------------

    let field_c     = load_field(px,     py,     tex_max);
    let field_l     = load_field(px - 1, py,     tex_max);  // left neighbor
    let field_r     = load_field(px + 1, py,     tex_max);  // right neighbor
    let field_above = load_field(px,     py - 1, tex_max);  // tile above in world = py-1 in tex
    let field_below = load_field(px,     py + 1, tex_max);  // tile below in world = py+1 in tex

    // Early discard: if center tile has no liquid, nothing to draw.
    let center_lvl = max_level(field_c);
    if center_lvl < MIN_LEVEL {
        discard;
    }

    // Discard isolated tiny drops — tiles with no liquid neighbors and
    // very low level just produce ugly scattered pixels.
    let any_neighbor = max(max_level(field_l), max(max_level(field_r),
                       max(max_level(field_above), max_level(field_below))));
    if any_neighbor < MIN_NEIGHBOR_LEVEL && center_lvl < ISOLATED_MIN_LEVEL {
        discard;
    }

    // -----------------------------------------------------------------------
    // Step 3: Per-channel liquid levels with horizontal blending.
    //
    // Blending rules:
    //   - Only blend between tiles that BOTH have liquid (prevents water
    //     appearing outside its boundaries)
    //   - Blending weight increases toward tile edges using smoothstep
    //   - Center tile always contributes with weight 1.0
    //   - Final level is a weighted average
    // -----------------------------------------------------------------------

    let has_l = step(MIN_NEIGHBOR_LEVEL, max_level(field_l));
    let has_r = step(MIN_NEIGHBOR_LEVEL, max_level(field_r));

    // Blend weights: ramp up toward left/right edges
    let raw_wl = smoothstep(H_BLEND_HALF, 0.0, frac.x);    // high at left edge
    let raw_wr = smoothstep(1.0 - H_BLEND_HALF, 1.0, frac.x);  // high at right edge

    // Gate on neighbor existence
    let wl = raw_wl * has_l;
    let wr = raw_wr * has_r;
    let wc = 1.0;
    let wt = wc + wl + wr;

    // Normalized blend weights
    let bl = wl / wt;
    let br = wr / wt;
    let bc = wc / wt;

    // Blended per-channel levels
    let water_lvl = field_c.r * bc + field_l.r * bl + field_r.r * br;
    let lava_lvl  = field_c.g * bc + field_l.g * bl + field_r.g * br;
    let oil_lvl   = field_c.b * bc + field_l.b * bl + field_r.b * br;

    // -----------------------------------------------------------------------
    // Step 4: Fill computation with per-edge rounding.
    //
    // Every non-submerged tile gets rounded at each exposed edge (where the
    // neighbor has no liquid of that type). This handles all cases uniformly:
    //   - Isolated drop: rounded on all 4 sides
    //   - Falling stream: rounded left+right, connected top+bottom
    //   - Spreading blob: rounded on whichever sides lack neighbors
    //   - Pool surface: rounded only at top (via water level smoothstep)
    //   - Submerged interior: fill = 1.0, no rounding
    // -----------------------------------------------------------------------

    let above_water = field_above.r > MIN_NEIGHBOR_LEVEL;
    let above_lava  = field_above.g > MIN_NEIGHBOR_LEVEL;
    let above_oil   = field_above.b > MIN_NEIGHBOR_LEVEL;

    let below_water = field_below.r > MIN_NEIGHBOR_LEVEL;
    let below_lava  = field_below.g > MIN_NEIGHBOR_LEVEL;
    let below_oil   = field_below.b > MIN_NEIGHBOR_LEVEL;

    let left_water = field_l.r > MIN_NEIGHBOR_LEVEL;
    let left_lava  = field_l.g > MIN_NEIGHBOR_LEVEL;
    let left_oil   = field_l.b > MIN_NEIGHBOR_LEVEL;

    let right_water = field_r.r > MIN_NEIGHBOR_LEVEL;
    let right_lava  = field_r.g > MIN_NEIGHBOR_LEVEL;
    let right_oil   = field_r.b > MIN_NEIGHBOR_LEVEL;

    // --- Edge rounding masks ---
    // Each exposed edge fades the fill smoothly near that edge.
    // EDGE_ROUND controls the rounding depth for left/right edges.
    // Bottom rounding scales with water level — low-level tiles get
    // heavily rounded bottoms so thin surface splashes look blobby.
    let EDGE_ROUND = 0.15;

    // Water edge masks
    var w_mask = step(MIN_NEIGHBOR_LEVEL, field_c.r);
    // Top: use water level smoothstep if no water above
    if !above_water {
        w_mask *= 1.0 - smoothstep(water_lvl - SURFACE_EDGE, water_lvl + SURFACE_EDGE, frac.y);
    }
    // Bottom: round only when falling (no liquid below AND missing at least
    // one side neighbor). Pool bottoms sitting on solid ground keep sharp edges.
    if !below_water && (!left_water || !right_water) {
        let w_bot_round = max(EDGE_ROUND, water_lvl * 0.5);
        w_mask *= smoothstep(0.0, w_bot_round, frac.y);
    }
    // Left: round if no water to the left
    if !left_water {
        w_mask *= smoothstep(0.0, EDGE_ROUND, frac.x);
    }
    // Right: round if no water to the right
    if !right_water {
        w_mask *= smoothstep(1.0, 1.0 - EDGE_ROUND, frac.x);
    }

    // Lava edge masks
    var l_mask = step(MIN_NEIGHBOR_LEVEL, field_c.g);
    if !above_lava {
        l_mask *= 1.0 - smoothstep(lava_lvl - SURFACE_EDGE, lava_lvl + SURFACE_EDGE, frac.y);
    }
    if !below_lava && (!left_lava || !right_lava) {
        let l_bot_round = max(EDGE_ROUND, lava_lvl * 0.5);
        l_mask *= smoothstep(0.0, l_bot_round, frac.y);
    }
    if !left_lava {
        l_mask *= smoothstep(0.0, EDGE_ROUND, frac.x);
    }
    if !right_lava {
        l_mask *= smoothstep(1.0, 1.0 - EDGE_ROUND, frac.x);
    }

    // Oil edge masks
    var o_mask = step(MIN_NEIGHBOR_LEVEL, field_c.b);
    if !above_oil {
        o_mask *= 1.0 - smoothstep(oil_lvl - SURFACE_EDGE, oil_lvl + SURFACE_EDGE, frac.y);
    }
    if !below_oil && (!left_oil || !right_oil) {
        let o_bot_round = max(EDGE_ROUND, oil_lvl * 0.5);
        o_mask *= smoothstep(0.0, o_bot_round, frac.y);
    }
    if !left_oil {
        o_mask *= smoothstep(0.0, EDGE_ROUND, frac.x);
    }
    if !right_oil {
        o_mask *= smoothstep(1.0, 1.0 - EDGE_ROUND, frac.x);
    }

    // --- Final fill ---
    // Always use edge-rounded mask. When a neighbor exists on that side,
    // the corresponding if-block above simply doesn't apply its rounding,
    // so the mask naturally stays 1.0 on connected edges.
    let water_fill = w_mask;
    let lava_fill  = l_mask;
    let oil_fill   = o_mask;

    // -----------------------------------------------------------------------
    // Step 5: Voronoi caustic highlights near the liquid surface.
    //
    // Caustics only on pool surface tiles — tiles that have at least one
    // horizontal neighbor (part of a pool body), are not submerged, and
    // are not fully isolated drops.
    // -----------------------------------------------------------------------

    let vor = voronoi(in.world_pos * VORONOI_FREQ, uniforms.time);
    let caustic = smoothstep(CAUSTIC_EDGE_LO, CAUSTIC_EDGE_HI, vor.y - vor.x) * CAUSTIC_INTENSITY;

    // Caustic only on pool surfaces: must have at least one side neighbor
    // and must not be submerged.
    let water_has_side = left_water || right_water;
    let lava_has_side  = left_lava  || right_lava;

    let is_water_pool_surface = select(1.0, 0.0, above_water || !water_has_side);
    let is_lava_pool_surface  = select(1.0, 0.0, above_lava  || !lava_has_side);

    let dist_to_water_surface = water_lvl - frac.y;
    let water_caustic_mask = smoothstep(SURFACE_BAND, 0.0, dist_to_water_surface)
                           * smoothstep(-SURFACE_EDGE, SURFACE_EDGE * 2.0, dist_to_water_surface)
                           * is_water_pool_surface;

    let dist_to_lava_surface = lava_lvl - frac.y;
    let lava_caustic_mask = smoothstep(SURFACE_BAND, 0.0, dist_to_lava_surface)
                          * smoothstep(-SURFACE_EDGE, SURFACE_EDGE * 2.0, dist_to_lava_surface)
                          * is_lava_pool_surface;

    // -----------------------------------------------------------------------
    // Step 6: Composite liquid colors.
    // -----------------------------------------------------------------------

    var color = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    if water_fill > 0.001 {
        let wc = uniforms.water_color;
        let highlight = caustic * water_caustic_mask;
        color = mix(color, vec4<f32>(wc.rgb + vec3<f32>(highlight), wc.a), water_fill);
    }

    if lava_fill > 0.001 {
        let lc = uniforms.lava_color;
        let highlight = caustic * lava_caustic_mask * 1.5;
        // Lava highlights are orange-biased
        color = mix(color, vec4<f32>(lc.rgb + vec3<f32>(highlight * 0.8, highlight * 0.4, 0.0), lc.a), lava_fill);
    }

    if oil_fill > 0.001 {
        // Oil has no caustic highlights (opaque, dark liquid)
        color = mix(color, uniforms.oil_color, oil_fill);
    }

    if color.a < 0.01 {
        discard;
    }

    // -----------------------------------------------------------------------
    // Step 7: Apply lightmap.
    // -----------------------------------------------------------------------

    let lm_uv = in.world_pos * lm_xform.scale + lm_xform.offset;
    let light = textureSample(lightmap_texture, lightmap_sampler, lm_uv).rgb;
    color = vec4<f32>(color.rgb * light, color.a);

    return color;
}
