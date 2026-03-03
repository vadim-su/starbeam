#import bevy_sprite::mesh2d_functions as mesh_functions

struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
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
    return out;
}

struct StarfieldUniforms {
    time: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@group(2) @binding(0) var<uniform> uniforms: StarfieldUniforms;

// --- sRGB → linear conversion ---
// The render pipeline applies sRGB encoding on output, so all colors
// must be in linear space. Without this, dark sRGB values like #06060e
// get double-gamma-encoded and appear washed out.
fn srgb_ch(c: f32) -> f32 {
    if c <= 0.04045 {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

fn srgb(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(srgb_ch(c.x), srgb_ch(c.y), srgb_ch(c.z));
}

// --- Hash helpers ---

fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + dot(p3, vec3<f32>(p3.y + 33.33, p3.z + 33.33, p3.x + 33.33));
    return fract((p3.x + p3.y) * p3.z);
}

fn hash22(p: vec2<f32>) -> vec2<f32> {
    let n = vec2<f32>(
        dot(p, vec2<f32>(127.1, 311.7)),
        dot(p, vec2<f32>(269.5, 183.3))
    );
    return fract(sin(n) * 43758.5453123);
}

// --- Star layer ---
// Matches the website JS starfield logic:
//   size: 0.2..2.0px  ->  radius 0.01..0.04 in UV cell space
//   alpha: 0.2..0.8
//   speed: 0.05..0.35  ->  scaled to UV/sec
//   twinkle: sin(time * (0.005..0.025) + offset) * 0.3 + 0.7
//   color: rgba(200, 220, 255, alpha)
fn star_layer(
    uv: vec2<f32>,
    time: f32,
    grid_scale: f32,
    speed: f32,
    density: f32,
    base_brightness: f32,
    twinkle_speed: f32,
    min_radius: f32,
    max_radius: f32,
) -> f32 {
    // Scroll downward
    var scrolled = uv;
    scrolled.y = scrolled.y - time * speed;

    let grid_uv = scrolled * grid_scale;
    let cell = floor(grid_uv);
    let local = fract(grid_uv) - 0.5;

    let r = hash22(cell);

    if r.x > density {
        return 0.0;
    }

    // Star position within cell
    let star_pos = (r - 0.5) * 0.7;
    let d = length(local - star_pos);

    // Star radius
    let radius = mix(min_radius, max_radius, hash21(cell + 100.0));

    // Smooth circle shape
    let star = smoothstep(radius, radius * 0.2, d);

    // Twinkle — matches JS: sin(time * twinkleSpeed + offset) * 0.3 + 0.7
    let phase = hash21(cell + 200.0) * 6.2831;
    let freq = mix(0.8, 3.5, hash21(cell + 300.0)) * twinkle_speed;
    let twinkle = sin(time * freq + phase) * 0.3 + 0.7;

    // Per-star alpha variation (0.2..0.8 like the JS)
    let alpha = mix(0.2, 0.8, hash21(cell + 400.0)) * base_brightness;

    return star * twinkle * alpha;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let time = uniforms.time;

    // === Background ===
    // All colors converted from sRGB to linear via srgb() helper.
    // Website: --bg-deep: #06060e, --bg-dark: #0a0a18
    let bg_deep = srgb(vec3<f32>(0.024, 0.024, 0.055));  // #06060e
    let bg_dark = srgb(vec3<f32>(0.039, 0.039, 0.094));   // #0a0a18
    let bg = mix(bg_dark, bg_deep, uv.y);

    // === Central blue glow ===
    // radial-gradient(ellipse 80% 60% at 50% 40%, rgba(92,184,255,0.06), transparent 70%)
    let glow_center = vec2<f32>(0.5, 0.4);
    let glow_uv = (uv - glow_center) * vec2<f32>(1.0 / 0.8, 1.0 / 0.6);
    let glow_dist = length(glow_uv);
    let blue_glow = smoothstep(0.7, 0.0, glow_dist) * 0.06;
    let accent_blue = srgb(vec3<f32>(0.361, 0.722, 1.0));  // #5cb8ff

    // Subtle warm glow at lower-left:
    // radial-gradient(ellipse 60% 40% at 30% 70%, rgba(255,140,66,0.04), transparent 60%)
    let warm_center = vec2<f32>(0.3, 0.7);
    let warm_uv = (uv - warm_center) * vec2<f32>(1.0 / 0.6, 1.0 / 0.4);
    let warm_dist = length(warm_uv);
    let warm_glow = smoothstep(0.6, 0.0, warm_dist) * 0.04;
    let accent_warm = srgb(vec3<f32>(1.0, 0.549, 0.259));  // #ff8c42

    var color = bg + accent_blue * blue_glow + accent_warm * warm_glow;

    // === Stars ===
    var stars = 0.0;

    // Layer 1: Distant tiny stars — very small, slow, dense, dim, fast twinkle
    stars += star_layer(uv, time, 50.0, 0.003, 0.40, 0.35, 3.0, 0.008, 0.018);

    // Layer 2: Medium stars — small, moderate speed
    stars += star_layer(uv, time, 30.0, 0.008, 0.28, 0.55, 1.5, 0.012, 0.025);

    // Layer 3: Front stars — still small, slightly faster, sparse
    stars += star_layer(uv, time, 22.0, 0.015, 0.14, 0.70, 1.0, 0.015, 0.030);

    // Star color: rgba(200, 220, 255) from JS — slight blue tint
    let star_color = srgb(vec3<f32>(0.784, 0.863, 1.0));  // rgb(200,220,255)
    color = color + star_color * stars;

    return vec4<f32>(color, 1.0);
}
