use noise::{NoiseFn, Perlin};

use crate::world::tile::TileType;
use crate::world::{CHUNK_SIZE, WORLD_HEIGHT_TILES, WORLD_WIDTH_TILES};

const SURFACE_BASE: f64 = 0.7; // 70% from bottom
const SURFACE_AMPLITUDE: f64 = 40.0; // tiles of variation
const SURFACE_FREQUENCY: f64 = 0.02;
const CAVE_FREQUENCY: f64 = 0.07;
const CAVE_THRESHOLD: f64 = 0.3;
const DIRT_DEPTH: i32 = 4;

/// Get the surface height (in tile Y) at a given tile X coordinate.
pub fn surface_height(seed: u32, tile_x: i32) -> i32 {
    let perlin = Perlin::new(seed);
    let base = SURFACE_BASE * WORLD_HEIGHT_TILES as f64;
    let noise_val = perlin.get([tile_x as f64 * SURFACE_FREQUENCY, 0.0]);
    (base + noise_val * SURFACE_AMPLITUDE) as i32
}

/// Generate tile type at an absolute tile position.
pub fn generate_tile(seed: u32, tile_x: i32, tile_y: i32) -> TileType {
    if tile_x < 0 || tile_x >= WORLD_WIDTH_TILES || tile_y < 0 || tile_y >= WORLD_HEIGHT_TILES {
        return TileType::Air;
    }

    let surface_y = surface_height(seed, tile_x);

    if tile_y > surface_y {
        return TileType::Air;
    }
    if tile_y == surface_y {
        return TileType::Grass;
    }
    if tile_y > surface_y - DIRT_DEPTH {
        return TileType::Dirt;
    }

    // Below dirt layer: stone with caves
    let cave_perlin = Perlin::new(seed.wrapping_add(1));
    let cave_val = cave_perlin.get([
        tile_x as f64 * CAVE_FREQUENCY,
        tile_y as f64 * CAVE_FREQUENCY,
    ]);
    if cave_val.abs() < CAVE_THRESHOLD {
        TileType::Air // cave
    } else {
        TileType::Stone
    }
}

/// Generate all tiles for a chunk. Returns Vec of CHUNK_SIZE*CHUNK_SIZE tiles in row-major order.
/// Index = local_y * CHUNK_SIZE + local_x
pub fn generate_chunk_tiles(seed: u32, chunk_x: i32, chunk_y: i32) -> Vec<TileType> {
    let base_x = chunk_x * CHUNK_SIZE as i32;
    let base_y = chunk_y * CHUNK_SIZE as i32;
    let mut tiles = Vec::with_capacity((CHUNK_SIZE * CHUNK_SIZE) as usize);

    for local_y in 0..CHUNK_SIZE as i32 {
        for local_x in 0..CHUNK_SIZE as i32 {
            tiles.push(generate_tile(seed, base_x + local_x, base_y + local_y));
        }
    }

    tiles
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SEED: u32 = 42;

    #[test]
    fn surface_height_is_deterministic() {
        let h1 = surface_height(TEST_SEED, 100);
        let h2 = surface_height(TEST_SEED, 100);
        assert_eq!(h1, h2);
    }

    #[test]
    fn surface_height_is_within_bounds() {
        for x in 0..WORLD_WIDTH_TILES {
            let h = surface_height(TEST_SEED, x);
            assert!(h >= 0 && h < WORLD_HEIGHT_TILES, "surface at x={x} is {h}");
        }
    }

    #[test]
    fn above_surface_is_air() {
        let h = surface_height(TEST_SEED, 500);
        assert_eq!(generate_tile(TEST_SEED, 500, h + 1), TileType::Air);
        assert_eq!(generate_tile(TEST_SEED, 500, h + 10), TileType::Air);
    }

    #[test]
    fn surface_is_grass() {
        let h = surface_height(TEST_SEED, 500);
        assert_eq!(generate_tile(TEST_SEED, 500, h), TileType::Grass);
    }

    #[test]
    fn below_surface_is_dirt_then_stone() {
        let h = surface_height(TEST_SEED, 500);
        assert_eq!(generate_tile(TEST_SEED, 500, h - 1), TileType::Dirt);
        let deep_tile = generate_tile(TEST_SEED, 500, 10);
        assert!(matches!(deep_tile, TileType::Stone | TileType::Air));
    }

    #[test]
    fn chunk_generation_has_correct_size() {
        let tiles = generate_chunk_tiles(TEST_SEED, 0, 0);
        assert_eq!(tiles.len(), (CHUNK_SIZE * CHUNK_SIZE) as usize);
    }

    #[test]
    fn chunk_generation_is_deterministic() {
        let tiles1 = generate_chunk_tiles(TEST_SEED, 5, 10);
        let tiles2 = generate_chunk_tiles(TEST_SEED, 5, 10);
        assert_eq!(tiles1, tiles2);
    }

    #[test]
    fn out_of_bounds_is_air() {
        assert_eq!(generate_tile(TEST_SEED, -1, 500), TileType::Air);
        assert_eq!(
            generate_tile(TEST_SEED, WORLD_WIDTH_TILES, 500),
            TileType::Air
        );
    }
}
