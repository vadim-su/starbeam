use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::inventory::{Hotbar, Inventory};
use crate::item::{calculate_drops, DroppedItem, DroppedItemPhysics, ItemRegistry, SpawnParams};
use crate::player::Player;
use crate::registry::tile::TileId;
use crate::world::chunk::{
    update_bitmasks_around, world_to_tile, ChunkDirty, Layer, LoadedChunks, WorldMap,
};
use crate::world::ctx::WorldCtx;

const BLOCK_REACH: f32 = 5.0;

#[allow(clippy::too_many_arguments)]
pub fn block_interaction_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut player_query: Query<(&Transform, &Hotbar, &mut Inventory), With<Player>>,
    ctx: WorldCtx,
    mut world_map: ResMut<WorldMap>,
    loaded_chunks: Res<LoadedChunks>,
    item_registry: Res<ItemRegistry>,
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
    let Ok((player_tf, hotbar, mut inventory)) = player_query.single_mut() else {
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
        // Foreground layer interaction
        let Some(current) = world_map.get_tile(tile_x, tile_y, Layer::Fg, &ctx_ref) else {
            return;
        };

        if ctx_ref.tile_registry.is_solid(current) {
            // Break fg tile
            let tile_def = ctx_ref.tile_registry.get(current);
            let drops = calculate_drops(&tile_def.drops);
            world_map.set_tile(tile_x, tile_y, Layer::Fg, TileId::AIR, &ctx_ref);

            // Spawn drops
            let tile_center = Vec2::new(
                tile_x as f32 * ctx_ref.config.tile_size + ctx_ref.config.tile_size / 2.0,
                tile_y as f32 * ctx_ref.config.tile_size + ctx_ref.config.tile_size / 2.0,
            );
            for (item_id, count) in drops {
                let params = SpawnParams::random(tile_center);
                commands.spawn((
                    DroppedItem {
                        item_id,
                        count,
                        velocity: params.velocity(),
                        lifetime: Timer::from_seconds(300.0, TimerMode::Once),
                        magnetized: false,
                    },
                    DroppedItemPhysics::default(),
                    Transform::from_translation(tile_center.extend(1.0)),
                ));
            }
        } else {
            // Place fg tile from left hand of active hotbar slot
            let has_neighbor = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| {
                let nx = tile_x + dx;
                let ny = tile_y + dy;
                world_map
                    .get_tile(nx, ny, Layer::Fg, &ctx_ref)
                    .is_some_and(|t| ctx_ref.tile_registry.is_solid(t))
                    || world_map
                        .get_tile(nx, ny, Layer::Bg, &ctx_ref)
                        .is_some_and(|t| t != TileId::AIR)
            });
            if !has_neighbor {
                return;
            }

            let Some(item_id) = hotbar.slots[hotbar.active_slot].left_hand.as_deref() else {
                return;
            };
            let Some(place_id) = resolve_placeable(item_id, &item_registry, &ctx_ref) else {
                return;
            };
            if inventory.count_item(item_id) == 0 {
                return;
            }

            world_map.set_tile(tile_x, tile_y, Layer::Fg, place_id, &ctx_ref);
            inventory.remove_item(item_id, 1);
        }
    } else if right_click {
        // Background layer interaction
        let Some(current_bg) = world_map.get_tile(tile_x, tile_y, Layer::Bg, &ctx_ref) else {
            return;
        };

        if current_bg != TileId::AIR {
            // Break bg tile
            let tile_def = ctx_ref.tile_registry.get(current_bg);
            let drops = calculate_drops(&tile_def.drops);
            world_map.set_tile(tile_x, tile_y, Layer::Bg, TileId::AIR, &ctx_ref);

            // Spawn drops
            let tile_center = Vec2::new(
                tile_x as f32 * ctx_ref.config.tile_size + ctx_ref.config.tile_size / 2.0,
                tile_y as f32 * ctx_ref.config.tile_size + ctx_ref.config.tile_size / 2.0,
            );
            for (item_id, count) in drops {
                let params = SpawnParams::random(tile_center);
                commands.spawn((
                    DroppedItem {
                        item_id,
                        count,
                        velocity: params.velocity(),
                        lifetime: Timer::from_seconds(300.0, TimerMode::Once),
                        magnetized: false,
                    },
                    DroppedItemPhysics::default(),
                    Transform::from_translation(tile_center.extend(1.0)),
                ));
            }
        } else {
            // Place bg tile from right hand of active hotbar slot
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

            let Some(item_id) = hotbar.slots[hotbar.active_slot].right_hand.as_deref() else {
                return;
            };
            let Some(place_id) = resolve_placeable(item_id, &item_registry, &ctx_ref) else {
                return;
            };
            if inventory.count_item(item_id) == 0 {
                return;
            }

            world_map.set_tile(tile_x, tile_y, Layer::Bg, place_id, &ctx_ref);
            inventory.remove_item(item_id, 1);
        }
    } else {
        return;
    }

    // Update bitmasks for the modified layer
    let modified_layer = if left_click { Layer::Fg } else { Layer::Bg };
    let bitmask_dirty =
        update_bitmasks_around(&mut world_map, tile_x, tile_y, modified_layer, &ctx_ref);

    let all_dirty = bitmask_dirty;

    for (cx, cy) in all_dirty {
        for (&(display_cx, display_cy), entities) in &loaded_chunks.map {
            if ctx_ref.config.wrap_chunk_x(display_cx) == cx && display_cy == cy {
                commands.entity(entities.fg).insert(ChunkDirty);
                commands.entity(entities.bg).insert(ChunkDirty);
            }
        }
    }
}

/// Look up item_id → placeable tile name → TileId. Returns None if not placeable.
fn resolve_placeable(
    item_id: &str,
    item_registry: &ItemRegistry,
    ctx: &crate::world::ctx::WorldCtxRef<'_>,
) -> Option<TileId> {
    let item_def_id = item_registry.by_name(item_id);
    let item_def = item_registry.get(item_def_id);
    let tile_name = item_def.placeable.as_deref()?;
    Some(ctx.tile_registry.by_name(tile_name))
}
