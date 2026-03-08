use bevy::prelude::*;
use bevy::sprite_render::MeshMaterial2d;

use crate::cosmos::address::CelestialAddress;
use crate::cosmos::capsule::{AirlockMarker, CapsuleLocation, CapsuleMarker};
use crate::cosmos::ship_location::ShipLocation;
use crate::cosmos::warp::{WarpToBody, WarpToShip};
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

/// Compute edge-to-edge distance between the player AABB and an object AABB,
/// accounting for world wrapping when `wrap_x` is true.
fn edge_distance(
    player_tf: &Transform,
    player_half_w: f32,
    player_half_h: f32,
    obj_tf: &Transform,
    world_width: f32,
    wrap_x: bool,
) -> f32 {
    let obj_half_w = obj_tf.scale.x / 2.0;
    let obj_half_h = obj_tf.scale.y / 2.0;

    let mut dx = (player_tf.translation.x - obj_tf.translation.x).abs();
    if wrap_x && world_width > 0.0 {
        dx %= world_width;
        if dx > world_width / 2.0 {
            dx = world_width - dx;
        }
    }
    let dy = (player_tf.translation.y - obj_tf.translation.y).abs();

    let edge_dx = (dx - player_half_w - obj_half_w).max(0.0);
    let edge_dy = (dy - player_half_h - obj_half_h).max(0.0);
    (edge_dx * edge_dx + edge_dy * edge_dy).sqrt()
}

/// Each frame, find the nearest interactable entity within range of the player.
///
/// Considers CraftingStation, CapsuleMarker, and AirlockMarker entities.
/// Distance is measured between the edges of the player's collision box and the
/// object's AABB -- not from the player's center point.
pub fn detect_nearby_interactable(
    mut nearby: ResMut<NearbyInteractable>,
    player_query: Query<(&Transform, &TileCollider), With<Player>>,
    station_query: Query<(Entity, &Transform), With<CraftingStation>>,
    capsule_query: Query<(Entity, &Transform), With<CapsuleMarker>>,
    airlock_query: Query<(Entity, &Transform), With<AirlockMarker>>,
    world_config: Res<ActiveWorld>,
) {
    let Ok((player_tf, player_col)) = player_query.single() else {
        nearby.entity = None;
        return;
    };

    let tile_size = world_config.tile_size;
    let world_width = world_config.width_tiles as f32 * tile_size;
    let range_px = INTERACTION_RANGE * tile_size;
    let wrap_x = world_config.wrap_x;

    let player_half_w = player_col.width / 2.0;
    let player_half_h = player_col.height / 2.0;

    let mut closest: Option<(Entity, f32)> = None;

    let mut check = |entity: Entity, obj_tf: &Transform| {
        let dist = edge_distance(player_tf, player_half_w, player_half_h, obj_tf, world_width, wrap_x);
        if dist <= range_px && (closest.is_none() || dist < closest.unwrap().1) {
            closest = Some((entity, dist));
        }
    };

    for (entity, tf) in &station_query {
        check(entity, tf);
    }
    for (entity, tf) in &capsule_query {
        check(entity, tf);
    }
    for (entity, tf) in &airlock_query {
        check(entity, tf);
    }

    nearby.entity = closest.map(|(e, _)| e);
}

