use noise::{NoiseFn, Perlin};

use crate::registry::biome::WorldLayer;
use crate::registry::tile::TileId;
use crate::registry::world::WorldConfig;
use crate::world::ctx::WorldCtxRef;

const SURFACE_BASE: f64 = 0.7;

pub fn surface_height(
    seed: u32,
    tile_x: i32,
    wc: &WorldConfig,
    frequency: f64,
    amplitude: f64,
) -> i32 {
    let perlin = Perlin::new(seed);
    let base = SURFACE_BASE * wc.height_tiles as f64;

    let angle = tile_x as f64 / wc.width_tiles as f64 * 2.0 * std::f64::consts::PI;
    let radius = wc.width_tiles as f64 * frequency / (2.0 * std::f64::consts::PI);
    let nx = radius * angle.cos();
    let ny = radius * angle.sin();
    let noise_val = perlin.get([nx, ny]);

    (base + noise_val * amplitude) as i32
}

pub fn generate_tile(seed: u32, tile_x: i32, tile_y: i32, ctx: &WorldCtxRef) -> TileId {
    let wc = ctx.config;
    let biome_map = ctx.biome_map;
    let biome_registry = ctx.biome_registry;
    let planet_config = ctx.planet_config;

    if tile_y < 0 || tile_y >= wc.height_tiles {
        return TileId::AIR;
    }

    let tile_x = wc.wrap_tile_x(tile_x);

    // Determine vertical layer
    let layer = WorldLayer::from_tile_y(tile_y, wc.height_tiles);

    // Get biome for this position
    let biome_id = match layer {
        WorldLayer::Surface => biome_map.biome_at(tile_x as u32).to_string(),
        WorldLayer::Underground => planet_config
            .layers
            .underground
            .primary_biome
            .clone()
            .unwrap_or_else(|| "underground_dirt".to_string()),
        WorldLayer::DeepUnderground => planet_config
            .layers
            .deep_underground
            .primary_biome
            .clone()
            .unwrap_or_else(|| "underground_rock".to_string()),
        WorldLayer::Core => planet_config
            .layers
            .core
            .primary_biome
            .clone()
            .unwrap_or_else(|| "core_magma".to_string()),
    };

    let biome = biome_registry.get(&biome_id);

    // Surface height (using surface layer params)
    let surface_y = surface_height(
        seed,
        tile_x,
        wc,
        planet_config.layers.surface.terrain_frequency,
        planet_config.layers.surface.terrain_amplitude,
    );

    // Above surface = air
    if tile_y > surface_y {
        return TileId::AIR;
    }

    // Surface/subsurface blocks: always use the surface biome regardless of
    // vertical layer, since the surface height can straddle layer boundaries.
    let surface_biome_id = biome_map.biome_at(tile_x as u32);
    let surface_biome = biome_registry.get(surface_biome_id);
    if tile_y == surface_y {
        return surface_biome.surface_block;
    }
    if tile_y > surface_y - surface_biome.subsurface_depth {
        return surface_biome.subsurface_block;
    }

    // Cave generation using layer-specific frequency
    let cave_perlin = Perlin::new(seed.wrapping_add(1));
    let layer_freq = match layer {
        WorldLayer::Surface => planet_config.layers.surface.terrain_frequency,
        WorldLayer::Underground => planet_config.layers.underground.terrain_frequency,
        WorldLayer::DeepUnderground => planet_config.layers.deep_underground.terrain_frequency,
        WorldLayer::Core => planet_config.layers.core.terrain_frequency,
    };
    let angle = tile_x as f64 / wc.width_tiles as f64 * 2.0 * std::f64::consts::PI;
    let radius = wc.width_tiles as f64 * layer_freq / (2.0 * std::f64::consts::PI);
    let cave_val = cave_perlin.get([
        radius * angle.cos(),
        radius * angle.sin(),
        tile_y as f64 * layer_freq,
    ]);
    if cave_val.abs() < biome.cave_threshold {
        TileId::AIR
    } else {
        biome.fill_block
    }
}

