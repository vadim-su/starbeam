use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::sprite_render::Material2d;

#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct TileMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub atlas: Handle<Image>,
}

impl Material2d for TileMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/tile.wgsl".into()
    }
}

/// Shared material handle for all chunk entities
#[derive(Resource)]
pub struct SharedTileMaterial {
    pub handle: Handle<TileMaterial>,
}
