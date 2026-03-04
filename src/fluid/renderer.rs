use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dKey};

use super::cell::FluidCell;
use crate::world::mesh_builder::MeshBuildBuffers;

/// Material for fluid rendering: flat color + lightmap.
#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct FluidMaterial {
    #[uniform(0)]
    pub color: Vec4, // RGB color + padding
    #[uniform(0)]
    pub alpha: f32,
    #[texture(1)]
    #[sampler(2)]
    pub lightmap: Handle<Image>,
    #[uniform(3)]
    pub lightmap_uv_rect: Vec4,
}

impl Material2d for FluidMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/fluid.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/fluid.wgsl".into()
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

        // Enable alpha blending for fluid transparency
        if let Some(fragment) = descriptor.fragment.as_mut() {
            for target in &mut fragment.targets {
                if let Some(state) = target {
                    state.blend = Some(bevy::render::render_resource::BlendState::ALPHA_BLENDING);
                }
            }
        }

        Ok(())
    }
}

/// Shared material handle for the fluid layer.
#[derive(Resource)]
pub struct SharedFluidMaterial {
    pub handle: Handle<FluidMaterial>,
}

/// Build a mesh for the fluid layer of a chunk.
///
/// Each non-empty fluid cell becomes a quad whose height is proportional to
/// the fluid level (0–255). The quad is anchored at the bottom of the tile
/// and extends upward.
pub fn build_fluid_mesh(
    fluids: &[FluidCell],
    display_chunk_x: i32,
    chunk_y: i32,
    chunk_size: u32,
    tile_size: f32,
    buffers: &mut MeshBuildBuffers,
) -> Mesh {
    buffers.positions.clear();
    buffers.uvs.clear();
    buffers.indices.clear();

    let base_x = display_chunk_x * chunk_size as i32;
    let base_y = chunk_y * chunk_size as i32;

    for local_y in 0..chunk_size {
        for local_x in 0..chunk_size {
            let idx = (local_y * chunk_size + local_x) as usize;
            let cell = fluids[idx];

            if cell.is_empty() {
                continue;
            }

            let world_x = base_x + local_x as i32;
            let world_y = base_y + local_y as i32;

            let px = world_x as f32 * tile_size;
            let py = world_y as f32 * tile_size;

            // Height proportional to fluid level
            let height = tile_size * (cell.level as f32 / 255.0);

            let vi = buffers.positions.len() as u32;

            // Quad anchored at bottom of tile, extends up by `height`
            buffers.positions.extend_from_slice(&[
                [px, py, 0.0],                    // bottom-left
                [px + tile_size, py, 0.0],        // bottom-right
                [px + tile_size, py + height, 0.0], // top-right
                [px, py + height, 0.0],           // top-left
            ]);

            // UV.y: 0 at bottom, 1 at top surface (used by shader for gradient)
            buffers.uvs.extend_from_slice(&[
                [0.0, 0.0],
                [1.0, 0.0],
                [1.0, 1.0],
                [0.0, 1.0],
            ]);

            buffers
                .indices
                .extend_from_slice(&[vi, vi + 1, vi + 2, vi, vi + 2, vi + 3]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, buffers.positions.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, buffers.uvs.clone());
    mesh.insert_indices(Indices::U32(buffers.indices.clone()));
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::cell::FluidId;

    #[test]
    fn empty_fluids_produce_empty_mesh() {
        let fluids = vec![FluidCell::default(); 4];
        let mut buffers = MeshBuildBuffers::default();
        let mesh = build_fluid_mesh(&fluids, 0, 0, 2, 32.0, &mut buffers);
        assert_eq!(buffers.positions.len(), 0);
        assert!(mesh.indices().is_some());
    }

    #[test]
    fn full_fluid_produces_full_quad() {
        let fluids = vec![
            FluidCell {
                fluid_id: FluidId(1),
                level: 255,
            },
            FluidCell::default(),
            FluidCell::default(),
            FluidCell::default(),
        ];
        let mut buffers = MeshBuildBuffers::default();
        build_fluid_mesh(&fluids, 0, 0, 2, 32.0, &mut buffers);

        assert_eq!(buffers.positions.len(), 4);
        // Full tile: top should be at py + 32.0
        assert_eq!(buffers.positions[0], [0.0, 0.0, 0.0]);
        assert_eq!(buffers.positions[2], [32.0, 32.0, 0.0]);
    }

    #[test]
    fn half_fluid_produces_half_height_quad() {
        let fluids = vec![
            FluidCell {
                fluid_id: FluidId(1),
                level: 128, // ~half
            },
            FluidCell::default(),
            FluidCell::default(),
            FluidCell::default(),
        ];
        let mut buffers = MeshBuildBuffers::default();
        build_fluid_mesh(&fluids, 0, 0, 2, 32.0, &mut buffers);

        assert_eq!(buffers.positions.len(), 4);
        // Half: height = 32 * 128/255 ≈ 16.06
        let expected_height = 32.0 * (128.0 / 255.0);
        assert!((buffers.positions[2][1] - expected_height).abs() < 0.01);
    }
}
