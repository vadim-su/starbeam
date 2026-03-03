use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dKey};

/// Custom material for the animated starfield background.
#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct StarfieldMaterial {
    #[uniform(0)]
    pub time: f32,
    #[uniform(0)]
    pub _pad0: f32,
    #[uniform(0)]
    pub _pad1: f32,
    #[uniform(0)]
    pub _pad2: f32,
}

impl StarfieldMaterial {
    pub fn new() -> Self {
        Self {
            time: 0.0,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        }
    }
}

impl Material2d for StarfieldMaterial {
    fn vertex_shader() -> ShaderRef {
        "engine/shaders/starfield.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "engine/shaders/starfield.wgsl".into()
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

/// Resource holding the handle to the starfield material for updates.
#[derive(Resource)]
pub struct StarfieldMaterialHandle(pub Handle<StarfieldMaterial>);

/// System: update the starfield shader time uniform each frame.
pub fn update_starfield_time(
    time: Res<Time>,
    handle: Option<Res<StarfieldMaterialHandle>>,
    mut materials: ResMut<Assets<StarfieldMaterial>>,
) {
    let Some(handle) = handle else { return };
    if let Some(mat) = materials.get_mut(&handle.0) {
        mat.time = time.elapsed_secs();
    }
}
