use bevy::prelude::*;

/// A dropped item entity in the world.
#[derive(Component, Debug)]
pub struct DroppedItem {
    pub item_id: String,
    pub count: u16,
    pub velocity: Vec2,
    pub lifetime: Timer,
    pub magnetized: bool,
}

/// Physics parameters for dropped items.
#[derive(Component, Debug, Clone, Copy)]
pub struct DroppedItemPhysics {
    pub gravity: f32,
    pub friction: f32,
    pub bounce: f32,
}

impl Default for DroppedItemPhysics {
    fn default() -> Self {
        Self {
            gravity: 400.0,
            friction: 0.9,
            bounce: 0.3,
        }
    }
}

/// Configuration for item pickup behavior.
#[derive(Resource, Debug, Clone)]
pub struct PickupConfig {
    pub magnet_radius: f32,
    pub magnet_strength: f32,
    pub pickup_radius: f32,
}

impl Default for PickupConfig {
    fn default() -> Self {
        Self {
            magnet_radius: 48.0, // 3 tiles
            magnet_strength: 200.0,
            pickup_radius: 16.0, // 1 tile
        }
    }
}

/// Parameters for spawning a dropped item.
pub struct SpawnParams {
    pub position: Vec2,
    pub angle: f32,
    pub speed: f32,
}

impl SpawnParams {
    /// Create spawn params with random angle (60°-150°) and speed (80-150).
    pub fn random(position: Vec2) -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let angle = rng.gen_range(0.6..2.5); // ~60°-150° in radians
        let speed = rng.gen_range(80.0..150.0);
        Self {
            position,
            angle,
            speed,
        }
    }

    /// Calculate initial velocity from angle and speed.
    pub fn velocity(&self) -> Vec2 {
        Vec2::new(self.angle.cos(), self.angle.sin()) * self.speed
    }
}

/// Apply gravity to velocity (pure function for testing).
pub fn apply_gravity(velocity: Vec2, gravity: f32, delta: f32) -> Vec2 {
    Vec2::new(velocity.x, velocity.y - gravity * delta)
}

/// Apply friction to velocity (pure function for testing).
pub fn apply_friction(velocity: Vec2, friction: f32) -> Vec2 {
    Vec2::new(velocity.x * friction, velocity.y * friction)
}

/// Apply bounce on collision (pure function for testing).
pub fn apply_bounce(velocity: Vec2, bounce: f32, hit_ground: bool) -> Vec2 {
    if hit_ground && velocity.y < 0.0 {
        Vec2::new(velocity.x * 0.9, -velocity.y * bounce)
    } else {
        velocity
    }
}

/// System that updates dropped item physics.
pub fn dropped_item_physics_system(
    time: Res<Time>,
    mut query: Query<(&mut DroppedItem, &DroppedItemPhysics, &mut Transform)>,
) {
    let delta = time.delta_secs();

    for (mut item, physics, mut transform) in &mut query {
        // Apply gravity
        item.velocity = apply_gravity(item.velocity, physics.gravity, delta);

        // Update position
        transform.translation.x += item.velocity.x * delta;
        transform.translation.y += item.velocity.y * delta;

        // Apply friction when moving slowly
        if item.velocity.length() < 10.0 {
            item.velocity = apply_friction(item.velocity, physics.friction);
        }

        // Update lifetime
        item.lifetime.tick(time.delta());
    }
}

/// Calculate drops from a tile definition.
pub fn calculate_drops(tile_drops: &[crate::item::DropDef]) -> Vec<(String, u16)> {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    tile_drops
        .iter()
        .filter_map(|drop| {
            if rng.gen_range(0.0..1.0) < drop.chance {
                let count = rng.gen_range(drop.min..=drop.max);
                Some((drop.item_id.clone(), count))
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_item_has_required_fields() {
        let item = DroppedItem {
            item_id: "dirt".into(),
            count: 5,
            velocity: Vec2::ZERO,
            lifetime: Timer::from_seconds(300.0, TimerMode::Once),
            magnetized: false,
        };

        assert_eq!(item.item_id, "dirt");
        assert_eq!(item.count, 5);
        assert!(!item.magnetized);
    }

    #[test]
    fn dropped_item_physics_defaults() {
        let physics = DroppedItemPhysics::default();
        assert_eq!(physics.gravity, 400.0);
        assert_eq!(physics.friction, 0.9);
        assert_eq!(physics.bounce, 0.3);
    }

    #[test]
    fn spawn_params_calculates_velocity() {
        let params = SpawnParams {
            position: Vec2::new(100.0, 200.0),
            angle: std::f32::consts::FRAC_PI_2, // 90 degrees (straight up)
            speed: 100.0,
        };

        // At 90 degrees: cos = 0, sin = 1
        assert!(params.velocity().x.abs() < 0.1);
        assert!((params.velocity().y - 100.0).abs() < 0.1);
    }

    #[test]
    fn physics_system_applies_gravity() {
        // This tests the pure calculation logic
        let velocity = Vec2::new(50.0, 100.0);
        let gravity = 400.0;
        let delta = 0.016; // ~60fps

        let new_velocity = apply_gravity(velocity, gravity, delta);

        assert_eq!(new_velocity.x, 50.0);
        assert!((new_velocity.y - (100.0 - gravity * delta)).abs() < 0.1);
    }

    #[test]
    fn physics_system_applies_friction_when_grounded() {
        let velocity = Vec2::new(100.0, 0.0);
        let friction = 0.9;

        let new_velocity = apply_friction(velocity, friction);

        assert!((new_velocity.x - 90.0).abs() < 0.1);
        assert_eq!(new_velocity.y, 0.0);
    }
}
