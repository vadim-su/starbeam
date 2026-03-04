use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dKey};

/// Material for rendering fluid meshes with fill-level clipping and surface waves.
#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct FluidMaterial {
    /// Elapsed game time for wave animation.
    #[uniform(0)]
    pub time: f32,
    /// Lightmap texture from RC lighting.
    #[texture(1)]
    #[sampler(2)]
    pub lightmap: Handle<Image>,
    /// Lightmap UV transform: (scale_x, scale_y, offset_x, offset_y).
    #[uniform(3)]
    pub lightmap_uv_rect: Vec4,
}

impl Material2d for FluidMaterial {
    fn vertex_shader() -> ShaderRef {
        "engine/shaders/fluid.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "engine/shaders/fluid.wgsl".into()
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
            Mesh::ATTRIBUTE_COLOR.at_shader_location(2),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

/// Shared material handle for all fluid chunk meshes.
#[derive(Resource)]
pub struct SharedFluidMaterial(pub Handle<FluidMaterial>);
