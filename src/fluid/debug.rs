use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::fluid::cell::FluidId;
use crate::fluid::registry::FluidRegistry;
use crate::fluid::sph_particle::{Particle, ParticleStore};
use crate::registry::world::ActiveWorld;
use crate::world::chunk::world_to_tile;

/// Debug system: press F5 to place water, F6 to place steam, F7 to place lava at cursor.
///
/// Spawns SPH particles in a 3x3 tile area around the cursor position.
#[allow(clippy::too_many_arguments)]
pub fn debug_place_fluid(
    input: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    active_world: Res<ActiveWorld>,
    fluid_registry: Res<FluidRegistry>,
    mut sph_particles: ResMut<ParticleStore>,
) {
    let f5 = input.just_pressed(KeyCode::F5);
    let f6 = input.just_pressed(KeyCode::F6);
    let f7 = input.just_pressed(KeyCode::F7);
    if !f5 && !f6 && !f7 {
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
    let (tile_x, tile_y) = world_to_tile(world_pos.x, world_pos.y, tile_size);

    // Clamp to world bounds
    if tile_y < 0 || tile_y >= active_world.height_tiles {
        return;
    }
    let wrapped_x = active_world.wrap_tile_x(tile_x);

    // Determine which fluid to place
    let (fluid_id, fluid_name) = if f5 {
        (
            fluid_registry.try_by_name("water").unwrap_or(FluidId(1)),
            "water",
        )
    } else if f7 {
        (
            fluid_registry.try_by_name("lava").unwrap_or(FluidId(2)),
            "lava",
        )
    } else {
        (
            fluid_registry.try_by_name("steam").unwrap_or(FluidId(3)),
            "steam",
        )
    };

    // Spawn SPH particles in a 3x3 area
    let center = Vec2::new(
        wrapped_x as f32 * tile_size + tile_size * 0.5,
        tile_y as f32 * tile_size + tile_size * 0.5,
    );
    for dy in -1..=1_i32 {
        for dx in -1..=1_i32 {
            let base = center + Vec2::new(dx as f32 * tile_size, dy as f32 * tile_size);
            // 4 particles per tile for decent density
            for j in 0..4 {
                let offset = Vec2::new(
                    (j % 2) as f32 * tile_size * 0.4 - tile_size * 0.2,
                    (j / 2) as f32 * tile_size * 0.4 - tile_size * 0.2,
                );
                sph_particles.add(Particle::new(base + offset, fluid_id, 1.0));
            }
        }
    }

    info!(
        "Debug: placed {} SPH particles at tile ({}, {})",
        fluid_name, wrapped_x, tile_y
    );
}
