//! Asset types for star templates and global generation configuration.
//!
//! [`StarTypeAsset`] defines a star type template (e.g. yellow dwarf) with orbit
//! ranges and temperature zones. [`GenerationConfigAsset`] holds global generation
//! rules shared across all worlds.

use bevy::prelude::*;
use bevy::reflect::TypePath;
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Generation config
// ---------------------------------------------------------------------------

/// Global generation rules loaded from generation.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct GenerationConfigAsset {
    pub default_planet_size: PlanetSizeConfig,
    pub chunk_size: u32,
    pub tile_size: f32,
    pub chunk_load_radius: i32,
    #[serde(default = "default_orbit_temp_falloff")]
    pub orbit_temperature_falloff: f32,
}

/// Default planet dimensions in tiles.
#[derive(Debug, Clone, Deserialize)]
pub struct PlanetSizeConfig {
    pub width: i32,
    pub height: i32,
}

fn default_orbit_temp_falloff() -> f32 {
    0.15
}

// ---------------------------------------------------------------------------
// Star type
// ---------------------------------------------------------------------------

/// Temperature zone within a star type.
#[derive(Debug, Clone, Deserialize)]
pub struct TemperatureZone {
    pub orbits: (u32, u32),
    pub temperature: String,
    pub types: Vec<String>,
}

/// Star type template loaded from *.star.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct StarTypeAsset {
    pub id: String,
    pub orbit_count: (u32, u32),
    pub luminosity: (f32, f32),
    pub sun_color: [f32; 3],
    pub zones: Vec<TemperatureZone>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_generation_config() {
        let ron_str = include_str!("../../assets/worlds/generation.ron");
        let config: GenerationConfigAsset =
            ron::from_str(ron_str).expect("Failed to parse generation.ron");
        assert_eq!(config.chunk_size, 32);
        assert!(config.default_planet_size.width > 0);
    }

    #[test]
    fn parse_star_type() {
        let ron_str =
            include_str!("../../assets/worlds/star_types/yellow_dwarf/yellow_dwarf.star.ron");
        let star: StarTypeAsset =
            ron::from_str(ron_str).expect("Failed to parse yellow_dwarf.star.ron");
        assert_eq!(star.id, "yellow_dwarf");
        assert!(!star.zones.is_empty());
        assert!(star.orbit_count.0 <= star.orbit_count.1);
    }
}
