use std::collections::HashSet;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dKey, MeshMaterial2d};

use crate::fluid::cell::FluidCell;
use crate::fluid::reactions::resolve_density_displacement;
use crate::fluid::registry::FluidRegistry;
use crate::fluid::render::{build_fluid_mesh, ATTRIBUTE_FLUID_DATA};
use crate::fluid::simulation::{reconcile_chunk_boundaries, simulate_grid, FluidSimConfig};
use crate::registry::tile::TileRegistry;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::{ChunkCoord, LoadedChunks, WorldMap};

/// Custom Material2d for fluid rendering with lightmap, wave animation, and emission.
///
/// Bindings (all in @group(2)):
///   - texture(0) / sampler(1): lightmap
///   - uniform(2): FluidUniforms { lightmap_uv_rect, time }
#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct FluidMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub lightmap: Handle<Image>,
    #[uniform(2)]
    pub lightmap_uv_rect: Vec4,
    #[uniform(2)]
    pub time: f32,
}

impl Material2d for FluidMaterial {
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

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
            Mesh::ATTRIBUTE_COLOR.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
            ATTRIBUTE_FLUID_DATA.at_shader_location(3),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

/// Tracks which DATA chunk coordinates have active (non-empty) fluids.
/// The simulation only processes chunks in this set.
/// When fluid is placed (e.g. debug commands), the chunk coords must be added here.
#[derive(Resource, Default, Debug)]
pub struct ActiveFluidChunks {
    pub chunks: HashSet<(i32, i32)>,
}

/// Marker component on chunk entities that own a fluid mesh overlay.
#[derive(Component)]
pub struct FluidMeshEntity;

/// Shared material handle for fluid mesh overlays.
#[derive(Resource)]
pub struct SharedFluidMaterial {
    pub handle: Handle<FluidMaterial>,
}

/// Create the shared FluidMaterial with a fallback 1×1 white lightmap.
pub fn init_fluid_material(
    mut commands: Commands,
    mut fluid_materials: ResMut<Assets<FluidMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
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
    commands.insert_resource(SharedFluidMaterial {
        handle: fluid_materials.add(FluidMaterial {
            lightmap: white_lightmap,
            lightmap_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
            time: 0.0,
        }),
    });
}

/// Update the time uniform on the shared FluidMaterial each frame.
pub fn update_fluid_time(
    time: Res<Time>,
    shared: Res<SharedFluidMaterial>,
    mut materials: ResMut<Assets<FluidMaterial>>,
) {
    if let Some(mat) = materials.get_mut(&shared.handle) {
        mat.time = time.elapsed_secs();
    }
}

/// Main fluid simulation system. Runs N iterations per tick on active chunks.
pub fn fluid_simulation(
    mut world_map: ResMut<WorldMap>,
    fluid_registry: Res<FluidRegistry>,
    tile_registry: Res<TileRegistry>,
    active_world: Res<ActiveWorld>,
    mut active_fluids: ResMut<ActiveFluidChunks>,
    config: Res<FluidSimConfig>,
) {
    let chunk_size = active_world.chunk_size;
    let len = (chunk_size * chunk_size) as usize;

    // Collect chunks to process this tick
    let chunks_to_process: Vec<(i32, i32)> = active_fluids.chunks.iter().copied().collect();

    if chunks_to_process.is_empty() {
        return;
    }

    let width_chunks = active_world.width_chunks();
    let height_chunks = active_world.height_chunks();

    for _ in 0..config.iterations_per_tick {
        // Step 1: Simulate each chunk in isolation
        for &(cx, cy) in &chunks_to_process {
            let Some(chunk) = world_map.chunks.get(&(cx, cy)) else {
                continue;
            };

            // Clone current state because simulate_grid reads the old state
            let tiles = chunk.fg.tiles.clone();
            let fluids = chunk.fluids.clone();
            let mut new_fluids = vec![FluidCell::EMPTY; len];

            simulate_grid(
                &tiles,
                &fluids,
                &mut new_fluids,
                chunk_size,
                chunk_size,
                &tile_registry,
                &fluid_registry,
                &config,
            );

            // Apply density displacement (heavier fluids sink)
            resolve_density_displacement(&mut new_fluids, chunk_size, chunk_size, &fluid_registry);

            // Write back
            if let Some(chunk) = world_map.chunks.get_mut(&(cx, cy)) {
                chunk.fluids = new_fluids;
            }
        }

        // Step 2: Transfer fluid across chunk boundaries
        reconcile_chunk_boundaries(
            &mut world_map,
            &active_fluids.chunks,
            chunk_size,
            width_chunks,
            height_chunks,
            &tile_registry,
            &fluid_registry,
            &config,
        );
    }

    // Activate neighbor chunks that received fluid from boundary transfer
    let mut new_active: Vec<(i32, i32)> = Vec::new();
    for &(cx, cy) in &active_fluids.chunks {
        // Check right neighbor
        let ncx = (cx + 1).rem_euclid(width_chunks);
        if !active_fluids.chunks.contains(&(ncx, cy)) {
            if let Some(chunk) = world_map.chunks.get(&(ncx, cy)) {
                if chunk.fluids.iter().any(|c| !c.is_empty()) {
                    new_active.push((ncx, cy));
                }
            }
        }
        // Check left neighbor
        let ncx = (cx - 1).rem_euclid(width_chunks);
        if !active_fluids.chunks.contains(&(ncx, cy)) {
            if let Some(chunk) = world_map.chunks.get(&(ncx, cy)) {
                if chunk.fluids.iter().any(|c| !c.is_empty()) {
                    new_active.push((ncx, cy));
                }
            }
        }
        // Check top neighbor
        if cy + 1 < height_chunks && !active_fluids.chunks.contains(&(cx, cy + 1)) {
            if let Some(chunk) = world_map.chunks.get(&(cx, cy + 1)) {
                if chunk.fluids.iter().any(|c| !c.is_empty()) {
                    new_active.push((cx, cy + 1));
                }
            }
        }
        // Check bottom neighbor
        if cy > 0 && !active_fluids.chunks.contains(&(cx, cy - 1)) {
            if let Some(chunk) = world_map.chunks.get(&(cx, cy - 1)) {
                if chunk.fluids.iter().any(|c| !c.is_empty()) {
                    new_active.push((cx, cy - 1));
                }
            }
        }
    }
    for coord in new_active {
        active_fluids.chunks.insert(coord);
    }

    // Prune chunks that no longer have any fluid
    active_fluids.chunks.retain(|&(cx, cy)| {
        world_map
            .chunks
            .get(&(cx, cy))
            .is_some_and(|chunk| chunk.fluids.iter().any(|c| !c.is_empty()))
    });
}

/// Rebuild fluid mesh overlays for active fluid chunks.
///
/// For each loaded chunk that has active fluids, we either create or update
/// a separate fluid mesh entity. Chunks without fluid get their overlay removed.
#[allow(clippy::too_many_arguments)]
pub fn fluid_rebuild_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    world_map: Res<WorldMap>,
    active_world: Res<ActiveWorld>,
    active_fluids: Res<ActiveFluidChunks>,
    fluid_registry: Res<FluidRegistry>,
    loaded_chunks: Res<LoadedChunks>,
    fluid_material: Res<SharedFluidMaterial>,
    existing_fluid_meshes: Query<(Entity, &ChunkCoord), With<FluidMeshEntity>>,
) {
    // Uses MeshMaterial2d<FluidMaterial> for custom shader rendering.
    let chunk_size = active_world.chunk_size;
    let tile_size = active_world.tile_size;

    // Build a set of display chunk coords that need fluid meshes.
    // Map data chunk coords -> display chunk coords via loaded_chunks.
    let mut display_chunks_with_fluid: HashSet<(i32, i32)> = HashSet::new();
    for &(display_cx, cy) in loaded_chunks.map.keys() {
        let data_cx = active_world.wrap_chunk_x(display_cx);
        for &(fx, fy) in &active_fluids.chunks {
            if fx == data_cx && fy == cy && loaded_chunks.map.contains_key(&(display_cx, cy)) {
                display_chunks_with_fluid.insert((display_cx, cy));
            }
        }
    }

    // Remove fluid mesh entities for chunks no longer needing them
    for (entity, coord) in &existing_fluid_meshes {
        if !display_chunks_with_fluid.contains(&(coord.x, coord.y)) {
            commands.entity(entity).despawn();
        }
    }

    // Create/update fluid meshes for active chunks
    let existing_set: HashSet<(i32, i32)> = existing_fluid_meshes
        .iter()
        .map(|(_, c)| (c.x, c.y))
        .collect();

    for &(display_cx, cy) in &display_chunks_with_fluid {
        let data_cx = active_world.wrap_chunk_x(display_cx);
        let Some(chunk) = world_map.chunks.get(&(data_cx, cy)) else {
            continue;
        };

        let Some(mesh) = build_fluid_mesh(
            &chunk.fluids,
            display_cx,
            cy,
            chunk_size,
            tile_size,
            &fluid_registry,
        ) else {
            continue;
        };

        let mesh_handle = meshes.add(mesh);

        if existing_set.contains(&(display_cx, cy)) {
            // Update existing entity's mesh
            for (entity, coord) in &existing_fluid_meshes {
                if coord.x == display_cx && coord.y == cy {
                    commands.entity(entity).insert(Mesh2d(mesh_handle.clone()));
                    break;
                }
            }
        } else {
            // Spawn new fluid mesh entity
            commands.spawn((
                ChunkCoord {
                    x: display_cx,
                    y: cy,
                },
                FluidMeshEntity,
                Mesh2d(mesh_handle),
                MeshMaterial2d(fluid_material.handle.clone()),
                // Fluid z = 0.5, between fg tiles (z=0) and entities
                Transform::from_translation(bevy::math::Vec3::new(0.0, 0.0, 0.5)),
                Visibility::default(),
            ));
        }
    }
}
