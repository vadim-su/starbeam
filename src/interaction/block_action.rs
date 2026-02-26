use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::player::Player;
use crate::registry::player::PlayerConfig;
use crate::registry::tile::{TerrainTiles, TileId, TileRegistry};
use crate::registry::world::WorldConfig;
use crate::world::chunk::{
    update_bitmasks_around, world_to_tile, ChunkDirty, LoadedChunks, WorldMap,
};

const BLOCK_REACH: f32 = 5.0;

#[allow(clippy::too_many_arguments)]
pub fn block_interaction_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    player_query: Query<&Transform, With<Player>>,
    player_config: Res<PlayerConfig>,
    world_config: Res<WorldConfig>,
    terrain_tiles: Res<TerrainTiles>,
    tile_registry: Res<TileRegistry>,
    mut world_map: ResMut<WorldMap>,
    loaded_chunks: Res<LoadedChunks>,
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

    let (tile_x, tile_y) = world_to_tile(world_pos.x, world_pos.y, world_config.tile_size);

    // Range check (wrap-aware on X axis)
    let player_tile_x = (player_tf.translation.x / world_config.tile_size).floor();
    let player_tile_y = (player_tf.translation.y / world_config.tile_size).floor();
    let raw_dx = (tile_x as f32 - player_tile_x).abs();
    let dx = raw_dx.min(world_config.width_tiles as f32 - raw_dx);
    let dy = (tile_y as f32 - player_tile_y).abs();
    if dx > BLOCK_REACH || dy > BLOCK_REACH {
        return;
    }

    if left_click {
        // Break block
        let current = world_map.get_tile(tile_x, tile_y, &world_config, &terrain_tiles);
        if !tile_registry.is_solid(current) {
            return;
        }

        world_map.set_tile(tile_x, tile_y, TileId::AIR, &world_config, &terrain_tiles);
    } else if right_click {
        // Place block
        let current = world_map.get_tile(tile_x, tile_y, &world_config, &terrain_tiles);
        if tile_registry.is_solid(current) {
            return;
        }

        // Check player overlap
        let half_w = player_config.width / 2.0;
        let half_h = player_config.height / 2.0;
        let player_min_x = player_tf.translation.x - half_w;
        let player_max_x = player_tf.translation.x + half_w;
        let player_min_y = player_tf.translation.y - half_h;
        let player_max_y = player_tf.translation.y + half_h;
        let tile_min_x = tile_x as f32 * world_config.tile_size;
        let tile_max_x = tile_min_x + world_config.tile_size;
        let tile_min_y = tile_y as f32 * world_config.tile_size;
        let tile_max_y = tile_min_y + world_config.tile_size;
        if player_max_x > tile_min_x
            && player_min_x < tile_max_x
            && player_max_y > tile_min_y
            && player_min_y < tile_max_y
        {
            return;
        }

        // TODO: replace with player's selected block type from hotbar/inventory
        let place_id = tile_registry.by_name("dirt");
        world_map.set_tile(tile_x, tile_y, place_id, &world_config, &terrain_tiles);
    } else {
        return;
    }

    // Update bitmasks and mark dirty chunks
    let dirty = update_bitmasks_around(
        &mut world_map,
        tile_x,
        tile_y,
        &world_config,
        &terrain_tiles,
        &tile_registry,
    );

    for (cx, cy) in dirty {
        // Find ALL loaded chunk entities that map to this data chunk
        for (&(display_cx, display_cy), &entity) in &loaded_chunks.map {
            if world_config.wrap_chunk_x(display_cx) == cx && display_cy == cy {
                commands.entity(entity).insert(ChunkDirty);
            }
        }
    }
}
