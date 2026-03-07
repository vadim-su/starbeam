use bevy::prelude::*;

use crate::crafting::CraftingStation;
use crate::player::Player;
use crate::registry::world::ActiveWorld;

/// Resource: the nearest interactable entity within range, if any.
#[derive(Resource, Default)]
pub struct NearbyInteractable {
    pub entity: Option<Entity>,
}

/// Resource: which crafting station UI is currently open.
#[derive(Resource, Default)]
pub struct OpenStation(pub Option<Entity>);

/// Resource: whether hand-craft UI is open.
#[derive(Resource, Default)]
pub struct HandCraftOpen(pub bool);

const INTERACTION_RANGE: f32 = 3.0; // tiles

/// Each frame, find the nearest CraftingStation within range of the player.
pub fn detect_nearby_interactable(
    mut nearby: ResMut<NearbyInteractable>,
    player_query: Query<&Transform, With<Player>>,
    station_query: Query<(Entity, &Transform), With<CraftingStation>>,
    world_config: Res<ActiveWorld>,
) {
    let Ok(player_tf) = player_query.single() else {
        nearby.entity = None;
        return;
    };

    let tile_size = world_config.tile_size;
    let world_width = world_config.width_tiles as f32 * tile_size;
    let range_px = INTERACTION_RANGE * tile_size;

    let mut closest: Option<(Entity, f32)> = None;

    for (entity, station_tf) in &station_query {
        // Distance to nearest edge of the object AABB, not its center.
        // Transform.scale stores (width_px, height_px, 1.0) for objects.
        let half_w = station_tf.scale.x / 2.0;
        let half_h = station_tf.scale.y / 2.0;

        let dx = (player_tf.translation.x - station_tf.translation.x).abs();
        let dx = dx.min(world_width - dx); // wrap-aware
        let dy = (player_tf.translation.y - station_tf.translation.y).abs();

        // Signed distance to AABB edge (negative = inside)
        let edge_dx = (dx - half_w).max(0.0);
        let edge_dy = (dy - half_h).max(0.0);
        let dist = (edge_dx * edge_dx + edge_dy * edge_dy).sqrt();

        if dist <= range_px {
            if closest.is_none() || dist < closest.unwrap().1 {
                closest = Some((entity, dist));
            }
        }
    }

    nearby.entity = closest.map(|(e, _)| e);
}

/// Handle E key: toggle station UI. Handle C key: toggle hand-craft UI.
/// Handle Escape: close any open crafting UI.
pub fn handle_interaction_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    nearby: Res<NearbyInteractable>,
    mut open_station: ResMut<OpenStation>,
    mut hand_craft_open: ResMut<HandCraftOpen>,
) {
    // Escape closes everything
    if keyboard.just_pressed(KeyCode::Escape) {
        if open_station.0.is_some() {
            open_station.0 = None;
            return;
        }
        if hand_craft_open.0 {
            hand_craft_open.0 = false;
            return;
        }
    }

    // E key: toggle station interaction
    if keyboard.just_pressed(KeyCode::KeyE) {
        // If a station is open, close it
        if open_station.0.is_some() {
            open_station.0 = None;
            return;
        }
        // Close hand-craft if open
        if hand_craft_open.0 {
            hand_craft_open.0 = false;
        }
        // If near a crafting station, open it
        if let Some(entity) = nearby.entity {
            open_station.0 = Some(entity);
        }
        return;
    }

    // C key: toggle hand-craft
    if keyboard.just_pressed(KeyCode::KeyC) {
        if hand_craft_open.0 {
            hand_craft_open.0 = false;
        } else {
            // Close station UI if open
            open_station.0 = None;
            hand_craft_open.0 = true;
        }
    }
}
