#import bevy_sprite::mesh2d_functions as mesh_functions

// ---------------------------------------------------------------------------
// Unified-blob liquid shader
// ---------------------------------------------------------------------------
//
// Renders all liquid types as a single merged blob shape, then colors the
// interior based on per-type level ratios. This ensures different liquid
// types (water, oil, lava) visually merge at their boundaries instead of
// drawing as independent shapes with sharp discontinuities.
//
// Algorithm:
//   1. Horizontal blending produces per-type levels AND a combined level
//   2. One unified fill mask from the combined level (smooth across types)
//   3. Color from per-type level ratios + vertical blend zone at boundaries
//   4. Caustics applied to the unified surface
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

// Edge rounding depth for left/right/bottom edges exposed to air
const EDGE_ROUND: f32 = 0.15;

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
// Helpers
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

/// Return the dominant channel index: 0=water(R), 1=lava(G), 2=oil(B).
/// Returns -1 if no liquid present.
fn dominant_channel(field: vec4<f32>) -> i32 {
    let ml = max_level(field);
    if ml < MIN_LEVEL {
        return -1;
    }
    if field.r >= field.g && field.r >= field.b {
        return 0;
    }
    if field.g >= field.b {
        return 1;
    }
    return 2;
}

/// Compute weighted color from per-type levels.
/// Returns (color_rgb, dominant_channel_index).
fn level_weighted_color(w: f32, l: f32, o: f32) -> vec3<f32> {
    let total = w + l + o;
    if total < MIN_LEVEL {
        return uniforms.water_color.rgb;
    }
    let inv = 1.0 / total;
    return uniforms.water_color.rgb * (w * inv)
         + uniforms.lava_color.rgb  * (l * inv)
         + uniforms.oil_color.rgb   * (o * inv);
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
    let frac_v = fract(p);
    var d1 = 8.0;
    var d2 = 8.0;
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let neighbor = vec2<f32>(f32(i), f32(j));
            let point = hash2(cell + neighbor);
            let animated = neighbor + 0.5 + 0.4 * sin(t * VORONOI_SPEED + 6.2831 * point) - frac_v;
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
    // -----------------------------------------------------------------------

    let tile_in_field = (in.world_pos - uniforms.field_origin) / ts;
    let tile_coord = floor(tile_in_field);
    let frac = tile_in_field - tile_coord;  // sub-tile position, x and y in [0, 1)

    // Texture pixel coordinate: X maps directly, Y is flipped.
    let px = i32(tile_coord.x);
    let py = i32(tex_dims.y - 1.0 - tile_coord.y);

    // -----------------------------------------------------------------------
    // Step 2: Load center tile and neighbors using textureLoad.
    // -----------------------------------------------------------------------

    let field_c     = load_field(px,     py,     tex_max);
    let field_l     = load_field(px - 1, py,     tex_max);
    let field_r     = load_field(px + 1, py,     tex_max);
    let field_above = load_field(px,     py - 1, tex_max);  // tile above in world
    let field_below = load_field(px,     py + 1, tex_max);  // tile below in world

    // Early discard: no liquid in this tile.
    let center_lvl = max_level(field_c);
    if center_lvl < MIN_LEVEL {
        discard;
    }

    // Discard isolated tiny drops.
    let any_neighbor = max(max_level(field_l), max(max_level(field_r),
                       max(max_level(field_above), max_level(field_below))));
    if any_neighbor < MIN_NEIGHBOR_LEVEL && center_lvl < ISOLATED_MIN_LEVEL {
        discard;
    }

    // -----------------------------------------------------------------------
    // Step 3: Horizontal blending — per-type levels AND combined level.
    //
    // Per-type levels are used for color weighting. Combined level
    // (max_level of blended field) is used for the unified fill mask.
    // The combined level stays continuous at tile boundaries because
    // symmetric blend weights at x=0/x=1 yield the same value from
    // both sides.
    // -----------------------------------------------------------------------

    let has_l = step(MIN_NEIGHBOR_LEVEL, max_level(field_l));
    let has_r = step(MIN_NEIGHBOR_LEVEL, max_level(field_r));

    let raw_wl = smoothstep(H_BLEND_HALF, 0.0, frac.x);
    let raw_wr = smoothstep(1.0 - H_BLEND_HALF, 1.0, frac.x);

    let wl = raw_wl * has_l;
    let wr = raw_wr * has_r;
    let wc = 1.0;
    let wt = wc + wl + wr;

    let bl = wl / wt;
    let br = wr / wt;
    let bc = wc / wt;

    // Per-channel blended levels (for color weighting)
    let water_lvl = field_c.r * bc + field_l.r * bl + field_r.r * br;
    let lava_lvl  = field_c.g * bc + field_l.g * bl + field_r.g * br;
    let oil_lvl   = field_c.b * bc + field_l.b * bl + field_r.b * br;

    // Combined level: operates on max_level of each tile, blended the
    // same way. This is the unified blob height.
    let comb_lvl = max_level(field_c) * bc + max_level(field_l) * bl + max_level(field_r) * br;

    // -----------------------------------------------------------------------
    // Step 4: Unified fill mask using combined level.
    //
    // One fill shape for the entire liquid blob. Edge rounding only
    // happens at air boundaries. Where liquid (any type) exists on a
    // neighbor side, the fill stays connected (mask = 1.0 on that edge).
    // -----------------------------------------------------------------------

    let above_any = max_level(field_above) > MIN_NEIGHBOR_LEVEL;
    let below_any = max_level(field_below) > MIN_NEIGHBOR_LEVEL;
    let left_any  = max_level(field_l)     > MIN_NEIGHBOR_LEVEL;
    let right_any = max_level(field_r)     > MIN_NEIGHBOR_LEVEL;

    var fill = 1.0;

    // --- Top edge ---
    if !above_any {
        // Exposed to air: smooth surface rounding using combined level
        fill *= 1.0 - smoothstep(comb_lvl - SURFACE_EDGE, comb_lvl + SURFACE_EDGE, frac.y);
    }
    // If above_any (submerged by any liquid): fill stays 1.0 at top

    // --- Bottom edge ---
    // Round bottom only when exposed to air AND not part of a wide pool
    // (at least one side lacks liquid). Wide pools keep flat bottoms.
    if !below_any && (!left_any || !right_any) {
        let bot_round = max(EDGE_ROUND, comb_lvl * 0.5);
        fill *= smoothstep(0.0, bot_round, frac.y);
    }

    // --- Left edge ---
    if !left_any {
        fill *= smoothstep(0.0, EDGE_ROUND, frac.x);
    }

    // --- Right edge ---
    if !right_any {
        fill *= smoothstep(1.0, 1.0 - EDGE_ROUND, frac.x);
    }

    if fill < 0.001 {
        discard;
    }

    // -----------------------------------------------------------------------
    // Step 5: Color from per-type level ratios + vertical blend zone.
    //
    // Base color is the weighted mix of liquid colors proportional to
    // per-type levels in this tile. At vertical type boundaries (tile
    // above or below has a different dominant type), we blend toward
    // the neighbor's color over a VBLEND-height band.
    // -----------------------------------------------------------------------

    var base_rgb = level_weighted_color(water_lvl, lava_lvl, oil_lvl);

    let center_dom = dominant_channel(field_c);

    // -----------------------------------------------------------------------
    // Step 6: Voronoi caustic highlights on the unified surface.
    //
    // Caustics appear near the liquid surface for water and lava pool
    // tiles (tiles with at least one horizontal neighbor, not submerged).
    // Uses the unified combined level for surface distance.
    // -----------------------------------------------------------------------

    let vor = voronoi(in.world_pos * VORONOI_FREQ, uniforms.time);
    let caustic = smoothstep(CAUSTIC_EDGE_LO, CAUSTIC_EDGE_HI, vor.y - vor.x) * CAUSTIC_INTENSITY;

    let has_side = left_any || right_any;
    let is_pool_surface = select(1.0, 0.0, above_any || !has_side);

    let dist_to_surface = comb_lvl - frac.y;
    let caustic_mask = smoothstep(SURFACE_BAND, 0.0, dist_to_surface)
                     * smoothstep(-SURFACE_EDGE, SURFACE_EDGE * 2.0, dist_to_surface)
                     * is_pool_surface;

    // Apply caustic based on dominant type:
    // - Water: white highlight
    // - Lava: orange-biased highlight
    // - Oil: no caustics (opaque dark liquid)
    var highlight = vec3<f32>(0.0);
    if center_dom == 0 {
        // Water: uniform white highlight
        highlight = vec3<f32>(caustic * caustic_mask);
    } else if center_dom == 1 {
        // Lava: orange-biased highlight (brighter)
        let lava_caustic = caustic * caustic_mask * 1.5;
        highlight = vec3<f32>(lava_caustic * 0.8, lava_caustic * 0.4, 0.0);
    }
    // Oil (center_dom == 2): no highlight

    // -----------------------------------------------------------------------
    // Step 7: Composite final color.
    // -----------------------------------------------------------------------

    // Use max alpha from the liquid colors, weighted by type ratio
    let total_lvl = water_lvl + lava_lvl + oil_lvl;
    var base_alpha = 1.0;
    if total_lvl > MIN_LEVEL {
        let inv = 1.0 / total_lvl;
        base_alpha = uniforms.water_color.a * (water_lvl * inv)
                   + uniforms.lava_color.a  * (lava_lvl * inv)
                   + uniforms.oil_color.a   * (oil_lvl * inv);
    }

    var color = vec4<f32>(base_rgb + highlight, base_alpha * fill);

    if color.a < 0.01 {
        discard;
    }

    // -----------------------------------------------------------------------
    // Step 8: Apply lightmap.
    // -----------------------------------------------------------------------

    let lm_uv = in.world_pos * lm_xform.scale + lm_xform.offset;
    let light = textureSample(lightmap_texture, lightmap_sampler, lm_uv).rgb;
    color = vec4<f32>(color.rgb * light, color.a);

    return color;
}
