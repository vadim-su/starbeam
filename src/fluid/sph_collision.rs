use bevy::math::Vec2;

/// Maximum particle speed in pixels/second. Prevents explosion from bad pressure.
pub const MAX_PARTICLE_SPEED: f32 = 300.0;

/// Friction applied to tangential velocity on surface contact (0=no friction, 1=full stop).
/// Water has very low friction — it slides freely along surfaces.
const SURFACE_FRICTION: f32 = 0.02;

/// Number of collision resolution iterations per particle.
const COLLISION_ITERATIONS: u32 = 3;

/// Small offset to push particles out of solid tiles.
const PUSH_EPSILON: f32 = 0.05;

/// Resolve a single particle against the tile grid.
/// Runs multiple iterations to handle being pushed into another solid.
/// Applies friction to tangential velocity on contact.
pub fn resolve_tile_collision(
    pos: &mut Vec2,
    vel: &mut Vec2,
    tile_size: f32,
    is_solid: &dyn Fn(i32, i32) -> bool,
    restitution: f32,
) {
    for _ in 0..COLLISION_ITERATIONS {
        if !resolve_solid_tile(pos, vel, tile_size, is_solid, restitution) {
            break;
        }
    }
    resolve_diagonal_corners(pos, vel, tile_size, is_solid, restitution);
}

/// Push particle out of the solid tile it's in. Returns true if a collision occurred.
fn resolve_solid_tile(
    pos: &mut Vec2,
    vel: &mut Vec2,
    tile_size: f32,
    is_solid: &dyn Fn(i32, i32) -> bool,
    restitution: f32,
) -> bool {
    let tx = (pos.x / tile_size).floor() as i32;
    let ty = (pos.y / tile_size).floor() as i32;

    if !is_solid(tx, ty) {
        return false;
    }

    let tile_left = tx as f32 * tile_size;
    let tile_right = tile_left + tile_size;
    let tile_bottom = ty as f32 * tile_size;
    let tile_top = tile_bottom + tile_size;

    let dist_left = pos.x - tile_left;
    let dist_right = tile_right - pos.x;
    let dist_bottom = pos.y - tile_bottom;
    let dist_top = tile_top - pos.y;

    // Find minimum penetration axis, preferring exits to non-solid neighbors
    let candidates = [
        (dist_left, -1i32, 0i32, true),   // exit left
        (dist_right, 1, 0, true),          // exit right
        (dist_bottom, 0, -1, false),       // exit down
        (dist_top, 0, 1, false),           // exit up
    ];

    // Try nearest edge with a non-solid neighbor first
    let mut best: Option<(f32, i32, i32, bool)> = None;
    for &(dist, dx, dy, is_x) in &candidates {
        let neighbor_free = !is_solid(tx + dx, ty + dy);
        if !neighbor_free {
            continue;
        }
        if best.is_none() || dist < best.unwrap().0 {
            best = Some((dist, dx, dy, is_x));
        }
    }

    if let Some((_dist, dx, dy, is_x)) = best {
        if is_x {
            // Horizontal exit
            if dx < 0 {
                pos.x = tile_left - PUSH_EPSILON;
            } else {
                pos.x = tile_right + PUSH_EPSILON;
            }
            // Reflect normal component, apply friction to tangential
            vel.x = if dx < 0 {
                -vel.x.abs() * restitution
            } else {
                vel.x.abs() * restitution
            };
            vel.y *= 1.0 - SURFACE_FRICTION;
        } else {
            // Vertical exit
            if dy < 0 {
                pos.y = tile_bottom - PUSH_EPSILON;
            } else {
                pos.y = tile_top + PUSH_EPSILON;
            }
            vel.y = if dy < 0 {
                -vel.y.abs() * restitution
            } else {
                vel.y.abs() * restitution
            };
            vel.x *= 1.0 - SURFACE_FRICTION;
        }
    } else {
        // Completely surrounded — kill velocity, push left as fallback
        pos.x = tile_left - PUSH_EPSILON;
        *vel = Vec2::ZERO;
    }

    true
}

