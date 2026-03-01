use bevy::prelude::*;
use bevy::sprite_render::MeshMaterial2d;

use super::definition::ObjectId;
use super::plugin::ObjectSpriteMaterials;
use super::registry::ObjectRegistry;
use crate::world::chunk::WorldMap;
use crate::world::lit_sprite::{LitSprite, SharedLitQuad};

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

        // Z = 0.5 (between fg tiles at 0.0 and dropped items at 1.0)
        let z = 0.5;

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

        if let (Some(sprites), Some(q)) = (object_sprites, quad) {
            if let Some(mat_handle) = sprites.materials.get(&obj.object_id) {
                entity_cmd.insert((
                    LitSprite,
                    Mesh2d(q.0.clone()),
                    MeshMaterial2d(mat_handle.clone()),
                ));
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
