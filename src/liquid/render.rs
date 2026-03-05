use std::collections::HashSet;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dKey};

use crate::liquid::data::LiquidCell;
use crate::liquid::registry::LiquidRegistry;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::{ChunkCoord, LoadedChunks, WorldMap};

// ---------------------------------------------------------------------------
// Material
// ---------------------------------------------------------------------------

#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct LiquidMaterial {
    #[uniform(0)]
    pub color: LinearRgba,
}

impl Material2d for LiquidMaterial {
    fn vertex_shader() -> ShaderRef {
        "engine/shaders/liquid.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "engine/shaders/liquid.wgsl".into()
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_COLOR.at_shader_location(1),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        // Enable alpha blending for semi-transparent liquid
        if let Some(target) = descriptor
            .fragment
            .as_mut()
            .and_then(|f| f.targets.get_mut(0))
            .and_then(|t| t.as_mut())
        {
            target.blend = Some(bevy::render::render_resource::BlendState::ALPHA_BLENDING);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Shared material resource
// ---------------------------------------------------------------------------

/// Shared liquid material handle, created once on InGame enter.
#[derive(Resource)]
pub struct SharedLiquidMaterial(pub Handle<LiquidMaterial>);

/// Set of data chunk coords whose liquid meshes need rebuilding.
/// Populated by the liquid simulation system, consumed by the rebuild system.
#[derive(Resource, Default)]
pub struct DirtyLiquidChunks(pub HashSet<(i32, i32)>);

// ---------------------------------------------------------------------------
// Marker component
// ---------------------------------------------------------------------------

/// Marker component for liquid mesh entities, linking them to their chunk.
#[derive(Component)]
pub struct LiquidMeshEntity;

// ---------------------------------------------------------------------------
// Mesh builder
// ---------------------------------------------------------------------------

/// Build a mesh for one chunk's liquid layer.
///
/// Each non-empty liquid cell becomes a quad whose height is proportional to
/// the cell's level (0..1). The quad fills the full tile width and sits at the
/// bottom of the tile.
pub fn build_liquid_mesh(
    cells: &[LiquidCell],
    display_chunk_x: i32,
    chunk_y: i32,
    chunk_size: u32,
    tile_size: f32,
    liquid_registry: &LiquidRegistry,
) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let base_x = display_chunk_x as f32 * chunk_size as f32 * tile_size;
    let base_y = chunk_y as f32 * chunk_size as f32 * tile_size;

    for local_y in 0..chunk_size {
        for local_x in 0..chunk_size {
            let idx = (local_y * chunk_size + local_x) as usize;
            if idx >= cells.len() {
                continue;
            }
            let cell = cells[idx];
            if cell.is_empty() {
                continue;
            }

            let color = liquid_registry
                .get(cell.liquid_type)
                .map(|d| d.color)
                .unwrap_or([0.0, 0.0, 1.0, 0.5]);

            let x = base_x + local_x as f32 * tile_size;
            let y = base_y + local_y as f32 * tile_size;
            let height = cell.level.clamp(0.0, 1.0) * tile_size;

            let vi = positions.len() as u32;
            // Quad: bottom-left, bottom-right, top-right, top-left
            positions.push([x, y, 0.0]);
            positions.push([x + tile_size, y, 0.0]);
            positions.push([x + tile_size, y + height, 0.0]);
            positions.push([x, y + height, 0.0]);

            colors.push(color);
            colors.push(color);
            colors.push(color);
            colors.push(color);

            indices.push(vi);
            indices.push(vi + 1);
            indices.push(vi + 2);
            indices.push(vi);
            indices.push(vi + 2);
            indices.push(vi + 3);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

// ---------------------------------------------------------------------------
// Init system — create shared material on InGame enter
// ---------------------------------------------------------------------------

pub fn init_liquid_material(mut commands: Commands, mut materials: ResMut<Assets<LiquidMaterial>>) {
    let handle = materials.add(LiquidMaterial {
        color: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
    });
    commands.insert_resource(SharedLiquidMaterial(handle));
}

// ---------------------------------------------------------------------------
// Rebuild system — rebuild liquid meshes for dirty chunks
// ---------------------------------------------------------------------------

/// Rebuild liquid meshes for chunks whose liquid data has changed.
///
/// Uses the `DirtyLiquidChunks` resource (populated by the liquid simulation)
/// to determine which chunks need a mesh rebuild. Clears the dirty set after
/// processing so rebuilds only happen once per change.
#[allow(clippy::too_many_arguments)]
pub fn rebuild_liquid_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    world_map: Res<WorldMap>,
    config: Res<ActiveWorld>,
    liquid_registry: Res<LiquidRegistry>,
    loaded_chunks: Res<LoadedChunks>,
    mut dirty_liquid: ResMut<DirtyLiquidChunks>,
    liquid_query: Query<(Entity, &ChunkCoord), With<LiquidMeshEntity>>,
) {
    if dirty_liquid.0.is_empty() {
        return;
    }

    for (entity, coord) in &liquid_query {
        let data_cx = config.wrap_chunk_x(coord.x);
        if !dirty_liquid.0.contains(&(data_cx, coord.y)) {
            continue;
        }

        // Verify this display chunk is still loaded.
        if !loaded_chunks.map.contains_key(&(coord.x, coord.y)) {
            continue;
        }

        let Some(chunk) = world_map.chunk(data_cx, coord.y) else {
            continue;
        };

        let mesh = build_liquid_mesh(
            &chunk.liquid.cells,
            coord.x,
            coord.y,
            config.chunk_size,
            config.tile_size,
            &liquid_registry,
        );

        commands.entity(entity).insert(Mesh2d(meshes.add(mesh)));
    }

    dirty_liquid.0.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::liquid::data::{LiquidCell, LiquidId};

    #[test]
    fn empty_cells_produce_empty_mesh() {
        let cells = vec![LiquidCell::EMPTY; 4];
        let registry = LiquidRegistry::default();
        let mesh = build_liquid_mesh(&cells, 0, 0, 2, 8.0, &registry);
        // No vertices for empty cells
        assert!(mesh.attribute(Mesh::ATTRIBUTE_POSITION).is_some());
        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap();
        assert_eq!(positions.len(), 0);
    }

    #[test]
    fn non_empty_cell_produces_quad() {
        let mut cells = vec![LiquidCell::EMPTY; 4];
        cells[0] = LiquidCell {
            liquid_type: LiquidId(1),
            level: 0.5,
        };
        let registry = LiquidRegistry::from_defs(vec![crate::liquid::registry::LiquidDef {
            name: "water".into(),
            density: 1.0,
            viscosity: 1.0,
            color: [0.0, 0.3, 0.8, 0.6],
            damage_on_contact: 0.0,
            light_emission: [0, 0, 0],
            light_opacity: 0,
            swim_speed_factor: 0.5,
            reactions: vec![],
        }]);
        let mesh = build_liquid_mesh(&cells, 0, 0, 2, 8.0, &registry);
        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap();
        // 1 non-empty cell = 1 quad = 4 vertices
        assert_eq!(positions.len(), 4);
        let indices = mesh.indices().unwrap();
        assert_eq!(indices.len(), 6);
    }
}
