use crate::registry::biome::BiomeRegistry;
use crate::registry::world::ActiveWorld;
use crate::world::biome_map::BiomeMap;
use crate::world::day_night::WorldTime;

/// Compute the local temperature at a given tile X position.
pub fn local_temperature(
    tile_x: i32,
    world: &ActiveWorld,
    world_time: &WorldTime,
    biome_map: &BiomeMap,
    biome_registry: &BiomeRegistry,
) -> f32 {
    let wrapped_x = world.wrap_tile_x(tile_x).max(0) as u32;
    let biome_id = biome_map.biome_at(wrapped_x);
    let biome = biome_registry.get(biome_id);

    world.base_temperature + world_time.temperature_celsius_offset + biome.temperature_offset
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::fixtures;

    #[test]
    fn local_temp_uses_base_plus_offsets() {
        let mut world = fixtures::test_active_world();
        world.base_temperature = 15.0;
        let br = fixtures::test_biome_registry();
        let bm = fixtures::test_biome_map(&br);
        let mut wt = WorldTime::default();
        wt.temperature_celsius_offset = -5.0;

        // meadow has temperature_offset = 0.0 in test fixtures
        let temp = local_temperature(100, &world, &wt, &bm, &br);
        assert!((temp - 10.0).abs() < 0.01); // 15 + (-5) + 0
    }
}
