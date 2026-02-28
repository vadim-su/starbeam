// Radiance Cascades 2D — Raymarch + Merge compute shader
//
// Dispatched once per cascade, from highest (N-1) down to 0.
// Each thread processes one probe and iterates over all its directions.
//
// Cascade parameters (branching factor B=4):
//   num_directions(n) = 4^(n+1)          → 4, 16, 64, 256, ...
//   probe_spacing(n)  = 2^n              → 1, 2, 4, 8, ...
//   interval_start(n) = 4^n (n>0), 0     → 0, 4, 16, 64, 256, ...
//   interval_end(n)   = 4^(n+1)          → 4, 16, 64, 256, 1024, ...
//
// Storage layout: each cascade texture packs directions into probe tiles.
//   dirs_side = sqrt(num_directions(n))
//   texel(probe_x * dirs_side + dir_x, probe_y * dirs_side + dir_y)

struct RcUniforms {
    input_size: vec2<u32>,       // 0..8
    cascade_index: u32,          // 8..12
    cascade_count: u32,          // 12..16
    viewport_offset: vec2<u32>,  // 16..24
    viewport_size: vec2<u32>,    // 24..32
    bounce_damping: f32,         // 32..36
    _pad0: f32,                  // 36..40
    grid_origin: vec2<i32>,      // 40..48  world-space origin (min_tx, min_ty)
    bounce_offset: vec2<i32>,    // 48..56  offset for lightmap_prev reads on grid snap
    _pad1: vec2<u32>,            // 56..64
}

@group(0) @binding(0) var<uniform> uniforms: RcUniforms;
@group(0) @binding(1) var density_map: texture_2d<f32>;
@group(0) @binding(2) var emissive_map: texture_2d<f32>;
@group(0) @binding(3) var albedo_map: texture_2d<f32>;
@group(0) @binding(4) var lightmap_prev: texture_2d<f32>;
@group(0) @binding(5) var cascade_read: texture_2d<f32>;
@group(0) @binding(6) var cascade_write: texture_storage_2d<rgba16float, write>;

const PI: f32 = 3.14159265359;
const BRANCHING: u32 = 4u;

// 4^exp via bit-shift: 4^n = 1 << (2n).
fn pow4(exp: u32) -> u32 {
    return 1u << (exp * 2u);
}

// Number of ray directions for cascade `n`: 4^(n+2).
// Using n+2 instead of n+1 gives 16 directions at cascade 0 (vs 4),
// dramatically improving point-light sampling in enclosed spaces.
fn num_directions(cascade: u32) -> u32 {
    return pow4(cascade + 2u);
}

// Probe spacing (pixels between probes) for cascade `n`: 2^n.
fn probe_spacing(cascade: u32) -> u32 {
    return 1u << cascade;
}

// Start distance (in pixels) of the ray interval for cascade `n`.
// Cascade 0 starts at 0; cascade n>0 starts at 4^n.
fn interval_start(cascade: u32) -> f32 {
    if cascade == 0u {
        return 0.0;
    }
    return f32(pow4(cascade));
}

// End distance (in pixels) of the ray interval for cascade `n`: 4^(n+1).
fn interval_end(cascade: u32) -> f32 {
    return f32(pow4(cascade + 1u));
}

