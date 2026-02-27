pub mod fixtures {
    use bevy::prelude::*;

    use crate::registry::biome::{
        BiomeDef, BiomeRegistry, LayerBoundaries, LayerConfig, LayerConfigs, PlanetConfig,
    };
    use crate::registry::player::PlayerConfig;
    use crate::registry::tile::{TileDef, TileId, TileRegistry};
    use crate::registry::world::WorldConfig;
    use crate::world::biome_map::BiomeMap;
    use crate::world::chunk::WorldMap;
    use crate::world::ctx::WorldCtxRef;
    use crate::world::terrain_gen::TerrainNoiseCache;

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

    pub fn test_biome_map(biome_registry: &BiomeRegistry) -> BiomeMap {
        BiomeMap::generate(
            "meadow",
            &["forest", "rocky"],
            42,
            2048,
            300,
            600,
            0.6,
            biome_registry,
        )
    }

    pub fn test_biome_registry() -> BiomeRegistry {
        let mut reg = BiomeRegistry::default();
        for (name, surface, subsurface, depth, fill, threshold) in [
            ("meadow", TileId(1), TileId(2), 4, TileId(3), 0.3),
            ("forest", TileId(1), TileId(2), 4, TileId(3), 0.3),
            ("rocky", TileId(3), TileId(3), 2, TileId(3), 0.3),
            ("underground_dirt", TileId(3), TileId(3), 0, TileId(3), 0.3),
            ("underground_rock", TileId(3), TileId(3), 0, TileId(3), 0.25),
            ("core_magma", TileId(3), TileId(3), 0, TileId(3), 0.15),
        ] {
            reg.insert(
                name,
                BiomeDef {
                    id: name.into(),
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
                light_emission: [0, 0, 0],
                light_opacity: 0,
                albedo: [0, 0, 0],
                drops: vec![],
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
                light_emission: [0, 0, 0],
                light_opacity: 4,
                albedo: [34, 139, 34],
                drops: vec![],
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
                light_emission: [0, 0, 0],
                light_opacity: 5,
                albedo: [139, 90, 43],
                drops: vec![],
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
                light_emission: [0, 0, 0],
                light_opacity: 8,
                albedo: [128, 128, 128],
                drops: vec![],
            },
            TileDef {
                id: "torch".into(),
                autotile: None,
                solid: false,
                hardness: 0.5,
                friction: 0.0,
                viscosity: 0.0,
                damage_on_contact: 0.0,
                effects: vec![],
                light_emission: [240, 180, 80],
                light_opacity: 0,
                albedo: [200, 160, 80],
                drops: vec![],
            },
        ])
    }

    pub fn test_planet_config() -> PlanetConfig {
        let layers = LayerConfigs {
            surface: LayerConfig {
                primary_biome: None,
                terrain_frequency: 0.02,
                terrain_amplitude: 40.0,
                depth_ratio: 0.30,
            },
            underground: LayerConfig {
                primary_biome: Some("underground_dirt".into()),
                terrain_frequency: 0.07,
                terrain_amplitude: 1.0,
                depth_ratio: 0.25,
            },
            deep_underground: LayerConfig {
                primary_biome: Some("underground_rock".into()),
                terrain_frequency: 0.05,
                terrain_amplitude: 1.0,
                depth_ratio: 0.33,
            },
            core: LayerConfig {
                primary_biome: Some("core_magma".into()),
                terrain_frequency: 0.04,
                terrain_amplitude: 1.0,
                depth_ratio: 0.12,
            },
        };
        let layer_boundaries = LayerBoundaries::from_layers(&layers, 1024);
        PlanetConfig {
            id: "garden".into(),
            primary_biome: "meadow".into(),
            secondary_biomes: vec!["forest".into(), "rocky".into()],
            layers,
            layer_boundaries,
            region_width_min: 300,
            region_width_max: 600,
            primary_region_ratio: 0.6,
        }
    }

    pub fn test_noise_cache() -> TerrainNoiseCache {
        TerrainNoiseCache::new(42)
    }

    /// Returns all resources needed to construct a `WorldCtxRef` for tests.
    pub fn test_world_ctx() -> (
        WorldConfig,
        BiomeMap,
        BiomeRegistry,
        TileRegistry,
        PlanetConfig,
        TerrainNoiseCache,
    ) {
        let br = test_biome_registry();
        let bm = test_biome_map(&br);
        (
            test_world_config(),
            bm,
            br,
            test_tile_registry(),
            test_planet_config(),
            test_noise_cache(),
        )
    }

    /// Convenience constructor for `WorldCtxRef` from individual references.
    pub fn make_ctx<'a>(
        wc: &'a WorldConfig,
        bm: &'a BiomeMap,
        br: &'a BiomeRegistry,
        tr: &'a TileRegistry,
        pc: &'a PlanetConfig,
        nc: &'a TerrainNoiseCache,
    ) -> WorldCtxRef<'a> {
        WorldCtxRef {
            config: wc,
            biome_map: bm,
            biome_registry: br,
            tile_registry: tr,
            planet_config: pc,
            noise_cache: nc,
        }
    }

    pub fn test_player_config() -> PlayerConfig {
        PlayerConfig {
            speed: 200.0,
            jump_velocity: 500.0,
            gravity: 980.0,
            width: 24.0,
            height: 40.0,
        }
    }

    /// Create a minimal Bevy App with all world resources for system tests.
    pub fn test_app() -> App {
        let br = test_biome_registry();
        let bm = test_biome_map(&br);

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(test_world_config());
        app.insert_resource(bm);
        app.insert_resource(br);
        app.insert_resource(test_tile_registry());
        app.insert_resource(test_planet_config());
        app.insert_resource(test_noise_cache());
        app.insert_resource(test_player_config());
        app.init_resource::<WorldMap>();
        app
    }
}
