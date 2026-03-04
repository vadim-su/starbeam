use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::fluid::cell::FluidCell;
use crate::fluid::registry::FluidRegistry;
use crate::fluid::simulation::DirtyFluidChunks;
use crate::world::chunk::{tile_to_chunk, world_to_tile, WorldMap};
use crate::world::ctx::WorldCtx;
use crate::world::rc_lighting::RcGridDirty;

const FLUID_NAMES: &[&str] = &["water", "lava", "steam", "toxic_gas", "smoke"];

/// Mass added per frame while pouring.
const POUR_RATE: f32 = 0.15;

/// Debug tool state for pouring fluids at the cursor.
#[derive(Resource, Default)]
pub struct FluidDebugTool {
    pub active: bool,
    pub fluid_index: usize,
}

/// F5: toggle fluid pour mode on/off.
pub fn toggle_fluid_tool(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut tool: ResMut<FluidDebugTool>,
) {
    if keyboard.just_pressed(KeyCode::F5) {
        tool.active = !tool.active;
        let name = FLUID_NAMES[tool.fluid_index];
        if tool.active {
            info!("Fluid tool ON: {name}");
        } else {
            info!("Fluid tool OFF");
        }
    }
}

/// F6: cycle to the next fluid type.
pub fn cycle_fluid_type(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut tool: ResMut<FluidDebugTool>,
) {
    if keyboard.just_pressed(KeyCode::F6) {
        tool.fluid_index = (tool.fluid_index + 1) % FLUID_NAMES.len();
        let name = FLUID_NAMES[tool.fluid_index];
        info!("Fluid type: {name}");
    }
}

/// While fluid tool is active and left mouse is held, pour fluid at cursor.
pub fn pour_fluid(
    tool: Res<FluidDebugTool>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    ctx: WorldCtx,
    mut world_map: ResMut<WorldMap>,
    fluid_registry: Option<Res<FluidRegistry>>,
    mut dirty_fluids: ResMut<DirtyFluidChunks>,
    mut rc_dirty: ResMut<RcGridDirty>,
) {
    if !tool.active || !mouse.pressed(MouseButton::Left) {
        return;
    }

    let Some(fluid_registry) = fluid_registry else {
        return;
    };

    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_gt)) = camera_query.single() else {
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

    // Don't pour into solid tiles
    if world_map.is_solid(tile_x, tile_y, &ctx_ref) {
        return;
    }

    let name = FLUID_NAMES[tool.fluid_index];
    let Some(fluid_id) = fluid_registry.try_by_name(name) else {
        warn!("Unknown fluid: {name}");
        return;
    };

    let current = world_map
        .get_fluid(tile_x, tile_y, &ctx_ref)
        .unwrap_or(FluidCell::EMPTY);

    // Only pour if empty or same fluid type
    if !current.is_empty() && current.fluid_id != fluid_id {
        return;
    }

    let new_mass = (current.mass + POUR_RATE).min(1.0);
    let cell = FluidCell::new(fluid_id, new_mass);
    world_map.set_fluid(tile_x, tile_y, cell, &ctx_ref);

    // Mark chunk dirty for fluid mesh rebuild
    let wrapped_x = ctx_ref.config.wrap_tile_x(tile_x);
    let (cx, cy) = tile_to_chunk(wrapped_x, tile_y, ctx_ref.config.chunk_size);
    dirty_fluids.0.insert((cx, cy));
    rc_dirty.0 = true;
}
