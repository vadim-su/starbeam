#import bevy_sprite::mesh2d_functions as mesh_functions
#import bevy_sprite::mesh2d_view_bindings::view

struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world_pos: vec2<f32>,
}

// Field order MUST match FluidUniformData in systems.rs exactly.
struct FluidUniforms {
    lightmap_uv_rect: vec4<f32>,
    time: f32,
    tile_size: f32,
    chunk_size: f32,
    threshold: f32,
    radius_min: f32,
    radius_max: f32,
    _pad0: f32,
    _pad1: f32,
    fluid_color_0: vec4<f32>,
    fluid_color_1: vec4<f32>,
    fluid_color_2: vec4<f32>,
    fluid_color_3: vec4<f32>,
    fluid_color_4: vec4<f32>,
    fluid_color_5: vec4<f32>,
    fluid_color_6: vec4<f32>,
    fluid_color_7: vec4<f32>,
    fluid_emission_0: vec4<f32>,
    fluid_emission_1: vec4<f32>,
}

@group(2) @binding(0) var density_texture: texture_2d<f32>;
@group(2) @binding(1) var density_sampler: sampler;
@group(2) @binding(2) var fluid_id_texture: texture_2d<f32>;
@group(2) @binding(3) var fluid_id_sampler: sampler;
@group(2) @binding(4) var lightmap_texture: texture_2d<f32>;
@group(2) @binding(5) var lightmap_sampler: sampler;
@group(2) @binding(6) var<uniform> uniforms: FluidUniforms;

// ------------------------------------------------------------------ //
// Vertex shader
// ------------------------------------------------------------------ //

@vertex
fn vertex(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);
    out.clip_position = mesh_functions::mesh2d_position_local_to_clip(
        world_from_local, vec4<f32>(in.position, 1.0),
    );
    out.uv = in.uv;
    out.world_pos = (world_from_local * vec4<f32>(in.position, 1.0)).xy;
    return out;
}

// ------------------------------------------------------------------ //
// Fluid color/emission lookup by ID
// ------------------------------------------------------------------ //

fn get_fluid_color(id_index: u32) -> vec4<f32> {
    switch id_index {
        case 0u: { return uniforms.fluid_color_0; }
        case 1u: { return uniforms.fluid_color_1; }
        case 2u: { return uniforms.fluid_color_2; }
        case 3u: { return uniforms.fluid_color_3; }
        case 4u: { return uniforms.fluid_color_4; }
        case 5u: { return uniforms.fluid_color_5; }
        case 6u: { return uniforms.fluid_color_6; }
        case 7u: { return uniforms.fluid_color_7; }
        default: { return vec4<f32>(0.0); }
    }
}

fn get_fluid_emission(id_index: u32) -> f32 {
    if id_index < 4u {
        return uniforms.fluid_emission_0[id_index];
    } else if id_index < 8u {
        return uniforms.fluid_emission_1[id_index - 4u];
    }
    return 0.0;
}

// ------------------------------------------------------------------ //
// Fragment shader — metaball density field evaluation
// ------------------------------------------------------------------ //

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let cs = uniforms.chunk_size;
    let tex_size = cs + 2.0;
    let texel = 1.0 / tex_size;

    // Map UV [0,1] to cell coordinates [0, chunk_size]
    let cell_coord = in.uv * cs;
    // Convert to texture UV with 1-texel padding offset
    let tex_uv = (cell_coord + 1.0) / tex_size;

    // Compute metaball field from 3x3 neighborhood
    var field: f32 = 0.0;
    var best_contribution: f32 = 0.0;
    var best_id: u32 = 0u;

    for (var dy: i32 = -1; dy <= 1; dy++) {
        for (var dx: i32 = -1; dx <= 1; dx++) {
            let sample_uv = tex_uv + vec2<f32>(f32(dx), f32(dy)) * texel;
            let mass = textureSample(density_texture, density_sampler, sample_uv).r;

            if mass > 0.002 {
                let neighbor_center = floor(cell_coord) + 0.5 + vec2<f32>(f32(dx), f32(dy));
                let diff = cell_coord - neighbor_center;
                let dist_sq = dot(diff, diff) + 0.001;

                let radius = uniforms.radius_min + (uniforms.radius_max - uniforms.radius_min) * mass;
                let r2 = radius * radius;
                let contribution = mass * r2 / dist_sq;
                field += contribution;

                if contribution > best_contribution {
                    best_contribution = contribution;
                    let raw_id = textureSample(fluid_id_texture, fluid_id_sampler, sample_uv).r;
                    best_id = u32(raw_id * 255.0 + 0.5);
                }
            }
        }
    }

    if field < uniforms.threshold {
        discard;
    }

    if best_id == 0u || best_id >= 8u {
        discard;
    }

    var color = get_fluid_color(best_id);

    // Emission glow
    let emission = get_fluid_emission(best_id);
    if emission > 0.0 {
        color = vec4<f32>(color.rgb * (1.0 + emission * 2.0), 1.0);
    }

    // Lightmap
    let lm_scale = uniforms.lightmap_uv_rect.xy;
    let lm_offset = uniforms.lightmap_uv_rect.zw;
    let lm_uv = in.world_pos * lm_scale + lm_offset;
    let light = textureSample(lightmap_texture, lightmap_sampler, lm_uv).rgb;

    if emission <= 0.0 {
        color = vec4<f32>(color.rgb * light, color.a);
    }

    return color;
}
