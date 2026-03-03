use std::collections::{HashMap, HashSet};

use bevy::asset::RenderAssetUsages;
use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dKey, MeshMaterial2d};

use crate::fluid::cell::FluidCell;
use crate::fluid::events::{FluidReactionEvent, ImpactKind, WaterImpactEvent};
use crate::fluid::reactions::FluidReactionRegistry;
use crate::fluid::reactions::{execute_fluid_reactions, resolve_density_displacement};
use crate::fluid::registry::FluidRegistry;
use crate::fluid::render::{
    build_fluid_mesh, column_gas_surface_h, column_liquid_surface_h, ATTRIBUTE_EDGE_FLAGS,
    ATTRIBUTE_FLUID_DATA, ATTRIBUTE_WAVE_HEIGHT, ATTRIBUTE_WAVE_PARAMS,
};
use crate::fluid::simulation::{reconcile_chunk_boundaries, simulate_grid, FluidSimConfig};
use crate::fluid::wave::{reconcile_wave_boundaries, WaveBuffer, WaveConfig, WaveState};
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
            ATTRIBUTE_WAVE_HEIGHT.at_shader_location(4),
            ATTRIBUTE_WAVE_PARAMS.at_shader_location(5),
            ATTRIBUTE_EDGE_FLAGS.at_shader_location(6),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

/// Tracks which DATA chunk coordinates have active (non-empty) fluids.
/// The simulation only processes chunks in this set.
/// How many consecutive calm ticks before a chunk enters sleep mode.
/// At 60 fps with default tick rate, 60 ticks ≈ 1 second of stillness.
const SLEEP_THRESHOLD: u32 = 60;

/// Minimum mass-change per cell to consider a chunk still active.
const CALM_MASS_EPSILON: f32 = 0.001;

/// When fluid is placed (e.g. debug commands), the chunk coords must be added here.
#[derive(Resource, Default, Debug)]
pub struct ActiveFluidChunks {
    pub chunks: HashSet<(i32, i32)>,
    /// Consecutive ticks with no fluid movement per chunk.
    /// When this exceeds `SLEEP_THRESHOLD`, simulation is skipped for that chunk.
    /// Reset to 0 whenever fluid moves or is externally added.
    pub calm_ticks: HashMap<(i32, i32), u32>,
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
    reaction_registry: Option<Res<FluidReactionRegistry>>,
    mut reaction_events: MessageWriter<FluidReactionEvent>,
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
    let tile_size = active_world.tile_size;

    // Step 1: Simulate each chunk with double-buffered fluid arrays.
    //
    // Double-buffering eliminates the per-iteration O(chunk_size²) allocation:
    //   OLD: N × (clone + alloc) per chunk per tick
    //   NEW: 1 × (clone + alloc) per chunk per tick, then N swaps
    //
    // Sleep/wake: chunks with no movement for SLEEP_THRESHOLD ticks are skipped.
    // reconcile_chunk_boundaries runs once after all intra-chunk iterations.
    for &(cx, cy) in &chunks_to_process {
        let Some(chunk) = world_map.chunks.get(&(cx, cy)) else {
            continue;
        };

        // --- Sleep check ---
        // Skip simulation for chunks that have been calm long enough.
        // We still process them in reconcile so cross-chunk flow can wake them.
        let calm = active_fluids
            .calm_ticks
            .get(&(cx, cy))
            .copied()
            .unwrap_or(0);
        if calm > SLEEP_THRESHOLD {
            continue;
        }

        // Allocate buffers once per chunk per tick
        let mut buf_a = chunk.fluids.clone();
        let initial_fluids = buf_a.clone(); // snapshot to detect movement
        let mut buf_b = vec![FluidCell::EMPTY; len];
        let mut tiles = chunk.fg.tiles.clone();

        for _ in 0..config.iterations_per_tick {
            simulate_grid(
                &tiles,
                &buf_a,
                &mut buf_b,
                chunk_size,
                chunk_size,
                &tile_registry,
                &fluid_registry,
                &config,
            );

            // Apply density displacement (heavier fluids sink)
            resolve_density_displacement(&mut buf_b, chunk_size, chunk_size, &fluid_registry);

            // Apply fluid reactions (e.g. lava + water → stone + steam)
            if let Some(ref rr) = reaction_registry {
                let events = execute_fluid_reactions(
                    &mut buf_b, &mut tiles, chunk_size, chunk_size, rr, cx, cy, tile_size,
                );
                for evt in events {
                    reaction_events.write(evt);
                }
            }

            // Swap buffers: buf_a becomes the new read state
            std::mem::swap(&mut buf_a, &mut buf_b);
            // Clear the write buffer for the next iteration
            buf_b.fill(FluidCell::EMPTY);
        }

        // --- Detect movement and update calm_ticks ---
        let moved = initial_fluids.iter().zip(buf_a.iter()).any(|(old, new)| {
            old.fluid_id != new.fluid_id || (old.mass - new.mass).abs() >= CALM_MASS_EPSILON
        });

        let entry = active_fluids.calm_ticks.entry((cx, cy)).or_insert(0);
        if moved {
            *entry = 0;
        } else {
            *entry = entry.saturating_add(1);
        }

        // Write back final state
        if let Some(chunk) = world_map.chunks.get_mut(&(cx, cy)) {
            chunk.fluids = buf_a;
            chunk.fg.tiles = tiles;
        }
    }

