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

@vertex
fn vertex(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);
    out.clip_position = mesh_functions::mesh2d_position_local_to_clip(
        world_from_local,
        vec4<f32>(in.position, 1.0),
    );
    out.uv = in.uv;
    // Pass world position directly — stable, avoids clip->NDC->world precision loss.
    out.world_pos = (world_from_local * vec4<f32>(in.position, 1.0)).xy;
    return out;
}

// Pre-computed affine transform: world position -> lightmap UV.
// lightmap_uv = world_pos * lm_xform.xy + lm_xform.zw
// Same format as TileMaterial's lightmap_uv_rect.
struct LightmapXform {
    scale: vec2<f32>,
    offset: vec2<f32>,
}

@group(2) @binding(0) var sprite_texture: texture_2d<f32>;
@group(2) @binding(1) var sprite_sampler: sampler;
@group(2) @binding(2) var lightmap_texture: texture_2d<f32>;
@group(2) @binding(3) var lightmap_sampler: sampler;
@group(2) @binding(4) var<uniform> lm_xform: LightmapXform;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Handle flipped UVs from negative Transform.scale.x (flip_x).
    // When scale.x < 0, uv.x arrives mirrored; abs() corrects it so the
    // texture always samples in [0,1]. The world_pos is already correct.
    let uv = vec2<f32>(abs(in.uv.x), in.uv.y);

    let color = textureSample(sprite_texture, sprite_sampler, uv);
    if color.a < 0.01 {
        discard;
    }

    // Sample lightmap at world position (same transform as tile shader)
    let lightmap_uv = in.world_pos * lm_xform.scale + lm_xform.offset;
    let light = textureSample(lightmap_texture, lightmap_sampler, lightmap_uv).rgb;

    return vec4<f32>(color.rgb * light, color.a);
}
