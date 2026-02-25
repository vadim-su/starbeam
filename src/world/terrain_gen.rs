use noise::{NoiseFn, Perlin};

use crate::registry::tile::{TerrainTiles, TileId};
use crate::registry::world::WorldConfig;

const SURFACE_BASE: f64 = 0.7;
const SURFACE_AMPLITUDE: f64 = 40.0;
const SURFACE_FREQUENCY: f64 = 0.02;
const CAVE_FREQUENCY: f64 = 0.07;
const CAVE_THRESHOLD: f64 = 0.3;
const DIRT_DEPTH: i32 = 4;

pub fn surface_height(seed: u32, tile_x: i32, wc: &WorldConfig) -> i32 {
    let perlin = Perlin::new(seed);
    let base = SURFACE_BASE * wc.height_tiles as f64;

    let angle = tile_x as f64 / wc.width_tiles as f64 * 2.0 * std::f64::consts::PI;
    let radius = wc.width_tiles as f64 * SURFACE_FREQUENCY / (2.0 * std::f64::consts::PI);
    let nx = radius * angle.cos();
    let ny = radius * angle.sin();
    let noise_val = perlin.get([nx, ny]);

    (base + noise_val * SURFACE_AMPLITUDE) as i32
}

pub fn generate_tile(
    seed: u32,
    tile_x: i32,
    tile_y: i32,
    wc: &WorldConfig,
    tt: &TerrainTiles,
) -> TileId {
    if tile_y < 0 || tile_y >= wc.height_tiles {
        return tt.air;
    }

    let tile_x = wc.wrap_tile_x(tile_x);
    let surface_y = surface_height(seed, tile_x, wc);

    if tile_y > surface_y {
        return tt.air;
    }
    if tile_y == surface_y {
        return tt.grass;
    }
    if tile_y > surface_y - DIRT_DEPTH {
        return tt.dirt;
    }

    let cave_perlin = Perlin::new(seed.wrapping_add(1));
    let angle = tile_x as f64 / wc.width_tiles as f64 * 2.0 * std::f64::consts::PI;
    let radius = wc.width_tiles as f64 * CAVE_FREQUENCY / (2.0 * std::f64::consts::PI);
    let cave_val = cave_perlin.get([
        radius * angle.cos(),
        radius * angle.sin(),
        tile_y as f64 * CAVE_FREQUENCY,
    ]);
    if cave_val.abs() < CAVE_THRESHOLD {
        tt.air
    } else {
        tt.stone
    }
}

pub fn generate_chunk_tiles(
    seed: u32,
    chunk_x: i32,
    chunk_y: i32,
    wc: &WorldConfig,
    tt: &TerrainTiles,
) -> Vec<TileId> {
    let base_x = chunk_x * wc.chunk_size as i32;
    let base_y = chunk_y * wc.chunk_size as i32;
    let mut tiles = Vec::with_capacity((wc.chunk_size * wc.chunk_size) as usize);

    for local_y in 0..wc.chunk_size as i32 {
        for local_x in 0..wc.chunk_size as i32 {
            tiles.push(generate_tile(
                seed,
                base_x + local_x,
                base_y + local_y,
                wc,
                tt,
            ));
        }
    }

    tiles
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SEED: u32 = 42;

    fn test_wc() -> WorldConfig {
        WorldConfig {
            width_tiles: 2048,
            height_tiles: 1024,
            chunk_size: 32,
            tile_size: 32.0,
            chunk_load_radius: 3,
            seed: 42,
        }
    }

    fn test_tt() -> TerrainTiles {
        TerrainTiles {
            air: TileId(0),
            grass: TileId(1),
            dirt: TileId(2),
            stone: TileId(3),
        }
    }

    #[test]
    fn surface_height_is_deterministic() {
        let wc = test_wc();
        let h1 = surface_height(TEST_SEED, 100, &wc);
        let h2 = surface_height(TEST_SEED, 100, &wc);
        assert_eq!(h1, h2);
    }

    #[test]
    fn surface_height_is_within_bounds() {
        let wc = test_wc();
        for x in 0..wc.width_tiles {
            let h = surface_height(TEST_SEED, x, &wc);
            assert!(h >= 0 && h < wc.height_tiles, "surface at x={x} is {h}");
        }
    }

    #[test]
    fn above_surface_is_air() {
        let wc = test_wc();
        let tt = test_tt();
        let h = surface_height(TEST_SEED, 500, &wc);
        assert_eq!(generate_tile(TEST_SEED, 500, h + 1, &wc, &tt), tt.air);
        assert_eq!(generate_tile(TEST_SEED, 500, h + 10, &wc, &tt), tt.air);
    }

    #[test]
    fn surface_is_grass() {
        let wc = test_wc();
        let tt = test_tt();
        let h = surface_height(TEST_SEED, 500, &wc);
        assert_eq!(generate_tile(TEST_SEED, 500, h, &wc, &tt), tt.grass);
    }

    #[test]
    fn below_surface_is_dirt_then_stone() {
        let wc = test_wc();
        let tt = test_tt();
        let h = surface_height(TEST_SEED, 500, &wc);
        assert_eq!(generate_tile(TEST_SEED, 500, h - 1, &wc, &tt), tt.dirt);
        let deep_tile = generate_tile(TEST_SEED, 500, 10, &wc, &tt);
        assert!(deep_tile == tt.stone || deep_tile == tt.air);
    }

    #[test]
    fn chunk_generation_has_correct_size() {
        let wc = test_wc();
        let tt = test_tt();
        let tiles = generate_chunk_tiles(TEST_SEED, 0, 0, &wc, &tt);
        assert_eq!(tiles.len(), (wc.chunk_size * wc.chunk_size) as usize);
    }

    #[test]
    fn chunk_generation_is_deterministic() {
        let wc = test_wc();
        let tt = test_tt();
        let tiles1 = generate_chunk_tiles(TEST_SEED, 5, 10, &wc, &tt);
        let tiles2 = generate_chunk_tiles(TEST_SEED, 5, 10, &wc, &tt);
        assert_eq!(tiles1, tiles2);
    }

    #[test]
    fn out_of_bounds_y_is_air() {
        let wc = test_wc();
        let tt = test_tt();
        assert_eq!(generate_tile(TEST_SEED, 500, -1, &wc, &tt), tt.air);
        assert_eq!(
            generate_tile(TEST_SEED, 500, wc.height_tiles, &wc, &tt),
            tt.air
        );
    }

    #[test]
    fn x_wraps_around() {
        let wc = test_wc();
        let tt = test_tt();
        let t1 = generate_tile(TEST_SEED, -1, 500, &wc, &tt);
        let t2 = generate_tile(TEST_SEED, wc.width_tiles - 1, 500, &wc, &tt);
        assert_eq!(t1, t2);

        let t3 = generate_tile(TEST_SEED, wc.width_tiles, 500, &wc, &tt);
        let t4 = generate_tile(TEST_SEED, 0, 500, &wc, &tt);
        assert_eq!(t3, t4);
    }

    #[test]
    fn surface_height_wraps_seamlessly() {
        let wc = test_wc();
        let h0 = surface_height(TEST_SEED, 0, &wc);
        let h_wrap = surface_height(TEST_SEED, wc.width_tiles, &wc);
        assert_eq!(h0, h_wrap);

        let h_neg = surface_height(TEST_SEED, -1, &wc);
        let h_pos = surface_height(TEST_SEED, wc.width_tiles - 1, &wc);
        assert_eq!(h_neg, h_pos);
    }
}
