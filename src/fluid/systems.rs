use std::collections::HashSet;

use bevy::asset::RenderAssetUsages;
use bevy::ecs::message::MessageWriter;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dKey, MeshMaterial2d};

use crate::fluid::events::FluidReactionEvent;
use crate::fluid::sph_collision::{enforce_world_bounds, resolve_tile_collision};
use crate::fluid::sph_particle::ParticleStore;
use crate::fluid::sph_render::build_particle_mesh;
use crate::fluid::sph_simulation::{sph_step, SphConfig};
use crate::fluid::reactions::{execute_sph_particle_reactions, FluidReactionRegistry};
use crate::fluid::registry::FluidRegistry;
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
    /// Whether caustics are enabled (0 = off, 1 = on).
    #[uniform(2)]
    pub enable_caustics: u32,
    /// Whether shimmer is enabled (0 = off, 1 = on).
    #[uniform(2)]
    pub enable_shimmer: u32,
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
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

/// Marker component on chunk entities that own a fluid mesh overlay.
#[derive(Component)]
pub struct FluidMeshEntity;

/// Shared material handle for fluid mesh overlays.
#[derive(Resource)]
pub struct SharedFluidMaterial {
    pub handle: Handle<FluidMaterial>,
}

/// Create the shared FluidMaterial with a fallback 1x1 white lightmap.
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
        // f16 1.0 = 0x3C00 -> little-endian [0x00, 0x3C] per channel
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
            enable_caustics: 1,
            enable_shimmer: 1,
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
            mat.enable_caustics = if dbg.enable_caustics { 1 } else { 0 };
            mat.enable_shimmer = if dbg.enable_shimmer { 1 } else { 0 };
        }
    }
}

/// SPH particle-based fluid simulation system.
/// Particles are simulated with SPH physics and collided against the tile world.
/// How many frames to skip between reaction checks.
const REACTION_COOLDOWN_FRAMES: u32 = 10;

pub fn sph_fluid_simulation(
    _time: Res<Time>,
    sph_config: Res<SphConfig>,
    _accumulator: ResMut<FluidTickAccumulator>,
    mut particles: ResMut<ParticleStore>,
    world_map: Res<WorldMap>,
    active_world: Res<ActiveWorld>,
    reaction_registry: Option<Res<FluidReactionRegistry>>,
    mut reaction_events: MessageWriter<FluidReactionEvent>,
    mut reaction_frame_counter: Local<u32>,
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

    // SPH particle reactions (only every N frames to avoid redundant spatial hash builds)
    *reaction_frame_counter = reaction_frame_counter.wrapping_add(1);
    if *reaction_frame_counter % REACTION_COOLDOWN_FRAMES == 0 {
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
}

/// Rebuild fluid mesh overlays from SPH particles for loaded chunks.
///
/// For each loaded chunk, builds a particle mesh from nearby SPH particles
/// and spawns/updates mesh entities for rendering.
#[allow(clippy::too_many_arguments)]
pub fn fluid_rebuild_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    active_world: Res<ActiveWorld>,
    fluid_registry: Res<FluidRegistry>,
    loaded_chunks: Res<LoadedChunks>,
    fluid_material: Res<SharedFluidMaterial>,
    existing_fluid_meshes: Query<(Entity, &ChunkCoord), With<FluidMeshEntity>>,
    particles: Res<ParticleStore>,
    sph_config: Res<SphConfig>,
    debug_state: Option<Res<crate::fluid::debug_overlay::FluidDebugState>>,
    mut last_generation: Local<u64>,
) {
    if particles.is_empty() {
        // Remove all existing fluid mesh entities when no particles
        for (entity, _) in &existing_fluid_meshes {
            commands.entity(entity).despawn();
        }
        *last_generation = 0;
        return;
    }

    // Skip rebuild if particles haven't changed since last frame
    if particles.generation == *last_generation {
        return;
    }
    *last_generation = particles.generation;

    let chunk_size = active_world.chunk_size;
    let tile_size = active_world.tile_size;
    let particle_radius = debug_state
        .as_ref()
        .and_then(|ds| {
            if ds.particle_visual_radius > 0.0 {
                Some(ds.particle_visual_radius)
            } else {
                None
            }
        })
        .unwrap_or(sph_config.smoothing_radius * 0.5);

    // Track which display chunks get meshes this frame
    let mut chunks_with_mesh: HashSet<(i32, i32)> = HashSet::new();

    let existing_set: HashSet<(i32, i32)> = existing_fluid_meshes
        .iter()
        .map(|(_, c)| (c.x, c.y))
        .collect();

    for &(display_cx, cy) in loaded_chunks.map.keys() {
        // Compute world-space bounds for this chunk
        let chunk_world_min = Vec2::new(
            display_cx as f32 * chunk_size as f32 * tile_size,
            cy as f32 * chunk_size as f32 * tile_size,
        );
        let chunk_world_max = Vec2::new(
            (display_cx + 1) as f32 * chunk_size as f32 * tile_size,
            (cy + 1) as f32 * chunk_size as f32 * tile_size,
        );

        let Some(mesh) = build_particle_mesh(
            &particles,
            chunk_world_min,
            chunk_world_max,
            particle_radius,
            &fluid_registry,
        ) else {
            continue;
        };

        chunks_with_mesh.insert((display_cx, cy));
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

    // Remove fluid mesh entities for chunks that no longer have particles
    for (entity, coord) in &existing_fluid_meshes {
        if !chunks_with_mesh.contains(&(coord.x, coord.y)) {
            commands.entity(entity).despawn();
        }
    }
}