    // Step 2: Transfer fluid across chunk boundaries (once per tick)
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
        // Wake newly activated chunks — they just received fluid
        active_fluids.calm_ticks.insert(coord, 0);
    }

    // Also wake sleeping chunks that received fluid from reconcile
    // (reconcile modifies chunk.fluids directly, bypassing the sleep check above).
    // Collect candidates first to avoid double-borrow.
    let sleeping_with_fluid: Vec<(i32, i32)> = active_fluids
        .chunks
        .iter()
        .copied()
        .filter(|coord| active_fluids.calm_ticks.get(coord).copied().unwrap_or(0) > SLEEP_THRESHOLD)
        .filter(|&(cx, cy)| {
            world_map
                .chunks
                .get(&(cx, cy))
                .is_some_and(|chunk| chunk.fluids.iter().any(|c| !c.is_empty()))
        })
        .collect();
    for coord in sleeping_with_fluid {
        active_fluids.calm_ticks.insert(coord, 0);
    }

    // Prune chunks that no longer have any fluid
    let to_remove: Vec<(i32, i32)> = active_fluids
        .chunks
        .iter()
        .copied()
        .filter(|&(cx, cy)| {
            !world_map
                .chunks
                .get(&(cx, cy))
                .is_some_and(|chunk| chunk.fluids.iter().any(|c| !c.is_empty()))
        })
        .collect();
    for coord in to_remove {
        active_fluids.chunks.remove(&coord);
        active_fluids.calm_ticks.remove(&coord);
    }
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
    tile_registry: Res<TileRegistry>,
    loaded_chunks: Res<LoadedChunks>,
    fluid_material: Res<SharedFluidMaterial>,
    existing_fluid_meshes: Query<(Entity, &ChunkCoord), With<FluidMeshEntity>>,
    wave_state: Res<WaveState>,
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

        // Extract neighbor boundary rows for cross-chunk surface detection.
        // Above: bottom row (local_y=0) of chunk at (data_cx, cy+1)
        let neighbor_above_row: Option<Vec<FluidCell>> =
            world_map.chunks.get(&(data_cx, cy + 1)).map(|c| {
                (0..chunk_size)
                    .map(|x| c.fluids[(0 * chunk_size + x) as usize])
                    .collect()
            });
        // Below: top row (local_y=chunk_size-1) of chunk at (data_cx, cy-1)
        let neighbor_below_row: Option<Vec<FluidCell>> = if cy > 0 {
            world_map.chunks.get(&(data_cx, cy - 1)).map(|c| {
                (0..chunk_size)
                    .map(|x| c.fluids[((chunk_size - 1) * chunk_size + x) as usize])
                    .collect()
            })
        } else {
            None
        };

        let wave_heights = wave_state
            .buffers
            .get(&(data_cx, cy))
            .map(|buf| buf.height.as_slice());

        // ── Cross-chunk surface smoothing data ──────────────────────────
        // Extract the liquid/gas surface height of the left neighbour's
        // rightmost column and the right neighbour's leftmost column.
        // These are used by build_fluid_mesh to smooth the surface vertex
        // heights at the horizontal chunk boundary so the seam disappears.
        let width_chunks = active_world.width_chunks();
        let left_data_cx = (data_cx - 1).rem_euclid(width_chunks);
        let right_data_cx = (data_cx + 1).rem_euclid(width_chunks);

        let left_edge_liquid_h = world_map.chunks.get(&(left_data_cx, cy)).and_then(|c| {
            column_liquid_surface_h(&c.fluids, chunk_size - 1, chunk_size, &fluid_registry)
        });
        let right_edge_liquid_h = world_map
            .chunks
            .get(&(right_data_cx, cy))
            .and_then(|c| column_liquid_surface_h(&c.fluids, 0, chunk_size, &fluid_registry));

        let left_edge_gas_h = world_map.chunks.get(&(left_data_cx, cy)).and_then(|c| {
            column_gas_surface_h(&c.fluids, chunk_size - 1, chunk_size, &fluid_registry)
        });
        let right_edge_gas_h = world_map
            .chunks
            .get(&(right_data_cx, cy))
            .and_then(|c| column_gas_surface_h(&c.fluids, 0, chunk_size, &fluid_registry));

        // Per-column absolute world-tile Y of the liquid surface in the chunk above.
        // Used by compute_depth to compute accurate depth for cells whose upward scan
        // reaches the top chunk boundary, preventing the brightness seam at the seam.
        let above_surface_world_ys: Option<Vec<Option<f32>>> =
            world_map.chunks.get(&(data_cx, cy + 1)).map(|c| {
                (0..chunk_size)
                    .map(|col| {
                        column_liquid_surface_h(&c.fluids, col, chunk_size, &fluid_registry)
                            .map(|local_h| (cy + 1) as f32 * chunk_size as f32 + local_h)
                    })
                    .collect()
            });

        let Some(mesh) = build_fluid_mesh(
            &chunk.fluids,
            chunk.fg.tiles.as_slice(),
            display_cx,
            cy,
            chunk_size,
            tile_size,
            &fluid_registry,
            &tile_registry,
            neighbor_above_row.as_deref(),
            neighbor_below_row.as_deref(),
            wave_heights,
            left_edge_liquid_h,
            right_edge_liquid_h,
            left_edge_gas_h,
            right_edge_gas_h,
            above_surface_world_ys.as_deref(),
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

/// Consume WaterImpactEvents and apply impulses to wave buffers.
pub fn wave_consume_events(
    mut events: MessageReader<WaterImpactEvent>,
    mut wave_state: ResMut<WaveState>,
    active_world: Res<ActiveWorld>,
    wave_config: Res<WaveConfig>,
) {
    let chunk_size = active_world.chunk_size;
    let tile_size = active_world.tile_size;

    for event in events.read() {
        // Convert world position to chunk + local coords
        let tile_x = (event.position.x / tile_size).floor() as i32;
        let tile_y = (event.position.y / tile_size).floor() as i32;
        let data_cx = active_world.wrap_chunk_x(tile_x.div_euclid(chunk_size as i32));
        let cy = tile_y.div_euclid(chunk_size as i32);
        let local_x = tile_x.rem_euclid(chunk_size as i32) as u32;
        let local_y = tile_y.rem_euclid(chunk_size as i32) as u32;

        let impulse = match event.kind {
            ImpactKind::Splash => event.velocity.y.abs() * 0.08 * event.mass.sqrt(),
            ImpactKind::Wake => event.velocity.length() * 0.02,
            ImpactKind::Pour => event.velocity.y.abs() * 0.04,
        };

        let max_imp = wave_config.max_impulse;

        // Main impulse
        wave_state
            .buffers
            .entry((data_cx, cy))
            .or_insert_with(|| WaveBuffer::new(chunk_size))
            .apply_impulse(local_x, local_y, impulse, max_imp);

        // Spread impulse to neighbors for wider splash
        if matches!(event.kind, ImpactKind::Splash) {
            let spread = impulse * 0.5;

            // Left spread
            if local_x > 0 {
                wave_state
                    .buffers
                    .entry((data_cx, cy))
                    .or_insert_with(|| WaveBuffer::new(chunk_size))
                    .apply_impulse(local_x - 1, local_y, spread, max_imp);
            } else {
                // Cross-chunk: left neighbor's rightmost column
                let left_cx = active_world.wrap_chunk_x(data_cx - 1);
                wave_state
                    .buffers
                    .entry((left_cx, cy))
                    .or_insert_with(|| WaveBuffer::new(chunk_size))
                    .apply_impulse(chunk_size - 1, local_y, spread, max_imp);
            }

            // Right spread
            if local_x + 1 < chunk_size {
                wave_state
                    .buffers
                    .entry((data_cx, cy))
                    .or_insert_with(|| WaveBuffer::new(chunk_size))
                    .apply_impulse(local_x + 1, local_y, spread, max_imp);
            } else {
                // Cross-chunk: right neighbor's leftmost column
                let right_cx = active_world.wrap_chunk_x(data_cx + 1);
                wave_state
                    .buffers
                    .entry((right_cx, cy))
                    .or_insert_with(|| WaveBuffer::new(chunk_size))
                    .apply_impulse(0, local_y, spread, max_imp);
            }
        }
    }
}

/// Step wave simulation for all active wave buffers.
pub fn wave_simulation(
    world_map: Res<WorldMap>,
    active_world: Res<ActiveWorld>,
    active_fluids: Res<ActiveFluidChunks>,
    mut wave_state: ResMut<WaveState>,
    wave_config: Res<WaveConfig>,
) {
    let chunk_size = active_world.chunk_size;
    let width_chunks = active_world.width_chunks();

    // Step each buffer
    for &(cx, cy) in &active_fluids.chunks {
        if let Some(buf) = wave_state.buffers.get_mut(&(cx, cy)) {
            if let Some(chunk) = world_map.chunks.get(&(cx, cy)) {
                buf.step(&chunk.fluids, &wave_config);
            }
        }
    }

    // Reconcile boundaries
    reconcile_wave_boundaries(
        &mut wave_state,
        &active_fluids.chunks,
        chunk_size,
        width_chunks,
    );

    // Prune calm buffers
    wave_state
        .buffers
        .retain(|_, buf| !buf.is_calm(wave_config.epsilon));
}
