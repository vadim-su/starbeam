// Radiance Cascades 2D — Finalize compute shader
//
// Extracts irradiance from cascade 0 into the final lightmap.
// Cascade 0 has 1 probe per pixel and 4 directions packed as 2×2 subtexels.
// For each viewport pixel, sum the 4 directional radiance values and average.

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

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let px = gid.x;
    let py = gid.y;

    if px >= uniforms.viewport_size.x || py >= uniforms.viewport_size.y {
        return;
    }

    // Map viewport pixel to input-space probe index.
    // Cascade 0 has probe_spacing=1, so probe index = input pixel coordinate.
    let input_x = px + uniforms.viewport_offset.x;
    let input_y = py + uniforms.viewport_offset.y;

    // Sum radiance from all 4 directions in cascade 0.
    // Directions are packed as a 2×2 block per probe in the cascade texture.
    var total_radiance = vec3<f32>(0.0);
    for (var d = 0u; d < N_DIRS; d++) {
        let dir_x = d % DIRS_SIDE;
        let dir_y = d / DIRS_SIDE;
        let read_x = input_x * DIRS_SIDE + dir_x;
        let read_y = input_y * DIRS_SIDE + dir_y;
        total_radiance += textureLoad(cascade_0, vec2<i32>(i32(read_x), i32(read_y)), 0).rgb;
    }

    let irradiance = total_radiance / f32(N_DIRS);

    textureStore(lightmap_out, vec2<i32>(i32(px), i32(py)), vec4<f32>(irradiance, 1.0));
}
