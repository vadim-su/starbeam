pub mod fixtures {
    use crate::registry::biome::{
        BiomeDef, BiomeRegistry, LayerConfig, LayerConfigs, PlanetConfig,
    };
    use crate::registry::tile::{TileDef, TileId, TileRegistry};
    use crate::registry::world::WorldConfig;
    use crate::world::biome_map::BiomeMap;

    pub fn test_world_config() -> WorldConfig {
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

    pub fn test_biome_map() -> BiomeMap {
        BiomeMap::generate("meadow", &["forest", "rocky"], 42, 2048, 300, 600, 0.6)
    }

    pub fn test_biome_registry() -> BiomeRegistry {
        let mut reg = BiomeRegistry::default();
        for (id, surface, subsurface, depth, fill, threshold) in [
            ("meadow", TileId(1), TileId(2), 4, TileId(3), 0.3),
            ("forest", TileId(1), TileId(2), 4, TileId(3), 0.3),
            ("rocky", TileId(3), TileId(3), 2, TileId(3), 0.3),
            ("underground_dirt", TileId(3), TileId(3), 0, TileId(3), 0.3),
            ("underground_rock", TileId(3), TileId(3), 0, TileId(3), 0.25),
            ("core_magma", TileId(3), TileId(3), 0, TileId(3), 0.15),
        ] {
            reg.biomes.insert(
                id.into(),
                BiomeDef {
                    id: id.into(),
                    surface_block: surface,
                    subsurface_block: subsurface,
                    subsurface_depth: depth,
                    fill_block: fill,
                    cave_threshold: threshold,
                    parallax_path: None,
                },
            );
        }
        reg
    }

    pub fn test_tile_registry() -> TileRegistry {
        TileRegistry::from_defs(vec![
            TileDef {
                id: "air".into(),
                autotile: None,
                solid: false,
                hardness: 0.0,
                friction: 0.0,
                viscosity: 0.0,
                damage_on_contact: 0.0,
                effects: vec![],
            },
            TileDef {
                id: "grass".into(),
                autotile: Some("grass".into()),
                solid: true,
                hardness: 1.0,
                friction: 0.8,
                viscosity: 0.0,
                damage_on_contact: 0.0,
                effects: vec![],
            },
            TileDef {
                id: "dirt".into(),
                autotile: Some("dirt".into()),
                solid: true,
                hardness: 2.0,
                friction: 0.7,
                viscosity: 0.0,
                damage_on_contact: 0.0,
                effects: vec![],
            },
            TileDef {
                id: "stone".into(),
                autotile: Some("stone".into()),
                solid: true,
                hardness: 5.0,
                friction: 0.6,
                viscosity: 0.0,
                damage_on_contact: 0.0,
                effects: vec![],
            },
        ])
    }

    pub fn test_planet_config() -> PlanetConfig {
        PlanetConfig {
            id: "garden".into(),
            primary_biome: "meadow".into(),
            secondary_biomes: vec!["forest".into(), "rocky".into()],
            layers: LayerConfigs {
                surface: LayerConfig {
                    primary_biome: None,
                    terrain_frequency: 0.02,
                    terrain_amplitude: 40.0,
                },
                underground: LayerConfig {
                    primary_biome: Some("underground_dirt".into()),
                    terrain_frequency: 0.07,
                    terrain_amplitude: 1.0,
                },
                deep_underground: LayerConfig {
                    primary_biome: Some("underground_rock".into()),
                    terrain_frequency: 0.05,
                    terrain_amplitude: 1.0,
                },
                core: LayerConfig {
                    primary_biome: Some("core_magma".into()),
                    terrain_frequency: 0.04,
                    terrain_amplitude: 1.0,
                },
            },
            region_width_min: 300,
            region_width_max: 600,
            primary_region_ratio: 0.6,
        }
    }
}
