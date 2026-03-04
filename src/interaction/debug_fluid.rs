use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::fluid::active::ActiveFluids;
use crate::fluid::cell::{FluidCell, FluidId};
use crate::player::Player;
use crate::world::chunk::{world_to_tile, ChunkFluidDirty, LoadedChunks, WorldMap};
use crate::world::ctx::WorldCtx;

const DEBUG_WATER_LEVEL: u8 = 255;

/// Debug system: middle-click places/removes water at cursor position.
#[allow(clippy::too_many_arguments)]
pub fn debug_fluid_place_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    player_query: Query<&Transform, With<Player>>,
    ctx: WorldCtx,
    mut world_map: ResMut<WorldMap>,
    loaded_chunks: Res<LoadedChunks>,
    mut active_fluids: ResMut<ActiveFluids>,
) {
    let middle_click = mouse.just_pressed(MouseButton::Middle);
    if !middle_click {
        return;
    }

    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_gt)) = camera_query.single() else {
        return;
    };
    let Ok(_player_tf) = player_query.single() else {
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

    // Don't place water in solid tiles
    if world_map.is_solid(tile_x, tile_y, &ctx_ref) {
        return;
    }

    let existing = world_map
        .get_fluid(tile_x, tile_y, &ctx_ref)
        .unwrap_or_default();

    // Toggle: if water exists, remove it; otherwise place it.
    // Hold Shift to place lava instead of water.
    let new_cell = if !existing.is_empty() {
        FluidCell::default()
    } else {
        let fluid_id = if keyboard.pressed(KeyCode::ShiftLeft)
            || keyboard.pressed(KeyCode::ShiftRight)
        {
            FluidId(2) // lava
        } else {
            FluidId(1) // water
        };
        FluidCell {
            fluid_id,
            level: DEBUG_WATER_LEVEL,
        }
    };

    world_map.set_fluid(tile_x, tile_y, new_cell, &ctx_ref);
    active_fluids.wake_with_neighbors(tile_x, tile_y);

    // Mark fluid chunk dirty for mesh rebuild
    let wrapped_x = ctx_ref.config.wrap_tile_x(tile_x);
    let (cx, cy) = crate::world::chunk::tile_to_chunk(wrapped_x, tile_y, ctx_ref.config.chunk_size);
    for (&(display_cx, display_cy), entities) in &loaded_chunks.map {
        if ctx_ref.config.wrap_chunk_x(display_cx) == cx && display_cy == cy {
            commands.entity(entities.fluid).insert(ChunkFluidDirty);
        }
    }
}