// Bilinear interpolation of a cascade texture at fractional probe coordinates.
// Reads 4 neighboring probes for the given direction and blends them.
fn sample_cascade_bilinear(
    tex: texture_2d<f32>,
    probe_f: vec2<f32>,
    dir_idx: u32,
    dirs_side: u32,
    probes_w: u32,
    probes_h: u32,
) -> vec3<f32> {
    // Fractional probe position (probe_f is already in probe-space, 0-based)
    let floor_p = vec2<i32>(floor(probe_f));
    let frac = probe_f - floor(probe_f);

    let dir_x = dir_idx % dirs_side;
    let dir_y = dir_idx / dirs_side;

    var result = vec3<f32>(0.0);
    var total_weight = 0.0;

    // 2×2 bilinear neighborhood
    for (var oy = 0i; oy <= 1i; oy++) {
        for (var ox = 0i; ox <= 1i; ox++) {
            let px = floor_p.x + ox;
            let py = floor_p.y + oy;

            // Skip out-of-bounds probes
            if px < 0 || py < 0 || px >= i32(probes_w) || py >= i32(probes_h) {
                continue;
            }

            let weight = mix(1.0 - frac.x, frac.x, f32(ox))
                       * mix(1.0 - frac.y, frac.y, f32(oy));

            let read_x = u32(px) * dirs_side + dir_x;
            let read_y = u32(py) * dirs_side + dir_y;
            let val = textureLoad(tex, vec2<i32>(i32(read_x), i32(read_y)), 0).rgb;

            result += val * weight;
            total_weight += weight;
        }
    }

    if total_weight > 0.0 {
        return result / total_weight;
    }
    return vec3<f32>(0.0);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let cascade = uniforms.cascade_index;
    let spacing = probe_spacing(cascade);
    let n_dirs = num_directions(cascade);
    let ray_start = interval_start(cascade);
    let ray_end = interval_end(cascade);

    // Probe grid dimensions for this cascade
    let probes_w = uniforms.input_size.x / spacing;
    let probes_h = uniforms.input_size.y / spacing;

    let probe_x = gid.x;
    let probe_y = gid.y;

    if probe_x >= probes_w || probe_y >= probes_h {
        return;
    }

    // Probe center in input-texture pixel coordinates
    let probe_center = vec2<f32>(
        (f32(probe_x) + 0.5) * f32(spacing),
        (f32(probe_y) + 0.5) * f32(spacing),
    );

    let dirs_side = u32(sqrt(f32(n_dirs)));
    let input_size = uniforms.input_size;

    for (var dir_idx = 0u; dir_idx < n_dirs; dir_idx++) {
        // With 16+ directions per probe, uniform angular spacing provides
        // sufficient coverage. No per-probe jitter needed — this keeps
        // lighting fully deterministic and stable across camera movement.
        let angle = (f32(dir_idx) + 0.5) / f32(n_dirs) * 2.0 * PI;
        let ray_dir = vec2<f32>(cos(angle), sin(angle));

        var radiance = vec3<f32>(0.0);
        var hit = false;

        // Raymarch through the interval [ray_start, ray_end)
        let max_steps = u32(ray_end - ray_start) + 1u;
        for (var step = 0u; step < max_steps; step++) {
            let dist = ray_start + f32(step);
            if dist >= ray_end {
                break;
            }

            // Skip self-sample: at dist=0 the ray samples its own tile,
            // causing solid tiles to always self-hit and appear black.
            if cascade == 0u && dist < 0.5 {
                continue;
            }

            let sample_pos = probe_center + ray_dir * dist;
            let sample_px = vec2<i32>(sample_pos);

            // Out of bounds — ray escaped the input grid.
            if sample_px.x < 0 || sample_px.y < 0 ||
               sample_px.x >= i32(input_size.x) ||
               sample_px.y >= i32(input_size.y) {
                // For the highest cascade, rays that escape upward (y < 0)
                // have reached the sky. Return sun color instead of black to
                // prevent view-dependent shadow artifacts from boundary
                // truncation. Lower cascades handle this via merge with
                // upper cascades that already have correct sky values.
                if cascade == uniforms.cascade_count - 1u && sample_px.y < 0 {
                    radiance = vec3<f32>(1.0, 0.98, 0.9); // SUN_COLOR
                    hit = true;
                }
                break;
            }

            let density = textureLoad(density_map, sample_px, 0).r;

            if density > 0.5 {
                // Hit a solid surface
                let emissive = textureLoad(emissive_map, sample_px, 0).rgb;
                let albedo = textureLoad(albedo_map, sample_px, 0).rgb;

                // Bounce light: read previous frame's lightmap at hit position.
                // Lightmap is input-sized. When the grid origin shifts (snap),
                // bounce_offset corrects the lookup into lightmap_prev which
                // was written with the previous frame's grid origin.
                let bounce_px = sample_px + uniforms.bounce_offset;
                var reflected = vec3<f32>(0.0);
                if bounce_px.x >= 0 && bounce_px.y >= 0 &&
                   bounce_px.x < i32(input_size.x) && bounce_px.y < i32(input_size.y) {
                    let prev_light = textureLoad(lightmap_prev, bounce_px, 0).rgb;
                    reflected = prev_light * albedo * uniforms.bounce_damping;
                }

                radiance = emissive + reflected;
                hit = true;
                break;
            }

            // Check for emissive air (sun edge emitters, lava glow, etc.)
            let air_emissive = textureLoad(emissive_map, sample_px, 0).rgb;
            let air_brightness = air_emissive.r + air_emissive.g + air_emissive.b;
            if air_brightness > 0.001 {
                radiance = air_emissive;
                hit = true;
                break;
            }
        }

        // If no hit and not the highest cascade, merge with upper cascade (N+1)
        if !hit && cascade < uniforms.cascade_count - 1u {
            let upper_cascade = cascade + 1u;
            let upper_spacing = probe_spacing(upper_cascade);
            let upper_n_dirs = num_directions(upper_cascade);
            let upper_dirs_side = u32(sqrt(f32(upper_n_dirs)));
            let upper_probes_w = uniforms.input_size.x / upper_spacing;
            let upper_probes_h = uniforms.input_size.y / upper_spacing;

            // Map probe center to upper cascade's probe-space (fractional)
            let upper_probe_f = probe_center / f32(upper_spacing) - 0.5;

            // Each lower direction maps to B (=4) upper directions.
            // Average over the corresponding group of 4 upper directions.
            let group_size = upper_n_dirs / n_dirs; // = BRANCHING = 4
            let base_upper_dir = dir_idx * group_size;

            var merged = vec3<f32>(0.0);
            for (var g = 0u; g < group_size; g++) {
                let upper_dir = base_upper_dir + g;
                merged += sample_cascade_bilinear(
                    cascade_read,
                    upper_probe_f,
                    upper_dir,
                    upper_dirs_side,
                    upper_probes_w,
                    upper_probes_h,
                );
            }
            radiance = merged / f32(group_size);
        }

        // Write radiance to cascade storage texture
        let write_dir_x = dir_idx % dirs_side;
        let write_dir_y = dir_idx / dirs_side;
        let write_x = probe_x * dirs_side + write_dir_x;
        let write_y = probe_y * dirs_side + write_dir_y;

        textureStore(
            cascade_write,
            vec2<i32>(i32(write_x), i32(write_y)),
            vec4<f32>(radiance, 1.0),
        );
    }
}
