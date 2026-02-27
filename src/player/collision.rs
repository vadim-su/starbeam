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