/// Seal diagonal corners where two solid tiles share only a corner.
fn resolve_diagonal_corners(
    pos: &mut Vec2,
    vel: &mut Vec2,
    tile_size: f32,
    is_solid: &dyn Fn(i32, i32) -> bool,
    restitution: f32,
) {
    let tx = (pos.x / tile_size).floor() as i32;
    let ty = (pos.y / tile_size).floor() as i32;

    if is_solid(tx, ty) {
        return;
    }

    let corner_threshold = tile_size * 0.15;

    for &(dx, dy) in &[(-1_i32, -1_i32), (1, -1), (-1, 1), (1, 1)] {
        if !is_solid(tx + dx, ty) || !is_solid(tx, ty + dy) {
            continue;
        }

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
            let push_dir = to_particle / dist;
            *pos = Vec2::new(corner_x, corner_y) + push_dir * (corner_threshold + 0.01);

            let vel_toward_corner = vel.dot(-push_dir);
            if vel_toward_corner > 0.0 {
                *vel += push_dir * vel_toward_corner * (1.0 + restitution);
            }
            // Apply friction
            *vel *= 1.0 - SURFACE_FRICTION;
        }
    }
}

/// Clamp particle velocity to MAX_PARTICLE_SPEED.
pub fn clamp_velocity(vel: &mut Vec2) {
    let speed = vel.length();
    if speed > MAX_PARTICLE_SPEED {
        *vel *= MAX_PARTICLE_SPEED / speed;
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
        let mut pos = Vec2::new(12.0, 9.0);
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
    fn friction_reduces_tangential_velocity() {
        let mut pos = Vec2::new(12.0, 8.1); // just inside tile (1,1), near bottom
        let mut vel = Vec2::new(100.0, -10.0); // fast horizontal, slow vertical
        let is_solid = |x: i32, y: i32| -> bool { x == 1 && y == 1 };
        resolve_tile_collision(&mut pos, &mut vel, 8.0, &is_solid, 0.0);
        // Horizontal (tangential) velocity should be reduced by friction
        assert!(
            vel.x.abs() < 100.0,
            "Tangential velocity should be reduced by friction, got {}",
            vel.x
        );
    }

    #[test]
    fn multiple_iterations_handle_chain_push() {
        // Particle in solid, pushed left into another solid, should resolve
        let mut pos = Vec2::new(12.0, 4.5);
        let mut vel = Vec2::new(0.0, -10.0);
        // Tiles (1,0) and (0,0) are solid, only (1,1) and above are free
        let is_solid = |x: i32, y: i32| -> bool { y == 0 && (x == 0 || x == 1) };
        resolve_tile_collision(&mut pos, &mut vel, 8.0, &is_solid, 0.0);
        // Should end up above both solid tiles
        let ty = (pos.y / 8.0).floor() as i32;
        assert!(
            ty >= 1 || pos.y >= 8.0 - 0.1,
            "Particle should be pushed up above solid row, pos={:?}",
            pos
        );
    }

    #[test]
    fn velocity_clamped() {
        let mut vel = Vec2::new(500.0, 500.0);
        clamp_velocity(&mut vel);
        assert!(
            vel.length() <= MAX_PARTICLE_SPEED + 0.1,
            "Velocity should be clamped, got {}",
            vel.length()
        );
    }

    #[test]
    fn diagonal_corner_sealed_pushes_particle_away() {
        let mut pos = Vec2::new(7.8, 7.8);
        let mut vel = Vec2::new(10.0, 10.0);
        let is_solid = |x: i32, y: i32| -> bool {
            (x == 1 && y == 0) || (x == 0 && y == 1)
        };
        resolve_tile_collision(&mut pos, &mut vel, 8.0, &is_solid, 0.3);
        let dist_to_corner = ((pos.x - 8.0).powi(2) + (pos.y - 8.0).powi(2)).sqrt();
        assert!(
            dist_to_corner > 1.0,
            "Particle should be pushed away from sealed diagonal corner, dist={}",
            dist_to_corner
        );
    }

    #[test]
    fn diagonal_corner_not_sealed_when_only_one_neighbor_solid() {
        let mut pos = Vec2::new(7.8, 7.8);
        let mut vel = Vec2::new(10.0, 10.0);
        let is_solid = |x: i32, y: i32| -> bool { x == 1 && y == 0 };
        let original_pos = pos;
        resolve_tile_collision(&mut pos, &mut vel, 8.0, &is_solid, 0.3);
        assert_eq!(pos, original_pos, "Particle should not be moved");
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
