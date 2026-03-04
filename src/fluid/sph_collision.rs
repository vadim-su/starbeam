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

    if !is_solid(tx, ty) {
        return;
    }

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
    fn world_boundary_bottom() {
        let mut pos = Vec2::new(50.0, -5.0);
        let mut vel = Vec2::new(0.0, -10.0);
        enforce_world_bounds(&mut pos, &mut vel, 0.0, 1000.0, 0.0, 500.0);
        assert!(pos.y >= 0.0);
        assert!(vel.y >= 0.0);
    }
}
