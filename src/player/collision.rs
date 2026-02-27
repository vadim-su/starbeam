use bevy::prelude::*;

use crate::math::{tile_aabb, Aabb};
use crate::player::{Grounded, Player, Velocity, MAX_DELTA_SECS};
use crate::registry::player::PlayerConfig;
use crate::world::chunk::WorldMap;
use crate::world::ctx::WorldCtx;

pub fn collision_system(
    time: Res<Time>,
    player_config: Res<PlayerConfig>,
    ctx: WorldCtx,
    world_map: Res<WorldMap>,
    mut query: Query<(&mut Transform, &mut Velocity, &mut Grounded), With<Player>>,
) {
    let dt = time.delta_secs().min(MAX_DELTA_SECS);
    let pw = player_config.width;
    let ph = player_config.height;
    let ts = ctx.config.tile_size;
    let ctx_ref = ctx.as_ref();

    for (mut transform, mut vel, mut grounded) in &mut query {
        let pos = &mut transform.translation;

        // --- Resolve X axis ---
        pos.x += vel.x * dt;
        let aabb = Aabb::from_center(pos.x, pos.y, pw, ph);
        for (tx, ty) in aabb.overlapping_tiles(ts) {
            if world_map.is_solid(tx, ty, &ctx_ref) {
                let tile = tile_aabb(tx, ty, ts);
                let player = Aabb::from_center(pos.x, pos.y, pw, ph);
                if player.overlaps(&tile) {
                    if vel.x > 0.0 {
                        pos.x = tile.min_x - pw / 2.0;
                    } else if vel.x < 0.0 {
                        pos.x = tile.max_x + pw / 2.0;
                    }
                    vel.x = 0.0;
                }
            }
        }

        // --- Resolve Y axis ---
        pos.y += vel.y * dt;
        grounded.0 = false;
        let aabb = Aabb::from_center(pos.x, pos.y, pw, ph);
        for (tx, ty) in aabb.overlapping_tiles(ts) {
            if world_map.is_solid(tx, ty, &ctx_ref) {
                let tile = tile_aabb(tx, ty, ts);
                let player = Aabb::from_center(pos.x, pos.y, pw, ph);
                if player.overlaps(&tile) {
                    if vel.y < 0.0 {
                        pos.y = tile.max_y + ph / 2.0;
                        grounded.0 = true;
                    } else if vel.y > 0.0 {
                        pos.y = tile.min_y - ph / 2.0;
                    }
                    vel.y = 0.0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::fixtures;
    use crate::world::chunk::WorldMap;
    use crate::world::terrain_gen;

    #[test]
    fn collision_no_crash_on_empty_world() {
        let mut app = fixtures::test_app();
        app.add_systems(Update, collision_system);

        // Spawn player floating in empty world (no chunks loaded)
        app.world_mut().spawn((
            Player,
            Transform::from_xyz(500.0, 30000.0, 0.0),
            Velocity { x: 0.0, y: -100.0 },
            Grounded(false),
        ));

        // Should not panic — WorldMap is empty, is_solid returns false
        app.update();

        let mut query = app.world_mut().query::<&Grounded>();
        let grounded = query.iter(app.world()).next().unwrap();
        assert!(!grounded.0, "should not be grounded in empty world");
    }

    #[test]
    fn collision_grounds_player_on_solid_surface() {
        let mut app = fixtures::test_app();
        app.add_systems(Update, collision_system);

        // Pre-generate chunks around surface using separate resource copies
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let surface_y = terrain_gen::surface_height(
            &nc,
            0,
            &wc,
            pc.layers.surface.terrain_frequency,
            pc.layers.surface.terrain_amplitude,
        );
        let chunk_size = wc.chunk_size as i32;
        let surface_chunk_y = surface_y.div_euclid(chunk_size);

        let mut world_map = WorldMap::default();
        // Generate a few chunks around the surface
        for cy in (surface_chunk_y - 1)..=(surface_chunk_y + 1) {
            world_map.get_or_generate_chunk(0, cy, &ctx);
        }

        // Insert pre-generated WorldMap
        *app.world_mut().resource_mut::<WorldMap>() = world_map;

        // Position player so bottom edge is slightly INSIDE the surface tile.
        // Surface tile spans [surface_y * ts .. (surface_y+1) * ts].
        // Player bottom = pos.y - h/2; we want it 2px inside the tile top.
        let tile_size = wc.tile_size;
        let player_height = fixtures::test_player_config().height;
        let spawn_y = (surface_y + 1) as f32 * tile_size + player_height / 2.0 - 2.0;

        app.world_mut().spawn((
            Player,
            Transform::from_xyz(tile_size / 2.0, spawn_y, 0.0),
            Velocity { x: 0.0, y: -200.0 },
            Grounded(false),
        ));

        // Even with dt=0, collision resolves because player already overlaps
        // the surface tile and vel.y < 0.
        app.update();

        let mut query = app.world_mut().query::<&Grounded>();
        let grounded = query.iter(app.world()).next().unwrap();
        assert!(
            grounded.0,
            "player should be grounded after landing on solid surface"
        );
    }
}
