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
use crate::fluid::fluid_world::FluidWorld;
use crate::fluid::sph_collision::{enforce_world_bounds, resolve_tile_collision};
use crate::fluid::sph_particle::ParticleStore;
use crate::fluid::sph_simulation::{sph_step, SphConfig};
use crate::fluid::reactions::{
    execute_fluid_reactions_global, execute_sph_particle_reactions,
    resolve_density_displacement_global, FluidReactionRegistry,
};
use crate::fluid::registry::FluidRegistry;
use crate::fluid::render::{
    build_fluid_mesh, column_gas_surface_h, column_liquid_surface_h, ATTRIBUTE_EDGE_FLAGS,
    ATTRIBUTE_FLUID_DATA, ATTRIBUTE_WAVE_HEIGHT, ATTRIBUTE_WAVE_PARAMS,
};
use crate::fluid::simulation::{simulate_tick, FluidSimConfig};
use crate::fluid::wave::{reconcile_wave_boundaries, WaveBuffer, WaveConfig, WaveState};
use crate::registry::tile::TileRegistry;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::{ChunkCoord, LoadedChunks, WorldMap};

/// Tracks time between fixed fluid simulation ticks.
#[derive(Resource, Default)]
pub struct FluidTickAccumulator {
    pub accumulator: f32,
}

/// Custom Material2d for fluid rendering with lightmap, wave animation, and emission.
///
/// Bindings (all in @group(2)):
///   - texture(0) / sampler(1): lightmap
///   - uniform(2): FluidUniforms { lightmap_uv_rect, time, debug_mode, show_grid }
#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct FluidMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub lightmap: Handle<Image>,
    #[uniform(2)]
    pub lightmap_uv_rect: Vec4,
    #[uniform(2)]
    pub time: f32,
    /// Debug visualization mode: 0=off, 1=mass, 2=surface, 3=fluid_type, 4=depth.
    #[uniform(2)]
    pub debug_mode: u32,
    /// Whether to show grid lines between cells in debug mode.
    #[uniform(2)]
    pub show_grid: u32,
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
            debug_mode: 0,
            show_grid: 0,
        }),
    });
}

/// Update the time and debug uniforms on the shared FluidMaterial each frame.
pub fn update_fluid_time(
    time: Res<Time>,
    shared: Res<SharedFluidMaterial>,
    mut materials: ResMut<Assets<FluidMaterial>>,
    debug_state: Option<Res<crate::fluid::debug_overlay::FluidDebugState>>,
) {
    if let Some(mat) = materials.get_mut(&shared.handle) {
        mat.time = time.elapsed_secs();
        if let Some(dbg) = &debug_state {
            if dbg.visible {
                mat.debug_mode = dbg.mode.as_u32();
                mat.show_grid = if dbg.show_grid { 1 } else { 0 };
            } else {
                mat.debug_mode = 0;
                mat.show_grid = 0;
            }
        }
    }
}

/// Main fluid simulation system. Runs at a fixed tick rate using an accumulator
/// to decouple simulation speed from frame rate.
pub fn fluid_simulation(
    time: Res<Time>,
    mut accumulator: ResMut<FluidTickAccumulator>,
    mut world_map: ResMut<WorldMap>,
    fluid_registry: Res<FluidRegistry>,
    tile_registry: Res<TileRegistry>,
    active_world: Res<ActiveWorld>,
    mut active_fluids: ResMut<ActiveFluidChunks>,
    config: Res<FluidSimConfig>,
    reaction_registry: Option<Res<FluidReactionRegistry>>,
    mut reaction_events: MessageWriter<FluidReactionEvent>,
) {
    let tick_interval = 1.0 / config.tick_rate;
    accumulator.accumulator += time.delta_secs();
    let mut ticks_this_frame = 0u32;

    while accumulator.accumulator >= tick_interval && ticks_this_frame < config.max_ticks_per_frame
    {
        accumulator.accumulator -= tick_interval;
        ticks_this_frame += 1;

        run_one_tick(
            &mut world_map,
            &fluid_registry,
            &tile_registry,
            &active_world,
            &mut active_fluids,
            &config,
            reaction_registry.as_deref(),
            &mut reaction_events,
            ticks_this_frame - 1,
        );
    }

    // Prevent accumulator spiral
    let max_acc = tick_interval * config.max_ticks_per_frame as f32;
    if accumulator.accumulator > max_acc {
        accumulator.accumulator = max_acc;
    }
}

