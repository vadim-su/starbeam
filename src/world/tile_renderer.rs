use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dKey};

use crate::world::mesh_builder::ATTRIBUTE_LIGHT;

#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct TileMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub atlas: Handle<Image>,
    #[uniform(2)]
    pub dim: f32,
}

impl Material2d for TileMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/tile.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/tile.wgsl".into()
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
            ATTRIBUTE_LIGHT.at_shader_location(2),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

/// Shared material handles for foreground (full brightness) and background (dimmed) layers.
#[derive(Resource)]
pub struct SharedTileMaterial {
    pub fg: Handle<TileMaterial>,
    pub bg: Handle<TileMaterial>,
}
