use bevy::prelude::*;

use crate::player::{Grounded, Player, Velocity, PLAYER_HEIGHT, PLAYER_WIDTH};
use crate::world::chunk::WorldMap;
use crate::world::TILE_SIZE;

/// Player AABB from center position.
pub struct Aabb {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

impl Aabb {
    pub fn from_center(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            min_x: x - w / 2.0,
            max_x: x + w / 2.0,
            min_y: y - h / 2.0,
            max_y: y + h / 2.0,
        }
    }

    /// Tile coordinates that this AABB overlaps.
    pub fn overlapping_tiles(&self) -> impl Iterator<Item = (i32, i32)> {
        let min_tx = (self.min_x / TILE_SIZE).floor() as i32;
        let max_tx = ((self.max_x - 0.001) / TILE_SIZE).floor() as i32;
        let min_ty = (self.min_y / TILE_SIZE).floor() as i32;
        let max_ty = ((self.max_y - 0.001) / TILE_SIZE).floor() as i32;

        let mut tiles = Vec::new();
        for ty in min_ty..=max_ty {
            for tx in min_tx..=max_tx {
                tiles.push((tx, ty));
            }
        }
        tiles.into_iter()
    }
}

/// Tile AABB from tile coordinates.
pub fn tile_aabb(tx: i32, ty: i32) -> Aabb {
    Aabb {
        min_x: tx as f32 * TILE_SIZE,
        max_x: (tx + 1) as f32 * TILE_SIZE,
        min_y: ty as f32 * TILE_SIZE,
        max_y: (ty + 1) as f32 * TILE_SIZE,
    }
}

pub fn collision_system(
    time: Res<Time>,
    mut world_map: ResMut<WorldMap>,
    mut query: Query<(&mut Transform, &mut Velocity, &mut Grounded), With<Player>>,
) {
    let dt = time.delta_secs();

    for (mut transform, mut vel, mut grounded) in &mut query {
        let pos = &mut transform.translation;

        // --- Resolve X axis ---
        pos.x += vel.x * dt;
        let aabb = Aabb::from_center(pos.x, pos.y, PLAYER_WIDTH, PLAYER_HEIGHT);
        for (tx, ty) in aabb.overlapping_tiles() {
            if world_map.is_solid(tx, ty) {
                let tile = tile_aabb(tx, ty);
                let player = Aabb::from_center(pos.x, pos.y, PLAYER_WIDTH, PLAYER_HEIGHT);
                if player.max_x > tile.min_x
                    && player.min_x < tile.max_x
                    && player.max_y > tile.min_y
                    && player.min_y < tile.max_y
                {
                    if vel.x > 0.0 {
                        pos.x = tile.min_x - PLAYER_WIDTH / 2.0;
                    } else if vel.x < 0.0 {
                        pos.x = tile.max_x + PLAYER_WIDTH / 2.0;
                    }
                    vel.x = 0.0;
                }
            }
        }

        // --- Resolve Y axis ---
        pos.y += vel.y * dt;
        grounded.0 = false;
        let aabb = Aabb::from_center(pos.x, pos.y, PLAYER_WIDTH, PLAYER_HEIGHT);
        for (tx, ty) in aabb.overlapping_tiles() {
            if world_map.is_solid(tx, ty) {
                let tile = tile_aabb(tx, ty);
                let player = Aabb::from_center(pos.x, pos.y, PLAYER_WIDTH, PLAYER_HEIGHT);
                if player.max_x > tile.min_x
                    && player.min_x < tile.max_x
                    && player.max_y > tile.min_y
                    && player.min_y < tile.max_y
                {
                    if vel.y < 0.0 {
                        pos.y = tile.max_y + PLAYER_HEIGHT / 2.0;
                        grounded.0 = true;
                    } else if vel.y > 0.0 {
                        pos.y = tile.min_y - PLAYER_HEIGHT / 2.0;
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

    #[test]
    fn aabb_from_center() {
        let aabb = Aabb::from_center(100.0, 200.0, 24.0, 48.0);
        assert_eq!(aabb.min_x, 88.0);
        assert_eq!(aabb.max_x, 112.0);
        assert_eq!(aabb.min_y, 176.0);
        assert_eq!(aabb.max_y, 224.0);
    }

    #[test]
    fn overlapping_tiles_single() {
        let center_x = 3.0 * TILE_SIZE + TILE_SIZE / 2.0;
        let center_y = 3.0 * TILE_SIZE + TILE_SIZE / 2.0;
        let aabb = Aabb::from_center(center_x, center_y, 20.0, 20.0);
        let tiles: Vec<_> = aabb.overlapping_tiles().collect();
        assert_eq!(tiles, vec![(3, 3)]);
    }

    #[test]
    fn overlapping_tiles_multiple() {
        let aabb = Aabb::from_center(32.0, 32.0, 24.0, 48.0);
        let tiles: Vec<_> = aabb.overlapping_tiles().collect();
        assert!(tiles.len() >= 2);
        assert!(tiles.contains(&(0, 0)));
    }

    #[test]
    fn tile_aabb_basic() {
        let aabb = tile_aabb(3, 5);
        assert_eq!(aabb.min_x, 96.0);
        assert_eq!(aabb.max_x, 128.0);
        assert_eq!(aabb.min_y, 160.0);
        assert_eq!(aabb.max_y, 192.0);
    }
}
