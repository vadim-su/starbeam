use bevy::prelude::*;

use super::components::Inventory;
use crate::item::ItemRegistry;
use crate::item::{DroppedItem, PickupConfig};
use crate::player::Player;

/// Calculate magnet strength based on distance (pure function for testing).
pub fn calculate_magnet_strength(distance: f32, config: &PickupConfig) -> f32 {
    if distance >= config.magnet_radius {
        return 0.0;
    }

    // Strength increases as distance decreases
    config.magnet_strength * (1.0 - distance / config.magnet_radius)
}

/// Check if item should be picked up (pure function for testing).
pub fn should_pickup(distance: f32, config: &PickupConfig) -> bool {
    distance < config.pickup_radius
}

/// Message fired when an item is picked up.
#[derive(Message, Debug)]
pub struct ItemPickupEvent {
    pub item_id: String,
    pub count: u16,
}

/// System that detects and triggers item pickup.
#[allow(clippy::too_many_arguments)]
pub fn item_pickup_system(
    config: Res<PickupConfig>,
    mut player_query: Query<(Entity, &Transform, &mut Inventory), With<Player>>,
    item_registry: Res<ItemRegistry>,
    mut item_query: Query<(Entity, &Transform, &DroppedItem)>,
    mut commands: Commands,
    mut pickup_events: MessageWriter<ItemPickupEvent>,
) {
    let Ok((_player_entity, player_tf, mut inventory)) = player_query.single_mut() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();

    for (item_entity, item_tf, item) in &mut item_query {
        let item_pos = item_tf.translation.truncate();
        let distance = player_pos.distance(item_pos);

        if should_pickup(distance, &config) {
            // Try to add to inventory
            let max_stack = item_registry.max_stack(item_registry.by_name(&item.item_id));
            let remaining = inventory.try_add_item(&item.item_id, item.count, max_stack);

            if remaining == 0 {
                // Successfully picked up
                commands.entity(item_entity).despawn();
                pickup_events.write(ItemPickupEvent {
                    item_id: item.item_id.clone(),
                    count: item.count,
                });
            }
        }
    }
}

/// System that pulls dropped items toward the player.
pub fn item_magnetism_system(
    config: Res<PickupConfig>,
    time: Res<Time>,
    player_query: Query<&Transform, With<Player>>,
    mut item_query: Query<(&Transform, &mut DroppedItem)>,
) {
    let Ok(player_tf) = player_query.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();
    let delta = time.delta_secs();

    for (item_tf, mut item) in &mut item_query {
        let item_pos = item_tf.translation.truncate();
        let distance = player_pos.distance(item_pos);

        // Activate magnet when in range
        if distance < config.magnet_radius {
            item.magnetized = true;
        }

        // Apply magnetism
        if item.magnetized && distance > 0.0 {
            let direction = (player_pos - item_pos).normalize();
            let strength = calculate_magnet_strength(distance, &config);

            item.velocity.x += direction.x * strength * delta;
            item.velocity.y += direction.y * strength * delta;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculate_magnet_strength_increases_near_player() {
        let config = PickupConfig::default();

        // Just inside magnet radius (47.0 < 48.0)
        let strength = calculate_magnet_strength(47.0, &config);
        assert!(strength > 0.0);

        // Very close to player
        let strength_close = calculate_magnet_strength(10.0, &config);
        assert!(strength_close > strength);
    }

    #[test]
    fn calculate_magnet_strength_zero_outside_radius() {
        let config = PickupConfig::default();

        let strength = calculate_magnet_strength(100.0, &config);
        assert_eq!(strength, 0.0);
    }

    #[test]
    fn should_pickup_within_radius() {
        let config = PickupConfig::default();

        assert!(should_pickup(10.0, &config));
        assert!(should_pickup(15.9, &config)); // Just under 16.0
        assert!(!should_pickup(20.0, &config));
    }
}
