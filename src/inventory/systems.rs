use bevy::prelude::*;

use super::components::{BagTarget, Inventory};
use super::hotbar::Hotbar;
use crate::item::ItemRegistry;
use crate::item::{DroppedItem, ItemType, PickupConfig};
use crate::physics::Velocity;
use crate::player::Player;
use crate::world::chunk::WorldMap;
use crate::world::ctx::WorldCtx;

/// Check line-of-sight between two world positions by stepping through tiles.
/// Returns true if no solid tile blocks the path.
pub fn has_line_of_sight(
    from: Vec2,
    to: Vec2,
    tile_size: f32,
    world_map: &WorldMap,
    ctx: &crate::world::ctx::WorldCtxRef,
) -> bool {
    let diff = to - from;
    let dist = diff.length();
    if dist < tile_size {
        return true; // Same tile or adjacent — always visible
    }

    let steps = (dist / (tile_size * 0.5)).ceil() as u32;
    for i in 1..steps {
        let t = i as f32 / steps as f32;
        let sample = from + diff * t;
        let tx = (sample.x / tile_size).floor() as i32;
        let ty = (sample.y / tile_size).floor() as i32;
        if world_map.is_solid(tx, ty, ctx) {
            return false;
        }
    }
    true
}

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
    mut item_query: Query<(Entity, &Transform, &mut DroppedItem)>,
    mut commands: Commands,
    mut pickup_events: MessageWriter<ItemPickupEvent>,
) {
    let Ok((_player_entity, player_tf, mut inventory)) = player_query.single_mut() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();

    for (item_entity, item_tf, mut item) in &mut item_query {
        let item_pos = item_tf.translation.truncate();
        let distance = player_pos.distance(item_pos);

        if should_pickup(distance, &config) {
            // Look up max_stack; skip unknown items instead of panicking
            let Some(item_def_id) = item_registry.by_name(&item.item_id) else {
                continue;
            };
            let item_def = item_registry.get(item_def_id);
            let max_stack = item_def.max_stack;
            let target = match item_def.item_type {
                ItemType::Block | ItemType::Material => BagTarget::Material,
                _ => BagTarget::Main,
            };
            let remaining = inventory.try_add_item(&item.item_id, item.count, max_stack, target);

            if remaining == 0 {
                // Fully picked up
                let picked_count = item.count;
                commands.entity(item_entity).despawn();
                pickup_events.write(ItemPickupEvent {
                    item_id: item.item_id.clone(),
                    count: picked_count,
                });
            } else if remaining < item.count {
                // Partially picked up — update dropped item count to prevent duplication
                let picked_count = item.count - remaining;
                item.count = remaining;
                pickup_events.write(ItemPickupEvent {
                    item_id: item.item_id.clone(),
                    count: picked_count,
                });
            }
            // remaining == item.count means nothing was picked up (inventory full)
        }
    }
}

/// System that pulls dropped items toward the player.
/// Items can only be magnetized if there is line-of-sight (no solid tiles blocking).
pub fn item_magnetism_system(
    config: Res<PickupConfig>,
    time: Res<Time>,
    ctx: WorldCtx,
    world_map: Res<WorldMap>,
    player_query: Query<&Transform, With<Player>>,
    mut item_query: Query<(&Transform, &mut DroppedItem, &mut Velocity)>,
) {
    let Ok(player_tf) = player_query.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();
    let delta = time.delta_secs();
    let ctx_ref = ctx.as_ref();

    for (item_tf, mut item, mut vel) in &mut item_query {
        let item_pos = item_tf.translation.truncate();
        let distance = player_pos.distance(item_pos);

        // Activate magnet when in range AND line-of-sight is clear
        if distance < config.magnet_radius
            && has_line_of_sight(
                item_pos,
                player_pos,
                ctx_ref.config.tile_size,
                &world_map,
                &ctx_ref,
            )
        {
            item.magnetized = true;
        }

        // Deactivate magnet if line-of-sight is blocked
        if item.magnetized
            && !has_line_of_sight(
                item_pos,
                player_pos,
                ctx_ref.config.tile_size,
                &world_map,
                &ctx_ref,
            )
        {
            item.magnetized = false;
        }

        // Apply magnetism
        if item.magnetized && distance > 0.0 {
            let direction = (player_pos - item_pos).normalize();
            let strength = calculate_magnet_strength(distance, &config);

            vel.x += direction.x * strength * delta;
            vel.y += direction.y * strength * delta;
        }
    }
}

/// Number keys mapped to hotbar slots.
const HOTBAR_KEYS: [KeyCode; 6] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
];

/// System that handles hotbar slot selection via number keys 1-6.
pub fn hotbar_input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut hotbar_query: Query<&mut Hotbar>,
) {
    let Ok(mut hotbar) = hotbar_query.single_mut() else {
        return;
    };

    for (i, key) in HOTBAR_KEYS.iter().enumerate() {
        if keyboard.just_pressed(*key) {
            hotbar.select_slot(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::fixtures;
    use crate::world::chunk::WorldMap;
    use crate::world::terrain_gen;

    #[test]
    fn line_of_sight_clear_in_empty_world() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let world_map = WorldMap::default();

        let from = Vec2::new(100.0, 30000.0);
        let to = Vec2::new(300.0, 30000.0);
        assert!(has_line_of_sight(from, to, wc.tile_size, &world_map, &ctx));
    }

    #[test]
    fn line_of_sight_blocked_by_solid_tile() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);

        let surface_y = terrain_gen::surface_height(
            &nc,
            0,
            &wc,
            pc.layers.surface.terrain_frequency,
            pc.layers.surface.terrain_amplitude,
        );
        let chunk_size = wc.chunk_size as i32;
        let surface_chunk_y = surface_y.div_euclid(chunk_size);

        let mut world_map = WorldMap::default();
        for cy in (surface_chunk_y - 2)..=(surface_chunk_y + 1) {
            for cx in -1..=1 {
                world_map.get_or_generate_chunk(cx, cy, &ctx);
            }
        }

        // From above ground to underground — should be blocked
        let ts = wc.tile_size;
        let above = Vec2::new(ts / 2.0, (surface_y + 3) as f32 * ts);
        let below = Vec2::new(ts / 2.0, (surface_y - 5) as f32 * ts);
        assert!(
            !has_line_of_sight(above, below, ts, &world_map, &ctx),
            "line of sight should be blocked through terrain"
        );
    }

    #[test]
    fn line_of_sight_same_tile_always_clear() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let world_map = WorldMap::default();

        let pos = Vec2::new(100.0, 100.0);
        let nearby = Vec2::new(105.0, 105.0);
        assert!(has_line_of_sight(
            pos,
            nearby,
            wc.tile_size,
            &world_map,
            &ctx
        ));
    }

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
