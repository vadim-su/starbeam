//! Spawn a trader entity on the planet surface.

use bevy::prelude::*;

use crate::registry::biome::PlanetConfig;
use crate::registry::world::ActiveWorld;
use crate::world::terrain_gen::TerrainNoiseCache;

use super::{TradeOffer, TradeOffers, Trader};

/// Marker to avoid spawning multiple traders.
#[derive(Component)]
pub struct TraderSpawned;

/// Spawn a trader at the approximate center of the world, on the surface.
pub fn spawn_trader(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    active_world: Res<ActiveWorld>,
    noise_cache: Res<TerrainNoiseCache>,
    planet_config: Res<PlanetConfig>,
    existing: Query<Entity, With<Trader>>,
) {
    // Don't spawn if a trader already exists (e.g. re-entering InGame after warp).
    if !existing.is_empty() {
        return;
    }

    // Ship worlds have zero amplitude — skip trader spawn on ships.
    if planet_config.layers.surface.terrain_amplitude == 0.0 {
        return;
    }

    let tile_x = active_world.width_tiles / 2;
    let surface_y = crate::world::terrain_gen::surface_height(
        &noise_cache,
        tile_x,
        &active_world,
        planet_config.layers.surface.terrain_frequency,
        planet_config.layers.surface.terrain_amplitude,
    );

    // Place 2 tiles above the surface
    let tile_y = surface_y + 2;
    let tile_size = active_world.tile_size;

    let world_x = tile_x as f32 * tile_size + tile_size / 2.0;
    let world_y = tile_y as f32 * tile_size + tile_size / 2.0;

    // Hardcoded trade offers for now
    let trade_offers = TradeOffers {
        offers: vec![
            TradeOffer {
                cost: vec![("coal_ore".to_string(), 10)],
                result: ("iron_bar".to_string(), 2),
            },
            TradeOffer {
                cost: vec![("iron_bar".to_string(), 5)],
                result: ("gold_bar".to_string(), 1),
            },
            TradeOffer {
                cost: vec![("dirt".to_string(), 50)],
                result: ("coal_ore".to_string(), 5),
            },
        ],
    };

    commands.spawn((
        Trader,
        trade_offers,
        Transform::from_translation(Vec3::new(world_x, world_y, 0.5)),
        Sprite::from_image(asset_server.load("sprites/npcs/merchant/rotations/east.png")),
        Visibility::default(),
    ));

    info!(
        "Trader spawned at tile ({}, {}), world ({:.0}, {:.0})",
        tile_x, tile_y, world_x, world_y
    );
}
