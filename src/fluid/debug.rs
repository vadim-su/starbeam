use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::fluid::cell::FluidCell;
use crate::fluid::registry::FluidRegistry;
use crate::fluid::systems::ActiveFluidChunks;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::{tile_to_chunk, world_to_tile, WorldMap};

/// Names of fluids available for debug placement, cycled with F6.
const FLUID_NAMES: &[&str] = &["water", "steam", "lava"];

/// Persistent state for the debug fluid placement tool.
#[derive(Resource)]
pub struct FluidPlacementMode {
    /// Whether placement mode is active (toggled with F5).
    pub enabled: bool,
    /// Index into [`FLUID_NAMES`] (cycled with F6).
    pub fluid_index: usize,
}

impl Default for FluidPlacementMode {
    fn default() -> Self {
        Self {
            enabled: false,
            fluid_index: 0,
        }
    }
}

/// Debug system: F5 toggles fluid placement mode, F6 cycles fluid type.
/// While placement mode is active, holding a mouse button places fluid at the
/// cursor position (3x3 area).
#[allow(clippy::too_many_arguments)]
pub fn debug_place_fluid(
    input: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut world_map: ResMut<WorldMap>,
    active_world: Res<ActiveWorld>,
    fluid_registry: Res<FluidRegistry>,
    mut active_fluids: ResMut<ActiveFluidChunks>,
    mut mode: ResMut<FluidPlacementMode>,
) {
    // F5: toggle placement mode
    if input.just_pressed(KeyCode::F5) {
        mode.enabled = !mode.enabled;
        let name = FLUID_NAMES[mode.fluid_index];
        if mode.enabled {
            info!("Fluid placement ON  [{}]", name);
        } else {
            info!("Fluid placement OFF");
        }
    }

    // F6: cycle fluid type
    if input.just_pressed(KeyCode::F6) {
        mode.fluid_index = (mode.fluid_index + 1) % FLUID_NAMES.len();
        let name = FLUID_NAMES[mode.fluid_index];
        info!("Fluid type: {}", name);
    }

    // Place fluid while mode is active and mouse button is held
    if !mode.enabled || !mouse.pressed(MouseButton::Left) {
        return;
    }

    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_gt)) = camera.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok(world_pos) = camera.viewport_to_world_2d(camera_gt, cursor_pos) else {
        return;
    };

    let tile_size = active_world.tile_size;
    let chunk_size = active_world.chunk_size;
    let (tile_x, tile_y) = world_to_tile(world_pos.x, world_pos.y, tile_size);

    // Clamp to world bounds
    if tile_y < 0 || tile_y >= active_world.height_tiles {
        return;
    }
    let wrapped_x = active_world.wrap_tile_x(tile_x);
    let (cx, cy) = tile_to_chunk(wrapped_x, tile_y, chunk_size);

    // Resolve fluid id from registry
    let fluid_name = FLUID_NAMES[mode.fluid_index];
    let Some(fluid_id) = fluid_registry.try_by_name(fluid_name) else {
        warn!("Unknown fluid '{}' in registry", fluid_name);
        return;
    };

    // Place fluid in a 3x3 area for more visible effect
    for dy in -1..=1_i32 {
        for dx in -1..=1_i32 {
            let tx = wrapped_x + dx;
            let ty = tile_y + dy;
            if ty < 0 || ty >= active_world.height_tiles {
                continue;
            }
            let tx = active_world.wrap_tile_x(tx);
            let (fcx, fcy) = tile_to_chunk(tx, ty, chunk_size);

            if let Some(chunk) = world_map.chunks.get_mut(&(fcx, fcy)) {
                let lx = tx.rem_euclid(chunk_size as i32) as u32;
                let ly = ty.rem_euclid(chunk_size as i32) as u32;
                let idx = (ly * chunk_size + lx) as usize;

                // Only place in non-solid, empty fluid cells
                if idx < chunk.fluids.len() && chunk.fluids[idx].is_empty() {
                    chunk.fluids[idx] = FluidCell::new(fluid_id, 1.0);
                    active_fluids.chunks.insert((fcx, fcy));
                }
            }
        }
    }

    // Also ensure the center chunk is active
    active_fluids.chunks.insert((cx, cy));
}
