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

struct SpriteUvRect {
    scale: vec2<f32>,
    offset: vec2<f32>,
}

@group(2) @binding(5) var<uniform> sprite_rect: SpriteUvRect;

// Submersion tint: (r, g, b, strength).
// When strength > 0, simulates looking through liquid: applies a subtle
// multiplicative hue shift that matches the liquid color without killing
// brightness. Think of it as looking through tinted glass.
@group(2) @binding(6) var<uniform> submerge_tint: vec4<f32>;

// Interaction highlight: (r, g, b, strength).
// When active, transparent pixels adjacent to opaque ones glow with (r,g,b)
// creating a back-lit outline effect.
@group(2) @binding(7) var<uniform> highlight: vec4<f32>;

// Check if any neighboring texel (4-directional, 1px offset) is opaque.
fn has_opaque_neighbor(uv: vec2<f32>, texel: vec2<f32>) -> bool {
    let offsets = array<vec2<f32>, 4>(
        vec2<f32>( 1.0,  0.0),
        vec2<f32>(-1.0,  0.0),
        vec2<f32>( 0.0,  1.0),
        vec2<f32>( 0.0, -1.0),
    );
    for (var i = 0u; i < 4u; i++) {
        let neighbor_uv = uv + offsets[i] * texel;
        let a = textureSample(sprite_texture, sprite_sampler, neighbor_uv).a;
        if a > 0.1 {
            return true;
        }
    }
    return false;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Handle flipped UVs from negative Transform.scale.x (flip_x).
    // When scale.x < 0, uv.x arrives mirrored; abs() corrects it so the
    // texture always samples in [0,1]. The world_pos is already correct.
    let base_uv = vec2<f32>(abs(in.uv.x), in.uv.y);
    let uv = base_uv * sprite_rect.scale + sprite_rect.offset;

    let color = textureSample(sprite_texture, sprite_sampler, uv);

    // Outline highlight: transparent pixels next to opaque ones glow.
    if highlight.w > 0.0 && color.a < 0.01 {
        let tex_size = vec2<f32>(textureDimensions(sprite_texture));
        let texel = sprite_rect.scale / tex_size;

        if has_opaque_neighbor(uv, texel) {
            // Scale outline brightness by the lightmap so it stays dim in darkness.
            let lightmap_uv = in.world_pos * lm_xform.scale + lm_xform.offset;
            let light = textureSample(lightmap_texture, lightmap_sampler, lightmap_uv).rgb;
            let outline_brightness = max(light.r, max(light.g, light.b));
            return vec4<f32>(highlight.xyz * outline_brightness, highlight.w);
        }
    }

    if color.a < 0.01 {
        discard;
    }

    // Sample lightmap at world position (same transform as tile shader)
    let lightmap_uv = in.world_pos * lm_xform.scale + lm_xform.offset;
    let light = textureSample(lightmap_texture, lightmap_sampler, lightmap_uv).rgb;

    var lit = color.rgb * light;

    // Submersion: multiplicative tint simulating view through liquid.
    // The tint color is normalized (max channel = 1.0), so blue channel
    // stays at full strength while red/green are reduced — giving a hue
    // shift without overall darkening.
    if submerge_tint.w > 0.0 {
        let tint = mix(vec3<f32>(1.0), submerge_tint.xyz, submerge_tint.w);
        lit = lit * tint;
    }

    return vec4<f32>(lit, color.a);
}
