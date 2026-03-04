use bevy::math::Vec2;

pub fn resolve_tile_collision(
    pos: &mut Vec2,
    vel: &mut Vec2,
    tile_size: f32,
    is_solid: &dyn Fn(i32, i32) -> bool,
    restitution: f32,
) {
    let tx = (pos.x / tile_size).floor() as i32;
    let ty = (pos.y / tile_size).floor() as i32;

    if is_solid(tx, ty) {
        let tile_left = tx as f32 * tile_size;
        let tile_right = tile_left + tile_size;
        let tile_bottom = ty as f32 * tile_size;
        let tile_top = tile_bottom + tile_size;

        let dist_left = (pos.x - tile_left).abs();
        let dist_right = (tile_right - pos.x).abs();
        let dist_bottom = (pos.y - tile_bottom).abs();
        let dist_top = (tile_top - pos.y).abs();

        let min_dist = dist_left.min(dist_right).min(dist_bottom).min(dist_top);

        if min_dist == dist_left && !is_solid(tx - 1, ty) {
            pos.x = tile_left - 0.01;
            vel.x = -vel.x.abs() * restitution;
        } else if min_dist == dist_right && !is_solid(tx + 1, ty) {
            pos.x = tile_right + 0.01;
            vel.x = vel.x.abs() * restitution;
        } else if min_dist == dist_bottom && !is_solid(tx, ty - 1) {
            pos.y = tile_bottom - 0.01;
            vel.y = -vel.y.abs() * restitution;
        } else if min_dist == dist_top && !is_solid(tx, ty + 1) {
            pos.y = tile_top + 0.01;
            vel.y = vel.y.abs() * restitution;
        } else {
            pos.x = tile_left - 0.01;
            *vel = Vec2::ZERO;
        }
    }

    // Diagonal corner sealing: prevent particles from slipping through diagonal
    // gaps where two solid tiles share only a corner.
    //
    // Re-read the tile after the primary push (the particle may have moved).
    let tx = (pos.x / tile_size).floor() as i32;
    let ty = (pos.y / tile_size).floor() as i32;

    // Only apply diagonal checks when the particle is in an air tile.
    if is_solid(tx, ty) {
        return;
    }

    let corner_threshold = tile_size * 0.15;

    // Check all 4 corners of the current air tile.
    // For each corner, if both axis-aligned neighbors are solid, the diagonal
    // gap is sealed and the particle must be pushed away from that corner.
    for &(dx, dy) in &[(-1_i32, -1_i32), (1, -1), (-1, 1), (1, 1)] {
        let neighbor_x_solid = is_solid(tx + dx, ty);
        let neighbor_y_solid = is_solid(tx, ty + dy);

        if !neighbor_x_solid || !neighbor_y_solid {
            continue;
        }

        // The corner world-space position where the two solid tiles meet.
        let corner_x = if dx < 0 {
            tx as f32 * tile_size
        } else {
            (tx + 1) as f32 * tile_size
        };
        let corner_y = if dy < 0 {
            ty as f32 * tile_size
        } else {
            (ty + 1) as f32 * tile_size
        };

        let to_particle = *pos - Vec2::new(corner_x, corner_y);
        let dist = to_particle.length();

        if dist < corner_threshold && dist > 1e-6 {
            // Push the particle away from the corner along the corner→particle
            // direction so it sits exactly at the threshold distance.
            let push_dir = to_particle / dist;
            *pos = Vec2::new(corner_x, corner_y) + push_dir * (corner_threshold + 0.01);

            // Reflect velocity component that points toward the corner.
            let vel_toward_corner = vel.dot(-push_dir);
            if vel_toward_corner > 0.0 {
                *vel += push_dir * vel_toward_corner * (1.0 + restitution);
            }
        }
    }
}

