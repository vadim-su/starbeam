//! Runtime resource holding the current star system and generation context.
//!
//! [`CurrentSystem`] stores the [`GeneratedSystem`] produced during loading so
//! that the star-map UI can display all bodies and the warp system can switch
//! the active planet.

use bevy::prelude::*;

use super::generation::GeneratedSystem;

/// The currently loaded star system with metadata needed for planet switching.
#[derive(Resource, Debug, Clone)]
pub struct CurrentSystem {
    /// The generated system (star + all bodies).
    pub system: GeneratedSystem,
    /// Universe seed used for seed derivation.
    pub universe_seed: u64,
    /// Chunk size from generation config (needed when building ActiveWorld).
    pub chunk_size: u32,
    /// Tile size from generation config.
    pub tile_size: f32,
    /// Chunk load radius from generation config.
    pub chunk_load_radius: i32,
}
