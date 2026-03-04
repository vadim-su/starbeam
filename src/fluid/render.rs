use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::fluid::cell::FluidCell;
use crate::fluid::material::{FluidMaterial, SharedFluidMaterial};
use crate::fluid::registry::FluidRegistry;
use crate::fluid::simulation::{sim_interpolation_frac, DirtyFluidChunks, FluidSimConfig, FluidSimState};
use crate::world::chunk::{ChunkCoord, WorldMap};
use crate::registry::world::ActiveWorld;

/// Marker component for fluid mesh entities.
#[derive(Component)]
pub struct FluidChunkMarker;

/// Marker component indicating a fluid chunk mesh needs rebuilding.
#[derive(Component)]
pub struct FluidDirty;

/// Reusable buffers for building fluid chunk meshes.
#[derive(Resource, Default)]
pub struct FluidMeshBuffers {
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

/// Build a mesh for the fluid layer of a single chunk.
///
/// Each non-empty fluid cell becomes a colored quad whose height is
/// proportional to the visual mass (lerped between prev_mass and mass).
pub fn build_fluid_mesh(
    fluids: &[FluidCell],
    display_chunk_x: i32,
    chunk_y: i32,
    chunk_size: u32,
    tile_size: f32,
    sim_frac: f32,
    fluid_registry: &FluidRegistry,
    buffers: &mut FluidMeshBuffers,
) -> Mesh {
    buffers.positions.clear();
    buffers.uvs.clear();
    buffers.colors.clear();
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

            // Interpolate between previous and current mass for smooth visuals
            let visual_mass = cell.prev_mass + (cell.mass - cell.prev_mass) * sim_frac;
            if visual_mass < 0.001 {
                continue;
            }

            let def = fluid_registry.get(cell.fluid_id);
            let color = [
                def.color[0] as f32 / 255.0,
                def.color[1] as f32 / 255.0,
                def.color[2] as f32 / 255.0,
                def.color[3] as f32 / 255.0,
            ];

            let fill = visual_mass.clamp(0.0, 1.0);
            let px = (base_x + local_x as i32) as f32 * tile_size;
            let py = (base_y + local_y as i32) as f32 * tile_size;
            let height = fill * tile_size;

            // Check if the cell above has fluid (submerged → no surface wave)
            let is_surface = if local_y + 1 < chunk_size {
                let above_idx = ((local_y + 1) * chunk_size + local_x) as usize;
                fluids[above_idx].is_empty()
            } else {
                // Top row of chunk — conservatively treat as surface
                true
            };

            let vi = buffers.positions.len() as u32;

            // Quad: bottom-left, bottom-right, top-right, top-left
            // Anchored at the bottom of the tile cell
            buffers.positions.extend_from_slice(&[
                [px, py, 0.0],
                [px + tile_size, py, 0.0],
                [px + tile_size, py + height, 0.0],
                [px, py + height, 0.0],
            ]);

            // UV: (0,0) = bottom-left, (1,1) = top-right
            // Used by shader for surface wave clipping.
            // For submerged cells (not surface), set uv.y = 0.0 everywhere
            // so the shader's wave check (near_top = 1.0 - uv.y) stays > surface_band
            // and the wave effect is skipped.
            let top_v = if is_surface { 1.0 } else { 0.0 };
            buffers.uvs.extend_from_slice(&[
                [0.0, 0.0],
                [1.0, 0.0],
                [1.0, top_v],
                [0.0, top_v],
            ]);

            buffers.colors.extend_from_slice(&[color, color, color, color]);

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
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, buffers.colors.clone());
    mesh.insert_indices(Indices::U32(buffers.indices.clone()));
    mesh
}

/// System: rebuild fluid meshes for chunks that were marked dirty by the simulation.
#[allow(clippy::too_many_arguments)]
pub fn rebuild_fluid_meshes(
    mut commands: Commands,
    dirty_chunks: Res<DirtyFluidChunks>,
    fluid_entities: Query<(Entity, &ChunkCoord), With<FluidChunkMarker>>,
    mut meshes: ResMut<Assets<Mesh>>,
    world_map: Res<WorldMap>,
    active_world: Res<ActiveWorld>,
    fluid_registry: Option<Res<FluidRegistry>>,
    sim_state: Res<FluidSimState>,
    sim_config: Res<FluidSimConfig>,
    mut buffers: ResMut<FluidMeshBuffers>,
) {
    let Some(fluid_registry) = fluid_registry else {
        return;
    };
    if dirty_chunks.0.is_empty() {
        return;
    }

    let sim_frac = sim_interpolation_frac(&sim_state, &sim_config);

    for (entity, coord) in &fluid_entities {
        let data_cx = active_world.wrap_chunk_x(coord.x);
        if !dirty_chunks.0.contains(&(data_cx, coord.y)) {
            continue;
        }

        let Some(chunk_data) = world_map.chunks.get(&(data_cx, coord.y)) else {
            continue;
        };

        let mesh = build_fluid_mesh(
            &chunk_data.fluids,
            coord.x,
            coord.y,
            active_world.chunk_size,
            active_world.tile_size,
            sim_frac,
            &fluid_registry,
            &mut buffers,
        );

        let mesh_handle = meshes.add(mesh);
        commands
            .entity(entity)
            .insert(Mesh2d(mesh_handle))
            .remove::<FluidDirty>();
    }
}

