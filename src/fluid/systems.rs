use std::collections::{HashMap, HashSet};

use bevy::asset::RenderAssetUsages;
use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dKey, MeshMaterial2d};

use crate::fluid::cell::FluidCell;
use crate::fluid::events::{FluidReactionEvent, ImpactKind, WaterImpactEvent};
use crate::fluid::fluid_world::FluidWorld;
use crate::fluid::reactions::{
    execute_fluid_reactions_global, resolve_density_displacement_global, FluidReactionRegistry,
};
use crate::fluid::registry::FluidRegistry;
use crate::fluid::render::{build_chunk_quad, build_fluid_textures, make_r8_texture};
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

/// GPU-side uniform data for the metaball fluid shader.
///
/// Field order MUST match the `FluidUniforms` struct in `fluid.wgsl` exactly.
/// The `ShaderType` derive handles alignment automatically.
#[derive(Clone, ShaderType)]
pub struct FluidUniformData {
    pub lightmap_uv_rect: Vec4,
    pub time: f32,
    pub tile_size: f32,
    pub chunk_size: f32,
    pub threshold: f32,
    pub radius_min: f32,
    pub radius_max: f32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub fluid_color_0: Vec4,
    pub fluid_color_1: Vec4,
    pub fluid_color_2: Vec4,
    pub fluid_color_3: Vec4,
    pub fluid_color_4: Vec4,
    pub fluid_color_5: Vec4,
    pub fluid_color_6: Vec4,
    pub fluid_color_7: Vec4,
    pub fluid_emission_0: Vec4,
    pub fluid_emission_1: Vec4,
}

impl Default for FluidUniformData {
    fn default() -> Self {
        Self {
            lightmap_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
            time: 0.0,
            tile_size: 8.0,
            chunk_size: 32.0,
            threshold: 0.5,
            radius_min: 0.4,
            radius_max: 0.8,
            _pad0: 0.0,
            _pad1: 0.0,
            fluid_color_0: Vec4::ZERO,
            fluid_color_1: Vec4::ZERO,
            fluid_color_2: Vec4::ZERO,
            fluid_color_3: Vec4::ZERO,
            fluid_color_4: Vec4::ZERO,
            fluid_color_5: Vec4::ZERO,
            fluid_color_6: Vec4::ZERO,
            fluid_color_7: Vec4::ZERO,
            fluid_emission_0: Vec4::ZERO,
            fluid_emission_1: Vec4::ZERO,
        }
    }
}

/// Custom Material2d for metaball fluid rendering.
///
/// Each chunk gets its own material instance with unique density/fluid_id textures.
/// Shared uniforms (time, colors, lightmap) are synced from `SharedFluidMaterial`.
///
/// Bindings (all in @group(2)):
///   - texture(0) / sampler(1): density_texture (R8Unorm, nearest)
///   - texture(2) / sampler(3): fluid_id_texture (R8Unorm, nearest)
///   - texture(4) / sampler(5): lightmap
///   - uniform(6): FluidUniformData
#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct FluidMaterial {
    #[texture(0, sample_type = "float", dimension = "2d")]
    #[sampler(1, sampler_type = "non_filtering")]
    pub density_texture: Handle<Image>,
    #[texture(2, sample_type = "float", dimension = "2d")]
    #[sampler(3, sampler_type = "non_filtering")]
    pub fluid_id_texture: Handle<Image>,
    #[texture(4)]
    #[sampler(5)]
    pub lightmap: Handle<Image>,
    #[uniform(6)]
    pub uniforms: FluidUniformData,
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
            Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
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
/// Used as a template for uniforms (time, lightmap, colors) that get
/// synced to all per-chunk materials by `update_fluid_time`.
#[derive(Resource)]
pub struct SharedFluidMaterial {
    pub handle: Handle<FluidMaterial>,
}

/// Tracks per-chunk material handles for the metaball fluid system.
#[derive(Resource, Default)]
pub struct ChunkFluidMaterials {
    pub materials: HashMap<(i32, i32), Handle<FluidMaterial>>,
}

/// Create the shared FluidMaterial with fallback 1×1 placeholder textures.
pub fn init_fluid_material(
    mut commands: Commands,
    mut fluid_materials: ResMut<Assets<FluidMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

    // 1×1 white lightmap (Rgba16Float)
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

    // 1×1 placeholder density texture (R8Unorm, value 0)
    let placeholder_density = images.add(make_r8_texture(vec![0u8], 1, 1));
    // 1×1 placeholder fluid_id texture (R8Unorm, value 0)
    let placeholder_fluid_id = images.add(make_r8_texture(vec![0u8], 1, 1));

    commands.insert_resource(SharedFluidMaterial {
        handle: fluid_materials.add(FluidMaterial {
            density_texture: placeholder_density,
            fluid_id_texture: placeholder_fluid_id,
            lightmap: white_lightmap,
            uniforms: FluidUniformData::default(),
        }),
    });
}

