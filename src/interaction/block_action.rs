use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::math::{tile_aabb, Aabb};
use crate::player::Player;
use crate::registry::player::PlayerConfig;
use crate::registry::tile::TileId;
use crate::world::chunk::{
    update_bitmasks_around, world_to_tile, ChunkDirty, LoadedChunks, WorldMap,
};
use crate::world::ctx::WorldCtx;

const BLOCK_REACH: f32 = 5.0;

#[allow(clippy::too_many_arguments)]
pub fn block_interaction_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    player_query: Query<&Transform, With<Player>>,
    player_config: Res<PlayerConfig>,
    ctx: WorldCtx,
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

    let ctx_ref = ctx.as_ref();
    let (tile_x, tile_y) = world_to_tile(world_pos.x, world_pos.y, ctx_ref.config.tile_size);

    // Range check (wrap-aware on X axis)
    let player_tile_x = (player_tf.translation.x / ctx_ref.config.tile_size).floor();
    let player_tile_y = (player_tf.translation.y / ctx_ref.config.tile_size).floor();
    let raw_dx = (tile_x as f32 - player_tile_x).abs();
    let dx = raw_dx.min(ctx_ref.config.width_tiles as f32 - raw_dx);
    let dy = (tile_y as f32 - player_tile_y).abs();
    if dx > BLOCK_REACH || dy > BLOCK_REACH {
        return;
    }

    if left_click {
        // Break block (read-only check, skip if chunk not loaded)
        let Some(current) = world_map.get_tile(tile_x, tile_y, &ctx_ref) else {
            return;
        };
        if !ctx_ref.tile_registry.is_solid(current) {
            return;
        }

        world_map.set_tile(tile_x, tile_y, TileId::AIR, &ctx_ref);
    } else if right_click {
        // Place block (read-only check, skip if chunk not loaded)
        let Some(current) = world_map.get_tile(tile_x, tile_y, &ctx_ref) else {
            return;
        };
        if ctx_ref.tile_registry.is_solid(current) {
            return;
        }

        // Check player overlap
        let player_aabb = Aabb::from_center(
            player_tf.translation.x,
            player_tf.translation.y,
            player_config.width,
            player_config.height,
        );
        let target_tile = tile_aabb(tile_x, tile_y, ctx_ref.config.tile_size);
        if player_aabb.overlaps(&target_tile) {
            return;
        }

        // TODO: replace with player's selected block type from hotbar/inventory
        let place_id = ctx_ref.tile_registry.by_name("dirt");
        world_map.set_tile(tile_x, tile_y, place_id, &ctx_ref);
    } else {
        return;
    }

    // Update bitmasks and mark dirty chunks
    let dirty = update_bitmasks_around(&mut world_map, tile_x, tile_y, &ctx_ref);

    for (cx, cy) in dirty {
        // Find ALL loaded chunk entities that map to this data chunk
        for (&(display_cx, display_cy), &entity) in &loaded_chunks.map {
            if ctx_ref.config.wrap_chunk_x(display_cx) == cx && display_cy == cy {
                commands.entity(entity).insert(ChunkDirty);
            }
        }
    }
}
