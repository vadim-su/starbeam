use bevy::prelude::*;

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
}
