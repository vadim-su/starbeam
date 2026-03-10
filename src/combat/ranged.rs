use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::inventory::{Hotbar, Inventory};
use crate::item::ItemRegistry;
use crate::player::Player;

use super::projectile;

const PROJECTILE_SPEED: f32 = 500.0;
const ARROW_ITEM_ID: &str = "arrow";

/// Returns true if the item_id represents a ranged weapon (bow).
pub fn is_ranged_weapon(item_id: &str) -> bool {
    item_id.contains("bow")
}

pub fn ranged_attack_system(
    mouse: Res<ButtonInput<MouseButton>>,
    item_registry: Option<Res<ItemRegistry>>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    mut commands: Commands,
    mut player_query: Query<(Entity, &GlobalTransform, &Hotbar, &mut Inventory), With<Player>>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Some(ref registry) = item_registry else {
        return;
    };

    // Resolve cursor world position
    let Ok(window) = window_query.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok((camera, camera_gt)) = camera_query.single() else {
        return;
    };
    let Ok(world_pos) = camera.viewport_to_world_2d(camera_gt, cursor_pos) else {
        return;
    };

    for (player_entity, player_gt, hotbar, mut inventory) in &mut player_query {
        // Check right hand first (primary), then left hand
        let item_name = hotbar
            .get_item_for_hand(false)
            .or_else(|| hotbar.get_item_for_hand(true));

        let Some(name) = item_name else {
            continue;
        };

        if !is_ranged_weapon(name) {
            continue;
        }

        let Some(item_id) = registry.by_name(name) else {
            continue;
        };
        let def = registry.get(item_id);

        // Read damage from item stats
        let damage = def
            .stats
            .as_ref()
            .and_then(|s| s.damage)
            .unwrap_or(5.0);
        let knockback = def
            .stats
            .as_ref()
            .and_then(|s| s.knockback)
            .unwrap_or(100.0);

        // Consume an arrow from inventory
        if !inventory.remove_item(ARROW_ITEM_ID, 1) {
            // No arrows available
            continue;
        }

        let player_pos = player_gt.translation().truncate();
        let direction = world_pos - player_pos;

        projectile::spawn_projectile(
            &mut commands,
            player_pos,
            direction,
            PROJECTILE_SPEED,
            damage,
            knockback,
            player_entity,
        );
    }
}
