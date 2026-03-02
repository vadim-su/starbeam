use std::collections::HashSet;

use bevy::prelude::*;
use bevy::sprite_render::MeshMaterial2d;

use crate::fluid::cell::FluidCell;
use crate::fluid::reactions::resolve_density_displacement;
use crate::fluid::registry::FluidRegistry;
use crate::fluid::render::build_fluid_mesh;
use crate::fluid::simulation::{reconcile_chunk_boundaries, simulate_grid, FluidSimConfig};
use crate::registry::tile::TileRegistry;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::{ChunkCoord, LoadedChunks, WorldMap};

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

/// Shared material handle for fluid mesh overlays (vertex-colored).
#[derive(Resource)]
pub struct SharedFluidMaterial {
    pub handle: Handle<ColorMaterial>,
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