/// System: initialize SharedFluidMaterial resource on entering InGame state.
pub fn init_fluid_material(
    mut commands: Commands,
    mut fluid_materials: ResMut<Assets<FluidMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    // Create a 1x1 white placeholder lightmap (will be replaced by RC lighting each frame)
    let placeholder_lightmap = images.add(Image::new_fill(
        bevy::render::render_resource::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        &[255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        bevy::render::render_resource::TextureFormat::Rgba16Float,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    ));

    let handle = fluid_materials.add(FluidMaterial {
        time: 0.0,
        lightmap: placeholder_lightmap,
        lightmap_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
    });

    commands.insert_resource(SharedFluidMaterial(handle));
}

/// System: update the FluidMaterial uniform with current time for wave animation.
pub fn update_fluid_material(
    time: Res<Time>,
    fluid_material_handle: Option<Res<SharedFluidMaterial>>,
    mut fluid_materials: ResMut<Assets<FluidMaterial>>,
) {
    let Some(handle) = fluid_material_handle else {
        return;
    };
    if let Some(mat) = fluid_materials.get_mut(&handle.0) {
        mat.time = time.elapsed_secs();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::cell::{FluidCell, FluidId};
    use crate::fluid::registry::{FluidDef, FluidRegistry};

    fn test_fluid_registry() -> FluidRegistry {
        FluidRegistry::from_defs(vec![FluidDef {
            id: "water".to_string(),
            density: 1000.0,
            viscosity: 0.1,
            max_compress: 0.02,
            is_gas: false,
            color: [64, 128, 255, 180],
            damage_on_contact: 0.0,
            light_emission: [0, 0, 0],
            effects: vec![],
            wave_amplitude: 1.0,
            wave_speed: 1.0,
            light_absorption: 0.3,
        }])
    }

    #[test]
    fn build_mesh_empty_fluids() {
        let reg = test_fluid_registry();
        let mut buffers = FluidMeshBuffers::default();
        let fluids = vec![FluidCell::EMPTY; 4];

        let _mesh = build_fluid_mesh(&fluids, 0, 0, 2, 32.0, 0.5, &reg, &mut buffers);

        assert_eq!(buffers.positions.len(), 0, "empty fluids = no vertices");
        assert_eq!(buffers.indices.len(), 0, "empty fluids = no indices");
    }

    #[test]
    fn build_mesh_single_cell() {
        let reg = test_fluid_registry();
        let mut buffers = FluidMeshBuffers::default();

        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 0.5);

        let _mesh = build_fluid_mesh(&fluids, 0, 0, 2, 32.0, 1.0, &reg, &mut buffers);

        assert_eq!(buffers.positions.len(), 4, "1 quad = 4 vertices");
        assert_eq!(buffers.indices.len(), 6, "1 quad = 6 indices");
        assert_eq!(buffers.colors.len(), 4, "1 quad = 4 colors");

        // Height should be 0.5 * 32.0 = 16.0
        // Bottom at y=0, top at y=16
        assert_eq!(buffers.positions[0][1], 0.0); // bottom
        assert_eq!(buffers.positions[2][1], 16.0); // top
    }

    #[test]
    fn build_mesh_interpolates_mass() {
        let reg = test_fluid_registry();
        let mut buffers = FluidMeshBuffers::default();

        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell {
            fluid_id: FluidId(1),
            mass: 1.0,
            prev_mass: 0.0,
        };

        // sim_frac = 0.5: visual_mass = lerp(0.0, 1.0, 0.5) = 0.5
        let _mesh = build_fluid_mesh(&fluids, 0, 0, 2, 32.0, 0.5, &reg, &mut buffers);

        // Height should be 0.5 * 32.0 = 16.0
        assert_eq!(buffers.positions[2][1], 16.0);
    }
}
