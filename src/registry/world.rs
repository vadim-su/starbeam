use bevy::prelude::*;
use serde::Deserialize;

/// World parameters loaded from RON.
#[derive(Resource, Debug, Clone, Deserialize)]
pub struct WorldConfig {
    pub width_tiles: i32,
    pub height_tiles: i32,
    pub chunk_size: u32,
    pub tile_size: f32,
    pub chunk_load_radius: i32,
    pub seed: u32,
    pub planet_type: String,
}

impl WorldConfig {
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

    fn test_config() -> WorldConfig {
        WorldConfig {
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