/// Update the time, tile_size, chunk_size, and fluid colors/emission uniforms
/// on the shared FluidMaterial and all per-chunk materials each frame.
pub fn update_fluid_time(
    time: Res<Time>,
    shared: Res<SharedFluidMaterial>,
    mut materials: ResMut<Assets<FluidMaterial>>,
    active_world: Option<Res<ActiveWorld>>,
    fluid_registry: Option<Res<FluidRegistry>>,
    chunk_materials: Option<Res<ChunkFluidMaterials>>,
) {
    let t = time.elapsed_secs();
    let ts = active_world.as_ref().map(|aw| aw.tile_size).unwrap_or(8.0);
    let cs = active_world
        .as_ref()
        .map(|aw| aw.chunk_size as f32)
        .unwrap_or(32.0);

    // Build fluid colors/emission from registry
    let mut colors = [Vec4::ZERO; 8];
    let mut emission_0 = Vec4::ZERO;
    let mut emission_1 = Vec4::ZERO;
    if let Some(ref reg) = fluid_registry {
        for i in 0..reg.len().min(7) {
            let def = &reg.defs[i];
            let fid = i + 1; // FluidId starts at 1
            if fid < 8 {
                colors[fid] = Vec4::new(
                    def.color[0] as f32 / 255.0,
                    def.color[1] as f32 / 255.0,
                    def.color[2] as f32 / 255.0,
                    def.color[3] as f32 / 255.0,
                );
                let em = (def.light_emission[0] as f32
                    + def.light_emission[1] as f32
                    + def.light_emission[2] as f32)
                    / (255.0 * 3.0);
                if fid < 4 {
                    emission_0[fid] = em;
                } else {
                    emission_1[fid - 4] = em;
                }
            }
        }
    }

    // Helper closure to apply uniform updates to a material
    let apply_uniforms = |mat: &mut FluidMaterial| {
        mat.uniforms.time = t;
        mat.uniforms.tile_size = ts;
        mat.uniforms.chunk_size = cs;
        mat.uniforms.fluid_color_0 = colors[0];
        mat.uniforms.fluid_color_1 = colors[1];
        mat.uniforms.fluid_color_2 = colors[2];
        mat.uniforms.fluid_color_3 = colors[3];
        mat.uniforms.fluid_color_4 = colors[4];
        mat.uniforms.fluid_color_5 = colors[5];
        mat.uniforms.fluid_color_6 = colors[6];
        mat.uniforms.fluid_color_7 = colors[7];
        mat.uniforms.fluid_emission_0 = emission_0;
        mat.uniforms.fluid_emission_1 = emission_1;
    };

    // Update shared material
    if let Some(mat) = materials.get_mut(&shared.handle) {
        apply_uniforms(mat);
    }

    // Read lightmap handle and uv_rect from shared material to propagate
    let shared_lightmap = materials
        .get(&shared.handle)
        .map(|m| (m.lightmap.clone(), m.uniforms.lightmap_uv_rect));

    // Sync time, colors, lightmap to all per-chunk materials
    if let Some(ref cm) = chunk_materials {
        for handle in cm.materials.values() {
            if let Some(mat) = materials.get_mut(handle) {
                apply_uniforms(mat);
                // Propagate lightmap from shared material (updated by rc_lighting)
                if let Some((ref lm, lm_rect)) = shared_lightmap {
                    mat.lightmap = lm.clone();
                    mat.uniforms.lightmap_uv_rect = lm_rect;
                }
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
                old.fluid_id != new.fluid_id || (old.mass - new.mass).abs() >= CALM_MASS_EPSILON
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

/// Rebuild fluid mesh overlays for active fluid chunks using density-texture approach.
///
/// For each loaded chunk that has active fluids, creates density and fluid_id
/// textures with 1-cell padding from neighbors, then renders a single quad
/// per chunk with a metaball fragment shader.
#[allow(clippy::too_many_arguments)]
pub fn fluid_rebuild_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut fluid_materials: ResMut<Assets<FluidMaterial>>,
    world_map: Res<WorldMap>,
    active_world: Res<ActiveWorld>,
    active_fluids: Res<ActiveFluidChunks>,
    loaded_chunks: Res<LoadedChunks>,
    shared_material: Res<SharedFluidMaterial>,
    mut chunk_materials: ResMut<ChunkFluidMaterials>,
    existing_fluid_meshes: Query<(Entity, &ChunkCoord), With<FluidMeshEntity>>,
) {
    let chunk_size = active_world.chunk_size;
    let tile_size = active_world.tile_size;
    let width_chunks = active_world.width_chunks();

    // Get template uniforms from shared material
    let template_uniforms = fluid_materials
        .get(&shared_material.handle)
        .map(|m| m.uniforms.clone())
        .unwrap_or_default();
    let template_lightmap = fluid_materials
        .get(&shared_material.handle)
        .map(|m| m.lightmap.clone())
        .unwrap_or_default();

    // Build set of display chunks that need fluid meshes
    let mut display_chunks_with_fluid: HashSet<(i32, i32)> = HashSet::new();
    for &(display_cx, cy) in loaded_chunks.map.keys() {
        let data_cx = active_world.wrap_chunk_x(display_cx);
        if active_fluids.chunks.contains(&(data_cx, cy)) {
            display_chunks_with_fluid.insert((display_cx, cy));
        }
    }

    // Remove entities for chunks no longer needing fluid
    for (entity, coord) in &existing_fluid_meshes {
        if !display_chunks_with_fluid.contains(&(coord.x, coord.y)) {
            commands.entity(entity).despawn();
            let data_cx = active_world.wrap_chunk_x(coord.x);
            chunk_materials.materials.remove(&(data_cx, coord.y));
        }
    }

    let existing_set: HashSet<(i32, i32)> = existing_fluid_meshes
        .iter()
        .map(|(_, c)| (c.x, c.y))
        .collect();

    for &(display_cx, cy) in &display_chunks_with_fluid {
        let data_cx = active_world.wrap_chunk_x(display_cx);
        let Some(chunk) = world_map.chunks.get(&(data_cx, cy)) else {
            continue;
        };

        // Get neighbor fluids for padding
        let left_cx = (data_cx - 1).rem_euclid(width_chunks);
        let right_cx = (data_cx + 1).rem_euclid(width_chunks);
        let neighbor_left = world_map
            .chunks
            .get(&(left_cx, cy))
            .map(|c| c.fluids.as_slice());
        let neighbor_right = world_map
            .chunks
            .get(&(right_cx, cy))
            .map(|c| c.fluids.as_slice());
        let neighbor_above = world_map
            .chunks
            .get(&(data_cx, cy + 1))
            .map(|c| c.fluids.as_slice());
        let neighbor_below = if cy > 0 {
            world_map
                .chunks
                .get(&(data_cx, cy - 1))
                .map(|c| c.fluids.as_slice())
        } else {
            None
        };

        let Some((density_data, fluid_id_data, tex_size)) = build_fluid_textures(
            &chunk.fluids,
            chunk_size,
            neighbor_left,
            neighbor_right,
            neighbor_above,
            neighbor_below,
        ) else {
            continue;
        };

        // Create texture images
        let density_img = images.add(make_r8_texture(density_data, tex_size, tex_size));
        let fluid_id_img = images.add(make_r8_texture(fluid_id_data, tex_size, tex_size));

        // Create per-chunk material
        let mat_handle = fluid_materials.add(FluidMaterial {
            density_texture: density_img,
            fluid_id_texture: fluid_id_img,
            lightmap: template_lightmap.clone(),
            uniforms: template_uniforms.clone(),
        });
        chunk_materials
            .materials
            .insert((data_cx, cy), mat_handle.clone());

        if existing_set.contains(&(display_cx, cy)) {
            // Update existing entity's material (textures changed)
            for (entity, coord) in &existing_fluid_meshes {
                if coord.x == display_cx && coord.y == cy {
                    commands
                        .entity(entity)
                        .insert(MeshMaterial2d(mat_handle.clone()));
                    break;
                }
            }
        } else {
            // Spawn new entity with static quad
            let quad = build_chunk_quad(display_cx, cy, chunk_size, tile_size);
            let mesh_handle = meshes.add(quad);
            commands.spawn((
                ChunkCoord {
                    x: display_cx,
                    y: cy,
                },
                FluidMeshEntity,
                Mesh2d(mesh_handle),
                MeshMaterial2d(mat_handle),
                Transform::from_translation(Vec3::new(0.0, 0.0, 0.5)),
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
