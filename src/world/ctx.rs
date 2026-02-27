use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::registry::biome::{BiomeRegistry, PlanetConfig};
use crate::registry::tile::TileRegistry;
use crate::registry::world::WorldConfig;
use crate::world::biome_map::BiomeMap;

/// Bevy SystemParam bundling the 5 read-only world resources that most
/// world-related systems need. Use `as_ref()` to obtain a lightweight
/// [`WorldCtxRef`] for passing into regular functions and methods.
#[derive(SystemParam)]
pub struct WorldCtx<'w> {
    pub config: Res<'w, WorldConfig>,
    pub biome_map: Res<'w, BiomeMap>,
    pub biome_registry: Res<'w, BiomeRegistry>,
    pub tile_registry: Res<'w, TileRegistry>,
    pub planet_config: Res<'w, PlanetConfig>,
}

impl WorldCtx<'_> {
    /// Create a lightweight reference bundle for passing into functions/methods.
    pub fn as_ref(&self) -> WorldCtxRef<'_> {
        WorldCtxRef {
            config: &self.config,
            biome_map: &self.biome_map,
            biome_registry: &self.biome_registry,
            tile_registry: &self.tile_registry,
            planet_config: &self.planet_config,
        }
    }
}

/// Lightweight reference bundle for passing world resources into regular
/// functions and methods without requiring ECS system parameters.
pub struct WorldCtxRef<'a> {
    pub config: &'a WorldConfig,
    pub biome_map: &'a BiomeMap,
    pub biome_registry: &'a BiomeRegistry,
    pub tile_registry: &'a TileRegistry,
    pub planet_config: &'a PlanetConfig,
}
