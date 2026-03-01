use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, PrimitiveTopology, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dKey};

/// Material for sprites affected by the RC lightmap.
///
/// Works like `TileMaterial` but for individual sprite textures (player,
/// dropped items, etc.) instead of the tile atlas. The shader multiplies
/// the sprite colour by the lightmap value at the entity's world position.
#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct LitSpriteMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub sprite: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    pub lightmap: Handle<Image>,
    #[uniform(4)]
    pub lightmap_uv_rect: Vec4, // (scale_x, scale_y, offset_x, offset_y)
    /// Sprite sheet sub-region: (scale_x, scale_y, offset_x, offset_y).
    /// Default (1,1,0,0) = full texture. For sprite sheets, scale = frame size / sheet size,
    /// offset = frame position in normalized coords.
    #[uniform(5)]
    pub sprite_uv_rect: Vec4,
}

impl Material2d for LitSpriteMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/lit_sprite.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/lit_sprite.wgsl".into()
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
        // Disable backface culling so negative Transform.scale.x (flip) works
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// Shared unit quad mesh used by all lit sprites.
/// A 1×1 quad centered at origin; scale via `Transform.scale` to set pixel size.
#[derive(Resource)]
pub struct SharedLitQuad(pub Handle<Mesh>);

/// Marker component for entities rendered with `LitSpriteMaterial`.
/// Used to query/identify lit-sprite entities (player, dropped items, etc.).
#[derive(Component)]
pub struct LitSprite;

/// Fallback 1×1 white lightmap handle, used before the RC pipeline produces output.
#[derive(Resource)]
pub struct FallbackLightmap(pub Handle<Image>);

/// Fallback 1×1 coloured image for dropped items that lack an icon.
#[derive(Resource)]
pub struct FallbackItemImage(pub Handle<Image>);

/// Build a unit quad mesh (1×1, centered at origin) with UVs for sprite rendering.
fn create_unit_quad() -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [-0.5, -0.5, 0.0],
            [0.5, -0.5, 0.0],
            [0.5, 0.5, 0.0],
            [-0.5, 0.5, 0.0],
        ],
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![
            [0.0, 1.0], // bottom-left
            [1.0, 1.0], // bottom-right
            [1.0, 0.0], // top-right
            [0.0, 0.0], // top-left
        ],
    )
    .with_inserted_indices(Indices::U16(vec![0, 1, 2, 0, 2, 3]))
}

/// System: create shared resources for lit sprites (runs once on InGame enter).
pub fn init_lit_sprite_resources(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
) {
    // Shared unit quad mesh
    let quad = meshes.add(create_unit_quad());
    commands.insert_resource(SharedLitQuad(quad));

    // Fallback 1×1 white lightmap (Rgba16Float, 1.0 in all channels)
    // Matches the format used by the RC pipeline lightmap.
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let white_lightmap = images.add(Image::new_fill(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        // f16 1.0 = 0x3C00 → little-endian [0x00, 0x3C] per channel
        &[0x00u8, 0x3C, 0x00, 0x3C, 0x00, 0x3C, 0x00, 0x3C],
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    ));
    commands.insert_resource(FallbackLightmap(white_lightmap));

    // Fallback 1×1 coloured image for items without an icon (warm brown).
    let fallback_item = images.add(Image::new_fill(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[204u8, 153, 51, 255], // sRGB ~(0.8, 0.6, 0.2, 1.0)
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    ));
    commands.insert_resource(FallbackItemImage(fallback_item));
}
