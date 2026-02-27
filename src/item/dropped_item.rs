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
}
