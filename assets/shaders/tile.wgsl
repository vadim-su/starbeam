#import bevy_sprite::mesh2d_vertex_output::VertexOutput

@group(2) @binding(0) var atlas_texture: texture_2d<f32>;
@group(2) @binding(1) var atlas_sampler: sampler;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(atlas_texture, atlas_sampler, mesh.uv);
    if color.a < 0.01 {
        discard;
    }
    return color;
}
