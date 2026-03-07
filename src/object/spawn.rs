use bevy::prelude::*;
use bevy::sprite_render::MeshMaterial2d;
use rand::Rng;

use super::definition::{ObjectId, ObjectType};
use super::plugin::{ObjectAnimation, ObjectSpriteMaterials};
use super::registry::ObjectRegistry;
use crate::crafting::CraftingStation;
use crate::world::chunk::WorldMap;
use crate::world::lit_sprite::{LitSprite, LitSpriteMaterial, SharedLitQuad};

/// Marker component linking a runtime entity to its ChunkData storage.
#[derive(Component)]
pub struct PlacedObjectEntity {
    pub data_chunk: (i32, i32),
    pub object_index: u16,
    pub object_id: ObjectId,
}

/// Marker linking to display chunk for despawn tracking.
#[derive(Component)]
pub struct ObjectDisplayChunk {
    pub display_chunk: (i32, i32),
}

/// Spawn entities for all objects in a chunk.
pub fn spawn_objects_for_chunk(
    commands: &mut Commands,
    world_map: &WorldMap,
    object_registry: &ObjectRegistry,
    object_sprites: Option<&ObjectSpriteMaterials>,
    quad: Option<&SharedLitQuad>,
    lit_materials: &mut Assets<LitSpriteMaterial>,
    data_chunk_x: i32,
    chunk_y: i32,
    display_chunk_x: i32,
    tile_size: f32,
    chunk_size: u32,
) {
    let Some(chunk) = world_map.chunk(data_chunk_x, chunk_y) else {
        return;
    };

    let display_offset_x = (display_chunk_x - data_chunk_x) as f32 * chunk_size as f32 * tile_size;

    let mut rng = rand::thread_rng();

    for (idx, obj) in chunk.objects.iter().enumerate() {
        if obj.object_id == ObjectId::NONE {
            continue;
        }

        let def = object_registry.get(obj.object_id);

        // World position of the anchor tile center
        let world_x = (data_chunk_x * chunk_size as i32 + obj.local_x as i32) as f32 * tile_size
            + tile_size / 2.0
            + display_offset_x;
        let world_y =
            (chunk_y * chunk_size as i32 + obj.local_y as i32) as f32 * tile_size + tile_size / 2.0;

        // Sprite offset for multi-tile objects: center sprite over all tiles
        let offset_x = (def.size.0 as f32 - 1.0) * tile_size / 2.0;
        let offset_y = (def.size.1 as f32 - 1.0) * tile_size / 2.0;

        // Background objects (trees, etc.) render behind the player, between
        // bg tiles (z=-1) and fg tiles (z=0). Foreground objects sit between
        // fg tiles (z=0) and dropped items (z=1).
        let z = if def.background { -0.5 } else { 0.5 };

        let mut entity_cmd = commands.spawn((
            PlacedObjectEntity {
                data_chunk: (data_chunk_x, chunk_y),
                object_index: idx as u16,
                object_id: obj.object_id,
            },
            ObjectDisplayChunk {
                display_chunk: (display_chunk_x, chunk_y),
            },
            Transform::from_translation(Vec3::new(world_x + offset_x, world_y + offset_y, z))
                .with_scale(Vec3::new(
                    def.size.0 as f32 * tile_size,
                    def.size.1 as f32 * tile_size,
                    1.0,
                )),
            Visibility::default(),
        ));

        if let ObjectType::CraftingStation { ref station_id } = def.object_type {
            entity_cmd.insert(CraftingStation {
                station_id: station_id.clone(),
                active_craft: None,
            });
        }

        if let (Some(sprites), Some(q)) = (object_sprites, quad) {
            if let Some(template_handle) = sprites.materials.get(&obj.object_id) {
                // Clone material for animated objects (each gets independent UV state),
                // share for non-animated.
                let mat_handle = if let Some(meta) = sprites.animation_meta.get(&obj.object_id) {
                    let cloned = lit_materials.get(template_handle).unwrap().clone();
                    let handle = lit_materials.add(cloned);

                    let start_frame = rng.gen_range(0..meta.total_frames);
                    let mut timer = Timer::from_seconds(1.0 / meta.fps, TimerMode::Repeating);
                    // Advance timer by a random fraction so entities tick at different times.
                    let random_elapsed = rng.gen_range(0.0..1.0 / meta.fps);
                    timer.tick(std::time::Duration::from_secs_f32(random_elapsed));

                    entity_cmd.insert(ObjectAnimation {
                        timer,
                        current_frame: start_frame,
                        total_frames: meta.total_frames,
                        columns: meta.columns,
                        rows: meta.rows,
                    });

                    // Set initial UV for the random start frame.
                    let col = start_frame / meta.rows;
                    let row = start_frame % meta.rows;
                    let scale_x = 1.0 / meta.columns as f32;
                    let scale_y = 1.0 / meta.rows as f32;
                    if let Some(mat) = lit_materials.get_mut(&handle) {
                        mat.sprite_uv_rect =
                            Vec4::new(scale_x, scale_y, col as f32 * scale_x, row as f32 * scale_y);
                    }

                    handle
                } else {
                    template_handle.clone()
                };

                entity_cmd.insert((LitSprite, Mesh2d(q.0.clone()), MeshMaterial2d(mat_handle)));
            }
        }
    }
}

/// Despawn all object entities for a given display chunk.
pub fn despawn_objects_for_chunk(
    commands: &mut Commands,
    query: &Query<(Entity, &ObjectDisplayChunk)>,
    display_chunk_x: i32,
    chunk_y: i32,
) {
    for (entity, display) in query.iter() {
        if display.display_chunk == (display_chunk_x, chunk_y) {
            commands.entity(entity).despawn();
        }
    }
}
