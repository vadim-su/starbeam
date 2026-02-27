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
}
