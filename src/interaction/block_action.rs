use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_ecs_tilemap::prelude::*;

use crate::player::{Player, PLAYER_HEIGHT, PLAYER_WIDTH};
use crate::world::chunk::{tile_to_local, world_to_tile, ChunkCoord, WorldMap};
use crate::world::tile::TileType;
use crate::world::CHUNK_SIZE;
use crate::world::{wrap_chunk_x, TILE_SIZE, WORLD_WIDTH_TILES};

const BLOCK_REACH: f32 = 5.0; // tiles

pub fn block_interaction_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    player_query: Query<&Transform, With<Player>>,
    mut world_map: ResMut<WorldMap>,
    mut tilemap_query: Query<(&ChunkCoord, &mut TileStorage, Entity)>,
) {
    let left_click = mouse.just_pressed(MouseButton::Left);
    let right_click = mouse.just_pressed(MouseButton::Right);
    if !left_click && !right_click {
        return;
    }

    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_gt)) = camera_query.single() else {
        return;
    };
    let Ok(player_tf) = player_query.single() else {
        return;
    };

    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok(world_pos) = camera.viewport_to_world_2d(camera_gt, cursor_pos) else {
        return;
    };

    let (tile_x, tile_y) = world_to_tile(world_pos.x, world_pos.y);

    // Range check (wrap-aware on X axis)
    let player_tile_x = (player_tf.translation.x / TILE_SIZE).floor();
    let player_tile_y = (player_tf.translation.y / TILE_SIZE).floor();
    let raw_dx = (tile_x as f32 - player_tile_x).abs();
    let dx = raw_dx.min(WORLD_WIDTH_TILES as f32 - raw_dx); // shortest distance on ring
    let dy = (tile_y as f32 - player_tile_y).abs();
    if dx > BLOCK_REACH || dy > BLOCK_REACH {
        return;
    }

    // Compute data chunk coords (wrapped) and local tile position
    let wrapped_tile_x = crate::world::wrap_tile_x(tile_x);
    let data_chunk_x = wrapped_tile_x.div_euclid(CHUNK_SIZE as i32);
    let data_chunk_y = tile_y.div_euclid(CHUNK_SIZE as i32);
    let (local_x, local_y) = tile_to_local(tile_x, tile_y);
    let tile_pos = TilePos::new(local_x, local_y);

    if left_click {
        // Break block
        let current = world_map.get_tile(tile_x, tile_y);
        if !current.is_solid() {
            return;
        }

        world_map.set_tile(tile_x, tile_y, TileType::Air);

        // Update ALL display tilemaps that show this data chunk
        for (coord, mut storage, _entity) in &mut tilemap_query {
            if wrap_chunk_x(coord.x) == data_chunk_x && coord.y == data_chunk_y {
                if let Some(tile_entity) = storage.remove(&tile_pos) {
                    commands.entity(tile_entity).despawn();
                }
            }
        }
    } else if right_click {
        // Place block
        let current = world_map.get_tile(tile_x, tile_y);
        if current.is_solid() {
            return; // already solid
        }

        // Check player overlap — can't place where player is standing
        let half_w = PLAYER_WIDTH / 2.0;
        let half_h = PLAYER_HEIGHT / 2.0;
        let player_min_x = player_tf.translation.x - half_w;
        let player_max_x = player_tf.translation.x + half_w;
        let player_min_y = player_tf.translation.y - half_h;
        let player_max_y = player_tf.translation.y + half_h;
        let tile_min_x = tile_x as f32 * TILE_SIZE;
        let tile_max_x = tile_min_x + TILE_SIZE;
        let tile_min_y = tile_y as f32 * TILE_SIZE;
        let tile_max_y = tile_min_y + TILE_SIZE;
        if player_max_x > tile_min_x
            && player_min_x < tile_max_x
            && player_max_y > tile_min_y
            && player_min_y < tile_max_y
        {
            return; // overlaps player
        }

        let place_type = TileType::Dirt;
        world_map.set_tile(tile_x, tile_y, place_type);

        // Update ALL display tilemaps that show this data chunk
        for (coord, mut storage, entity) in &mut tilemap_query {
            if wrap_chunk_x(coord.x) == data_chunk_x && coord.y == data_chunk_y {
                let tilemap_id = TilemapId(entity);
                let color = place_type.color().unwrap();
                let tile_entity = commands
                    .spawn(TileBundle {
                        position: tile_pos,
                        tilemap_id,
                        texture_index: TileTextureIndex(0),
                        color: TileColor(color),
                        ..Default::default()
                    })
                    .id();
                commands.entity(entity).add_child(tile_entity);
                storage.set(&tile_pos, tile_entity);
            }
        }
    }
}