/// One tick of the global fluid simulation using FluidWorld.
///
/// Creates a snapshot of all active chunks, runs simulation, density displacement,
/// and reactions globally, then detects movement and manages chunk sleep/wake.
#[allow(clippy::too_many_arguments)]
fn run_one_tick(
    world_map: &mut WorldMap,
    fluid_registry: &FluidRegistry,
    tile_registry: &TileRegistry,
    active_world: &ActiveWorld,
    active_fluids: &mut ActiveFluidChunks,
    config: &FluidSimConfig,
    reaction_registry: Option<&FluidReactionRegistry>,
    reaction_events: &mut MessageWriter<FluidReactionEvent>,
    tick_parity: u32,
) {
    let chunk_size = active_world.chunk_size;
    let width_chunks = active_world.width_chunks();
    let height_chunks = active_world.height_chunks();

    // Filter out sleeping chunks
    let chunks_to_process: Vec<(i32, i32)> = active_fluids
        .chunks
        .iter()
        .copied()
        .filter(|coord| {
            active_fluids.calm_ticks.get(coord).copied().unwrap_or(0) <= SLEEP_THRESHOLD
        })
        .collect();

    if chunks_to_process.is_empty() {
        return;
    }

    // Snapshot for movement detection (before simulation modifies live data)
    let initial_snapshots: HashMap<(i32, i32), Vec<FluidCell>> = chunks_to_process
        .iter()
        .filter_map(|&(cx, cy)| {
            world_map
                .chunks
                .get(&(cx, cy))
                .map(|c| ((cx, cy), c.fluids.clone()))
        })
        .collect();

    // --- Run global simulation within a FluidWorld scope ---
    {
        let mut fluid_world = FluidWorld::new(
            world_map,
            chunk_size,
            width_chunks,
            height_chunks,
            tile_registry,
            fluid_registry,
        );

        simulate_tick(&mut fluid_world, &chunks_to_process, config, tick_parity);
        resolve_density_displacement_global(&mut fluid_world, &chunks_to_process);

        if let Some(rr) = reaction_registry {
            let events = execute_fluid_reactions_global(
                &mut fluid_world,
                &chunks_to_process,
                rr,
                active_world.tile_size,
            );
            for evt in events {
                reaction_events.write(evt);
            }
        }
    }
    // fluid_world dropped here, releasing &mut world_map

    // --- Detect movement and update calm_ticks ---
    for &(cx, cy) in &chunks_to_process {
        let moved = if let (Some(initial), Some(chunk)) = (
            initial_snapshots.get(&(cx, cy)),
            world_map.chunks.get(&(cx, cy)),
        ) {
            initial.iter().zip(chunk.fluids.iter()).any(|(old, new)| {
                old.fluid_id() != new.fluid_id() || (old.mass() - new.mass()).abs() >= CALM_MASS_EPSILON
            })
        } else {
            false
        };

        let entry = active_fluids.calm_ticks.entry((cx, cy)).or_insert(0);
        if moved {
            *entry = 0;
        } else {
            *entry = entry.saturating_add(1);
        }
    }

    // --- Activate neighbor chunks that have fluid ---
    let current_chunks: Vec<(i32, i32)> = active_fluids.chunks.iter().copied().collect();
    let mut new_active: Vec<(i32, i32)> = Vec::new();
    for &(cx, cy) in &current_chunks {
        for (dx, dy) in [(-1i32, 0), (1, 0), (0, -1), (0, 1)] {
            let ncx = if dy == 0 {
                (cx + dx).rem_euclid(width_chunks)
            } else {
                cx
            };
            let ncy = cy + dy;
            if ncy < 0 || ncy >= height_chunks {
                continue;
            }
            if active_fluids.chunks.contains(&(ncx, ncy)) {
                continue;
            }
            if let Some(chunk) = world_map.chunks.get(&(ncx, ncy)) {
                if chunk.fluids.iter().any(|c| !c.is_empty()) {
                    new_active.push((ncx, ncy));
                }
            }
        }
    }
    for coord in new_active {
        active_fluids.chunks.insert(coord);
        active_fluids.calm_ticks.insert(coord, 0);
    }

    // Wake sleeping chunks that still have fluid (may have received fluid from
    // global simulation crossing chunk boundaries)
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
            None,
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
            ImpactKind::Splash => event.velocity.y.abs() * 0.03 * event.mass.sqrt(),
            ImpactKind::Wake => event.velocity.length() * 0.005,
            ImpactKind::Pour => event.velocity.y.abs() * 0.015,
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

/// SPH particle-based fluid simulation system.
/// Runs alongside the existing CA system. Particles are simulated with SPH
/// physics and collided against the tile world.
pub fn sph_fluid_simulation(
    _time: Res<Time>,
    sph_config: Res<SphConfig>,
    _accumulator: ResMut<FluidTickAccumulator>,
    mut particles: ResMut<ParticleStore>,
    world_map: Res<WorldMap>,
    active_world: Res<ActiveWorld>,
    reaction_registry: Option<Res<FluidReactionRegistry>>,
    mut reaction_events: MessageWriter<FluidReactionEvent>,
) {
    if particles.is_empty() {
        return;
    }

    let dt = 1.0 / 60.0;

    sph_step(&mut particles, &sph_config, dt);

    // Tile collisions
    let tile_size = active_world.tile_size;
    let chunk_size = active_world.chunk_size;
    let width_chunks = active_world.width_chunks();
    let world_width = width_chunks as f32 * chunk_size as f32 * tile_size;
    let world_height = active_world.height_chunks() as f32 * chunk_size as f32 * tile_size;

    let is_solid = |gx: i32, gy: i32| -> bool {
        let cx = gx.div_euclid(chunk_size as i32);
        let cy = gy.div_euclid(chunk_size as i32);
        let lx = gx.rem_euclid(chunk_size as i32) as u32;
        let ly = gy.rem_euclid(chunk_size as i32) as u32;
        let data_cx = cx.rem_euclid(width_chunks);
        world_map
            .chunks
            .get(&(data_cx, cy))
            .map_or(true, |chunk| {
                let idx = (ly * chunk_size + lx) as usize;
                chunk
                    .fg
                    .tiles
                    .get(idx)
                    .map_or(true, |t| *t != crate::registry::tile::TileId::AIR)
            })
    };

    let particles = &mut *particles;
    let (positions, velocities) = (&mut particles.positions, &mut particles.velocities);
    for i in 0..positions.len() {
        resolve_tile_collision(&mut positions[i], &mut velocities[i], tile_size, &is_solid, 0.2);
        enforce_world_bounds(
            &mut positions[i],
            &mut velocities[i],
            0.0,
            world_width,
            0.0,
            world_height,
        );
    }

    // SPH particle reactions
    if let Some(rr) = reaction_registry {
        let (events, to_remove) =
            execute_sph_particle_reactions(&particles, sph_config.smoothing_radius, &rr);
        for evt in events {
            reaction_events.write(evt);
        }
        // Remove consumed particles in descending index order
        for idx in to_remove {
            particles.remove_swap(idx);
        }
    }
}
