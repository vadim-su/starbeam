use std::collections::HashSet;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::player::Player;
use crate::registry::tile::TileId;
use crate::world::chunk::{
    update_bitmasks_around, world_to_tile, ChunkDirty, Layer, LoadedChunks, WorldMap,
};
use crate::world::ctx::WorldCtx;
use crate::world::lighting;

const BLOCK_REACH: f32 = 5.0;

#[allow(clippy::too_many_arguments)]
pub fn block_interaction_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    player_query: Query<&Transform, With<Player>>,
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
        let Some(current) = world_map.get_tile(tile_x, tile_y, Layer::Fg, &ctx_ref) else {
            return;
        };
        if !ctx_ref.tile_registry.is_solid(current) {
            return;
        }

        world_map.set_tile(tile_x, tile_y, Layer::Fg, TileId::AIR, &ctx_ref);
    } else if right_click {
        // Background layer interaction
        let Some(current_bg) = world_map.get_tile(tile_x, tile_y, Layer::Bg, &ctx_ref) else {
            return;
        };

        if current_bg != TileId::AIR {
            // Break bg tile
            world_map.set_tile(tile_x, tile_y, Layer::Bg, TileId::AIR, &ctx_ref);
        } else {
            // Place bg tile — must be adjacent to an existing tile (fg or bg)
            let has_neighbor = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| {
                let nx = tile_x + dx;
                let ny = tile_y + dy;
                world_map
                    .get_tile(nx, ny, Layer::Fg, &ctx_ref)
                    .is_some_and(|t| t != TileId::AIR)
                    || world_map
                        .get_tile(nx, ny, Layer::Bg, &ctx_ref)
                        .is_some_and(|t| t != TileId::AIR)
            });
            if !has_neighbor {
                return;
            }

            // TODO: replace with player's selected block type from inventory
            let place_id = ctx_ref.tile_registry.by_name("dirt");
            world_map.set_tile(tile_x, tile_y, Layer::Bg, place_id, &ctx_ref);
        }
    } else {
        return;
    }

    // Update bitmasks for the modified layer
    let modified_layer = if left_click { Layer::Fg } else { Layer::Bg };
    let bitmask_dirty =
        update_bitmasks_around(&mut world_map, tile_x, tile_y, modified_layer, &ctx_ref);

    // Recompute lighting for affected area
    let light_dirty = lighting::relight_around(&mut world_map, tile_x, tile_y, &ctx_ref);

    // Merge dirty sets and mark chunks for mesh rebuild
    let all_dirty: HashSet<(i32, i32)> = bitmask_dirty.union(&light_dirty).copied().collect();

    for (cx, cy) in all_dirty {
        for (&(display_cx, display_cy), entities) in &loaded_chunks.map {
            if ctx_ref.config.wrap_chunk_x(display_cx) == cx && display_cy == cy {
                commands.entity(entities.fg).insert(ChunkDirty);
                commands.entity(entities.bg).insert(ChunkDirty);
            }
        }
    }
}
