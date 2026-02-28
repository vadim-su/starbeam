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

struct TileUniforms {
    dim: f32,
}

// Constant lightmap parameters (don't change during smooth camera movement).
// The actual camera position is read from view.world_position (always current frame).
struct LightmapParams {
    viewport_tiles: vec2<f32>,  // (vp_tiles_w, vp_tiles_h)
    tile_size_pad: vec2<f32>,   // (tile_size, 0.0)
}

@group(2) @binding(0) var atlas_texture: texture_2d<f32>;
@group(2) @binding(1) var atlas_sampler: sampler;
@group(2) @binding(2) var<uniform> uniforms: TileUniforms;
@group(2) @binding(3) var lightmap_texture: texture_2d<f32>;
@group(2) @binding(4) var lightmap_sampler: sampler;
@group(2) @binding(5) var<uniform> lm_params: LightmapParams;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(atlas_texture, atlas_sampler, in.uv);
    if color.a < 0.01 {
        if uniforms.dim < 1.0 {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
        discard;
    }

    // --- Compute lightmap UV from world position ---
    // Uses view.world_from_clip (guaranteed current-frame camera data) to
    // derive the world position, then maps to lightmap UV using constant
    // viewport parameters. This avoids any frame-lag from CPU-side uniforms.
    let screen_uv = (in.clip_position.xy - view.viewport.xy) / view.viewport.zw;

    // Screen UV → NDC (Y flipped: UV y=0 top → NDC y=+1 top)
    let ndc = screen_uv * vec2(2.0, -2.0) + vec2(-1.0, 1.0);

    // NDC → world position via inverse view-projection matrix
    let world_h = view.world_from_clip * vec4(ndc, 0.0, 1.0);
    let world_pos = world_h.xy / world_h.w;

    // Lightmap parameters (constant during smooth movement)
    let vp_tiles = lm_params.viewport_tiles;
    let ts = lm_params.tile_size_pad.x;

    // Camera tile coordinates from view uniform (current frame, no lag)
    let cam_tile_x = floor(view.world_position.x / ts);
    let cam_tile_y = floor(view.world_position.y / ts);
    let half_w = floor(vp_tiles.x / 2.0);
    let half_h = floor(vp_tiles.y / 2.0);

    // World position → lightmap UV
    // Lightmap texel (px, py) represents world tile:
    //   x: cam_tile_x - half_w + px
    //   y: cam_tile_y + half_h - py  (Y flipped: py=0 is top = highest world Y)
    let lightmap_uv = vec2(
        (world_pos.x / ts - cam_tile_x + half_w) / vp_tiles.x,
        (cam_tile_y + half_h + 1.0 - world_pos.y / ts) / vp_tiles.y,
    );

    let light = textureSample(lightmap_texture, lightmap_sampler, lightmap_uv).rgb;

    return vec4<f32>(color.rgb * light * uniforms.dim, color.a);
}
