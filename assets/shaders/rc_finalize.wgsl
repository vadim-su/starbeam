// Radiance Cascades 2D — Finalize compute shader
//
// Extracts irradiance from cascade 0 into the final lightmap.
// Cascade 0 has 1 probe per pixel and 4 directions packed as 2×2 subtexels.
//
// A 3×3 spatial blur averages 9 neighboring probes, each with a different
// angular jitter offset. This yields 9×4 = 36 effective directional samples,
// producing smooth circular light spread from point sources without
// increasing cascade 0's direction count.

struct FinalizeUniforms {
    input_size: vec2<u32>,
    viewport_offset: vec2<u32>,
    viewport_size: vec2<u32>,
    _pad: vec2<u32>,
}

@group(0) @binding(0) var<uniform> uniforms: FinalizeUniforms;
@group(0) @binding(1) var cascade_0: texture_2d<f32>;
@group(0) @binding(2) var lightmap_out: texture_storage_2d<rgba16float, write>;

const N_DIRS: u32 = 4u;
const DIRS_SIDE: u32 = 2u;

/// Read the average radiance of a single probe (all 4 directions).
fn probe_radiance(ix: i32, iy: i32) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    for (var d = 0u; d < N_DIRS; d++) {
        let dir_x = i32(d % DIRS_SIDE);
        let dir_y = i32(d / DIRS_SIDE);
        let rx = ix * i32(DIRS_SIDE) + dir_x;
        let ry = iy * i32(DIRS_SIDE) + dir_y;
        sum += textureLoad(cascade_0, vec2<i32>(rx, ry), 0).rgb;
    }
    return sum / f32(N_DIRS);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let px = gid.x;
    let py = gid.y;

    if px >= uniforms.viewport_size.x || py >= uniforms.viewport_size.y {
        return;
    }

    // Map viewport pixel to input-space probe index.
    let ix = i32(px + uniforms.viewport_offset.x);
    let iy = i32(py + uniforms.viewport_offset.y);
    let max_ix = i32(uniforms.input_size.x) - 1;
    let max_iy = i32(uniforms.input_size.y) - 1;

    // 3×3 weighted blur: center=4, cardinal=2, diagonal=1 (total=16).
    // Each neighbor probe has a different angular jitter, so 9 probes
    // effectively sample 36 directions — much smoother than 4.
    var total = probe_radiance(ix, iy) * 4.0;
    total += probe_radiance(clamp(ix - 1, 0, max_ix), iy) * 2.0;
    total += probe_radiance(clamp(ix + 1, 0, max_ix), iy) * 2.0;
    total += probe_radiance(ix, clamp(iy - 1, 0, max_iy)) * 2.0;
    total += probe_radiance(ix, clamp(iy + 1, 0, max_iy)) * 2.0;
    total += probe_radiance(clamp(ix - 1, 0, max_ix), clamp(iy - 1, 0, max_iy)) * 1.0;
    total += probe_radiance(clamp(ix + 1, 0, max_ix), clamp(iy - 1, 0, max_iy)) * 1.0;
    total += probe_radiance(clamp(ix - 1, 0, max_ix), clamp(iy + 1, 0, max_iy)) * 1.0;
    total += probe_radiance(clamp(ix + 1, 0, max_ix), clamp(iy + 1, 0, max_iy)) * 1.0;

    let irradiance = total / 16.0;

    textureStore(lightmap_out, vec2<i32>(i32(px), i32(py)), vec4<f32>(irradiance, 1.0));
}