pub fn generate_chunk_tiles(
    seed: u32,
    chunk_x: i32,
    chunk_y: i32,
    ctx: &WorldCtxRef,
) -> Vec<TileId> {
    let chunk_size = ctx.config.chunk_size;
    let base_x = chunk_x * chunk_size as i32;
    let base_y = chunk_y * chunk_size as i32;
    let mut tiles = Vec::with_capacity((chunk_size * chunk_size) as usize);

    for local_y in 0..chunk_size as i32 {
        for local_x in 0..chunk_size as i32 {
            tiles.push(generate_tile(seed, base_x + local_x, base_y + local_y, ctx));
        }
    }

    tiles
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::fixtures;

    const TEST_SEED: u32 = 42;

    #[test]
    fn surface_height_is_deterministic() {
        let wc = fixtures::test_world_config();
        let pc = fixtures::test_planet_config();
        let h1 = surface_height(
            TEST_SEED,
            100,
            &wc,
            pc.layers.surface.terrain_frequency,
            pc.layers.surface.terrain_amplitude,
        );
        let h2 = surface_height(
            TEST_SEED,
            100,
            &wc,
            pc.layers.surface.terrain_frequency,
            pc.layers.surface.terrain_amplitude,
        );
        assert_eq!(h1, h2);
    }

    #[test]
    fn surface_height_is_within_bounds() {
        let wc = fixtures::test_world_config();
        let pc = fixtures::test_planet_config();
        for x in 0..wc.width_tiles {
            let h = surface_height(
                TEST_SEED,
                x,
                &wc,
                pc.layers.surface.terrain_frequency,
                pc.layers.surface.terrain_amplitude,
            );
            assert!(h >= 0 && h < wc.height_tiles, "surface at x={x} is {h}");
        }
    }

    #[test]
    fn above_surface_is_air() {
        let (wc, bm, br, tr, pc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc);
        let h = surface_height(
            TEST_SEED,
            500,
            &wc,
            pc.layers.surface.terrain_frequency,
            pc.layers.surface.terrain_amplitude,
        );
        assert_eq!(generate_tile(TEST_SEED, 500, h + 1, &ctx), TileId::AIR);
        assert_eq!(generate_tile(TEST_SEED, 500, h + 10, &ctx), TileId::AIR);
    }

    #[test]
    fn surface_is_biome_surface_block() {
        let (wc, bm, br, tr, pc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc);
        let h = surface_height(
            TEST_SEED,
            500,
            &wc,
            pc.layers.surface.terrain_frequency,
            pc.layers.surface.terrain_amplitude,
        );
        let tile = generate_tile(TEST_SEED, 500, h, &ctx);
        let biome_id = bm.biome_at(500);
        let biome = br.get(biome_id);
        assert_eq!(tile, biome.surface_block);
    }

    #[test]
    fn below_surface_is_subsurface_then_fill_or_air() {
        let (wc, bm, br, tr, pc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc);
        let h = surface_height(
            TEST_SEED,
            500,
            &wc,
            pc.layers.surface.terrain_frequency,
            pc.layers.surface.terrain_amplitude,
        );
        let biome_id = bm.biome_at(500);
        let biome = br.get(biome_id);
        assert_eq!(
            generate_tile(TEST_SEED, 500, h - 1, &ctx),
            biome.subsurface_block
        );
        let deep_tile = generate_tile(TEST_SEED, 500, 10, &ctx);
        assert!(deep_tile == TileId(3) || deep_tile == TileId::AIR);
    }

    #[test]
    fn chunk_generation_has_correct_size() {
        let (wc, bm, br, tr, pc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc);
        let tiles = generate_chunk_tiles(TEST_SEED, 0, 0, &ctx);
        assert_eq!(tiles.len(), (wc.chunk_size * wc.chunk_size) as usize);
    }

    #[test]
    fn chunk_generation_is_deterministic() {
        let (wc, bm, br, tr, pc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc);
        let tiles1 = generate_chunk_tiles(TEST_SEED, 5, 10, &ctx);
        let tiles2 = generate_chunk_tiles(TEST_SEED, 5, 10, &ctx);
        assert_eq!(tiles1, tiles2);
    }

    #[test]
    fn out_of_bounds_y_is_air() {
        let (wc, bm, br, tr, pc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc);
        assert_eq!(generate_tile(TEST_SEED, 500, -1, &ctx), TileId::AIR);
        assert_eq!(
            generate_tile(TEST_SEED, 500, wc.height_tiles, &ctx),
            TileId::AIR
        );
    }

    #[test]
    fn x_wraps_around() {
        let (wc, bm, br, tr, pc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc);
        let t1 = generate_tile(TEST_SEED, -1, 500, &ctx);
        let t2 = generate_tile(TEST_SEED, wc.width_tiles - 1, 500, &ctx);
        assert_eq!(t1, t2);

        let t3 = generate_tile(TEST_SEED, wc.width_tiles, 500, &ctx);
        let t4 = generate_tile(TEST_SEED, 0, 500, &ctx);
        assert_eq!(t3, t4);
    }

    #[test]
    fn surface_height_wraps_seamlessly() {
        let wc = fixtures::test_world_config();
        let pc = fixtures::test_planet_config();
        let freq = pc.layers.surface.terrain_frequency;
        let amp = pc.layers.surface.terrain_amplitude;
        let h0 = surface_height(TEST_SEED, 0, &wc, freq, amp);
        let h_wrap = surface_height(TEST_SEED, wc.width_tiles, &wc, freq, amp);
        assert_eq!(h0, h_wrap);

        let h_neg = surface_height(TEST_SEED, -1, &wc, freq, amp);
        let h_pos = surface_height(TEST_SEED, wc.width_tiles - 1, &wc, freq, amp);
        assert_eq!(h_neg, h_pos);
    }
}
