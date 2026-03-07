use bevy::prelude::*;
use bevy::sprite_render::MeshMaterial2d;

use crate::crafting::CraftingStation;
use crate::physics::TileCollider;
use crate::player::Player;
use crate::registry::world::ActiveWorld;
use crate::world::lit_sprite::LitSpriteMaterial;

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

const INTERACTION_RANGE: f32 = 1.5; // tiles from the nearest object edge

/// Each frame, find the nearest CraftingStation within range of the player.
///
/// Distance is measured between the edges of the player's collision box and the
/// station's AABB — not from the player's center point. This prevents the
/// highlight from activating when the player *looks* far away (the large sprite
/// is offset from the small collision box).
pub fn detect_nearby_interactable(
    mut nearby: ResMut<NearbyInteractable>,
    player_query: Query<(&Transform, &TileCollider), With<Player>>,
    station_query: Query<(Entity, &Transform), With<CraftingStation>>,
    world_config: Res<ActiveWorld>,
) {
    let Ok((player_tf, player_col)) = player_query.single() else {
        nearby.entity = None;
        return;
    };

    let tile_size = world_config.tile_size;
    let world_width = world_config.width_tiles as f32 * tile_size;
    let range_px = INTERACTION_RANGE * tile_size;

    // Player collision half-extents
    let player_half_w = player_col.width / 2.0;
    let player_half_h = player_col.height / 2.0;

    let mut closest: Option<(Entity, f32)> = None;

    for (entity, station_tf) in &station_query {
        // Station AABB half-extents.
        // Transform.scale stores (width_px, height_px, 1.0) for objects.
        let station_half_w = station_tf.scale.x / 2.0;
        let station_half_h = station_tf.scale.y / 2.0;

        // Wrap-aware horizontal distance: normalize to [0, world_width) first,
        // then pick the shorter path around the cylinder.
        let mut dx = (player_tf.translation.x - station_tf.translation.x).abs() % world_width;
        if dx > world_width / 2.0 {
            dx = world_width - dx;
        }
        let dy = (player_tf.translation.y - station_tf.translation.y).abs();

        // Edge-to-edge distance between both AABBs (negative = overlapping)
        let edge_dx = (dx - player_half_w - station_half_w).max(0.0);
        let edge_dy = (dy - player_half_h - station_half_h).max(0.0);
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
/// ESC-close is handled by the unified window system in `window.rs`.
pub fn handle_interaction_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    nearby: Res<NearbyInteractable>,
    mut open_station: ResMut<OpenStation>,
    mut hand_craft_open: ResMut<HandCraftOpen>,
) {
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

/// Outline color for the nearest interactable object (soft white contour).
const HIGHLIGHT_COLOR: Vec4 = Vec4::new(1.0, 1.0, 1.0, 0.25);

/// Set highlight on the nearest interactable entity, clear on others.
pub fn update_interactable_highlight(
    nearby: Res<NearbyInteractable>,
    station_query: Query<&MeshMaterial2d<LitSpriteMaterial>, With<CraftingStation>>,
    mut materials: ResMut<Assets<LitSpriteMaterial>>,
) {
    if !nearby.is_changed() {
        return;
    }

    // Clear highlight on ALL station materials, then set on the active one.
    for mat_handle in &station_query {
        if let Some(mat) = materials.get_mut(&mat_handle.0) {
            mat.highlight = Vec4::ZERO;
        }
    }

    if let Some(entity) = nearby.entity {
        if let Ok(mat_handle) = station_query.get(entity) {
            if let Some(mat) = materials.get_mut(&mat_handle.0) {
                mat.highlight = HIGHLIGHT_COLOR;
            }
        }
    }
}
