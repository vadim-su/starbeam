use bevy::prelude::*;

use crate::cosmos::address::{CelestialAddress, CelestialSeeds};

/// Active world parameters — the currently loaded celestial body.
/// Renamed from WorldConfig to reflect that this represents the active world,
/// not just configuration.
#[derive(Resource, Debug, Clone)]
pub struct ActiveWorld {
    pub address: CelestialAddress,
    pub seeds: CelestialSeeds,
    pub width_tiles: i32,
    pub height_tiles: i32,
    pub chunk_size: u32,
    pub tile_size: f32,
    pub chunk_load_radius: i32,
    pub seed: u32, // TEMPORARY — kept for BiomeMap/TerrainNoiseCache compat
    pub planet_type: String,
}

impl ActiveWorld {
    pub fn width_chunks(&self) -> i32 {
        self.width_tiles / self.chunk_size as i32
    }

    pub fn height_chunks(&self) -> i32 {
        self.height_tiles / self.chunk_size as i32
    }

    pub fn wrap_tile_x(&self, tile_x: i32) -> i32 {
        tile_x.rem_euclid(self.width_tiles)
    }

    pub fn wrap_chunk_x(&self, chunk_x: i32) -> i32 {
        chunk_x.rem_euclid(self.width_chunks())
    }

    pub fn world_pixel_width(&self) -> f32 {
        self.width_tiles as f32 * self.tile_size
    }

    pub fn world_pixel_height(&self) -> f32 {
        self.height_tiles as f32 * self.tile_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::IVec2;

    fn test_config() -> ActiveWorld {
        let address = CelestialAddress {
            galaxy: IVec2::ZERO,
            system: IVec2::ZERO,
            orbit: 2,
            satellite: None,
        };
        let seeds = CelestialSeeds::derive(42, &address);
        ActiveWorld {
            address,
            seeds,
            width_tiles: 2048,
            height_tiles: 1024,
            chunk_size: 32,
            tile_size: 32.0,
            chunk_load_radius: 3,
            seed: 42,
            planet_type: "garden".into(),
        }
    }

    #[test]
    fn computed_chunk_dimensions() {
        let c = test_config();
        assert_eq!(c.width_chunks(), 64);
        assert_eq!(c.height_chunks(), 32);
    }

    #[test]
    fn wrap_tile_x_identity() {
        let c = test_config();
        assert_eq!(c.wrap_tile_x(0), 0);
        assert_eq!(c.wrap_tile_x(100), 100);
    }

    #[test]
    fn wrap_tile_x_overflow() {
        let c = test_config();
        assert_eq!(c.wrap_tile_x(2048), 0);
        assert_eq!(c.wrap_tile_x(2049), 1);
    }

    #[test]
    fn wrap_tile_x_negative() {
        let c = test_config();
        assert_eq!(c.wrap_tile_x(-1), 2047);
    }

    #[test]
    fn wrap_chunk_x_overflow() {
        let c = test_config();
        assert_eq!(c.wrap_chunk_x(64), 0);
        assert_eq!(c.wrap_chunk_x(-1), 63);
    }

    #[test]
    fn world_pixel_width() {
        let c = test_config();
        assert_eq!(c.world_pixel_width(), 2048.0 * 32.0);
    }
}