pub fn enforce_world_bounds(
    pos: &mut Vec2,
    vel: &mut Vec2,
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
) {
    if pos.x < min_x {
        pos.x = min_x;
        vel.x = vel.x.abs() * 0.3;
    } else if pos.x > max_x {
        pos.x = max_x;
        vel.x = -vel.x.abs() * 0.3;
    }
    if pos.y < min_y {
        pos.y = min_y;
        vel.y = vel.y.abs() * 0.3;
    } else if pos.y > max_y {
        pos.y = max_y;
        vel.y = -vel.y.abs() * 0.3;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec2;

    #[test]
    fn particle_above_floor_unaffected() {
        let mut pos = Vec2::new(50.0, 50.0);
        let mut vel = Vec2::new(0.0, -10.0);
        let is_solid = |_x: i32, _y: i32| -> bool { false };
        resolve_tile_collision(&mut pos, &mut vel, 8.0, &is_solid, 0.3);
        assert_eq!(pos, Vec2::new(50.0, 50.0));
    }

    #[test]
    fn particle_in_solid_pushed_out() {
        let mut pos = Vec2::new(12.0, 12.0);
        let mut vel = Vec2::new(0.0, -10.0);
        let is_solid = |x: i32, y: i32| -> bool { x == 1 && y == 1 };
        resolve_tile_collision(&mut pos, &mut vel, 8.0, &is_solid, 0.3);
        let tx = (pos.x / 8.0).floor() as i32;
        let ty = (pos.y / 8.0).floor() as i32;
        assert!(
            !(tx == 1 && ty == 1),
            "Particle should be outside solid tile"
        );
    }

    #[test]
    fn velocity_reflected_on_collision() {
        let mut pos = Vec2::new(12.0, 9.0); // Near bottom edge of tile (1,1)
        let mut vel = Vec2::new(0.0, -50.0);
        let is_solid = |x: i32, y: i32| -> bool { x == 1 && y == 1 };
        resolve_tile_collision(&mut pos, &mut vel, 8.0, &is_solid, 0.3);
        assert!(
            vel.length() < 50.0,
            "Velocity should be reduced after collision, got {}",
            vel.length()
        );
    }

    #[test]
    fn diagonal_corner_sealed_pushes_particle_away() {
        // Solid tiles at (1,0) and (0,1), air at (0,0) and (1,1).
        // The corner at world pos (8,8) should be sealed.
        // Place particle very close to that corner inside tile (0,0).
        let mut pos = Vec2::new(7.8, 7.8);
        let mut vel = Vec2::new(10.0, 10.0); // moving toward the corner
        let is_solid = |x: i32, y: i32| -> bool {
            (x == 1 && y == 0) || (x == 0 && y == 1)
        };
        resolve_tile_collision(&mut pos, &mut vel, 8.0, &is_solid, 0.3);
        // Particle should have been pushed away from the corner at (8,8)
        let dist_to_corner = ((pos.x - 8.0).powi(2) + (pos.y - 8.0).powi(2)).sqrt();
        assert!(
            dist_to_corner > 1.0,
            "Particle should be pushed away from sealed diagonal corner, dist={}",
            dist_to_corner
        );
    }

    #[test]
    fn diagonal_corner_not_sealed_when_only_one_neighbor_solid() {
        // Only one axis neighbor solid — diagonal is NOT sealed.
        let mut pos = Vec2::new(7.8, 7.8);
        let mut vel = Vec2::new(10.0, 10.0);
        let is_solid = |x: i32, y: i32| -> bool { x == 1 && y == 0 };
        let original_pos = pos;
        resolve_tile_collision(&mut pos, &mut vel, 8.0, &is_solid, 0.3);
        // Particle should be unaffected (it's in air and no sealed corner)
        assert_eq!(pos, original_pos, "Particle should not be moved");
    }

    #[test]
    fn diagonal_corner_velocity_reflected() {
        // Particle near sealed corner should have velocity reflected away.
        let mut pos = Vec2::new(7.9, 7.9);
        let mut vel = Vec2::new(20.0, 20.0); // heading toward corner
        let is_solid = |x: i32, y: i32| -> bool {
            (x == 1 && y == 0) || (x == 0 && y == 1)
        };
        resolve_tile_collision(&mut pos, &mut vel, 8.0, &is_solid, 0.3);
        // After reflection, velocity should point away from corner (negative components)
        assert!(
            vel.x < 0.0 || vel.y < 0.0,
            "Velocity should be reflected away from corner, got {:?}",
            vel
        );
    }

    #[test]
    fn world_boundary_bottom() {
        let mut pos = Vec2::new(50.0, -5.0);
        let mut vel = Vec2::new(0.0, -10.0);
        enforce_world_bounds(&mut pos, &mut vel, 0.0, 1000.0, 0.0, 500.0);
        assert!(pos.y >= 0.0);
        assert!(vel.y >= 0.0);
    }
}