/// Handle E key: toggle station UI, or trigger capsule/airlock warp.
/// Handle C key: toggle hand-craft UI.
/// ESC-close is handled by the unified window system in `window.rs`.
#[allow(clippy::too_many_arguments)]
pub fn handle_interaction_input(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    nearby: Res<NearbyInteractable>,
    mut open_station: ResMut<OpenStation>,
    mut hand_craft_open: ResMut<HandCraftOpen>,
    chat_state: Res<crate::chat::ChatState>,
    // Queries to determine what type the nearby entity is
    station_query: Query<Entity, With<CraftingStation>>,
    capsule_query: Query<&Transform, With<CapsuleMarker>>,
    airlock_query: Query<Entity, With<AirlockMarker>>,
    active_world: Res<ActiveWorld>,
    ship_location: Option<Res<ShipLocation>>,
    capsule_location: Option<Res<CapsuleLocation>>,
    mut warp_body_events: bevy::ecs::message::MessageWriter<WarpToBody>,
    mut warp_ship_events: bevy::ecs::message::MessageWriter<WarpToShip>,
) {
    if chat_state.is_active {
        return;
    }
    // E key: toggle station interaction or trigger warp
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

        if let Some(entity) = nearby.entity {
            // Check if it's a crafting station
            if station_query.get(entity).is_ok() {
                open_station.0 = Some(entity);
                return;
            }

            // Check if it's a capsule (planet → ship warp)
            if let Ok(capsule_tf) = capsule_query.get(entity) {
                // Store capsule location for return trip
                let tile_size = active_world.tile_size;
                let tile_x = (capsule_tf.translation.x / tile_size).floor() as i32;
                let tile_y = (capsule_tf.translation.y / tile_size).floor() as i32;

                let orbit = active_world.address.orbit().unwrap_or(0);
                commands.insert_resource(CapsuleLocation {
                    planet_address: active_world.address.clone(),
                    planet_orbit: orbit,
                    tile_x,
                    tile_y,
                });

                // Fire WarpToShip
                warp_ship_events.write(WarpToShip);
                info!(
                    "Capsule activated at tile ({}, {}) — warping to ship",
                    tile_x, tile_y
                );
                return;
            }

            // Check if it's an airlock (ship → planet warp)
            if airlock_query.get(entity).is_ok() {
                // Must be on a ship
                if !matches!(active_world.address, CelestialAddress::Ship { .. }) {
                    warn!("Airlock interaction on non-ship world — ignoring");
                    return;
                }

                // Ship must be in orbit (not in transit)
                match ship_location.as_deref() {
                    Some(ShipLocation::Orbit(planet_addr)) => {
                        // If we have a capsule location on that planet, warp to it
                        if let Some(ref capsule_loc) = capsule_location {
                            if capsule_loc.planet_address == *planet_addr {
                                warp_body_events.write(WarpToBody {
                                    orbit: capsule_loc.planet_orbit,
                                });
                                info!(
                                    "Airlock activated — warping to planet orbit {}",
                                    capsule_loc.planet_orbit
                                );
                                return;
                            }
                        }
                        // No capsule on the orbited planet — warp to the planet
                        // orbit anyway (will spawn at default location)
                        if let Some(orbit) = planet_addr.orbit() {
                            warp_body_events.write(WarpToBody { orbit });
                            info!("Airlock activated — warping to orbit {}", orbit);
                        } else {
                            warn!("Airlock: orbiting body has no orbit index");
                        }
                    }
                    Some(ShipLocation::InTransit { .. }) => {
                        info!("Cannot disembark — ship is in transit");
                    }
                    None => {
                        warn!("Airlock: no ShipLocation resource found");
                    }
                }
                return;
            }
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
    interactable_query: Query<&MeshMaterial2d<LitSpriteMaterial>, Or<(With<CraftingStation>, With<CapsuleMarker>, With<AirlockMarker>)>>,
    mut materials: ResMut<Assets<LitSpriteMaterial>>,
) {
    if !nearby.is_changed() {
        return;
    }

    // Clear highlight on ALL interactable materials, then set on the active one.
    for mat_handle in &interactable_query {
        if let Some(mat) = materials.get_mut(&mat_handle.0) {
            mat.highlight = Vec4::ZERO;
        }
    }

    if let Some(entity) = nearby.entity {
        if let Ok(mat_handle) = interactable_query.get(entity) {
            if let Some(mat) = materials.get_mut(&mat_handle.0) {
                mat.highlight = HIGHLIGHT_COLOR;
            }
        }
    }
}
