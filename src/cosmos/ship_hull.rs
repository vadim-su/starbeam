//! Ship starter hull generation.
//!
//! When a ship world is first loaded, [`generate_starter_hull`] places a small
//! stone structure with functional blocks (airlock, fuel tank, autopilot
//! console) at the center of the world.  The hull is 16 tiles wide by 8 tiles
//! tall, positioned so that its center aligns with the center of the 128x64
//! ship world.

use bevy::prelude::*;

use crate::cosmos::address::CelestialAddress;
use crate::cosmos::persistence::{DirtyChunks, Universe};
use crate::object::placement::place_object;
use crate::object::registry::ObjectRegistry;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::{tile_to_chunk, Layer, WorldMap};
use crate::world::ctx::WorldCtx;

// ---------------------------------------------------------------------------
// Hull constants
// ---------------------------------------------------------------------------

/// Hull dimensions in tiles.
pub const HULL_WIDTH: i32 = 16;
pub const HULL_HEIGHT: i32 = 8;

/// Compute the bottom-left corner of the hull given the world dimensions.
fn hull_origin(width_tiles: i32, height_tiles: i32) -> (i32, i32) {
    let cx = width_tiles / 2;
    let cy = height_tiles / 2;
    (cx - HULL_WIDTH / 2, cy - HULL_HEIGHT / 2)
}

// ---------------------------------------------------------------------------
// Core generation function (pure, testable)
// ---------------------------------------------------------------------------

/// Place the starter hull into `world_map`.
///
/// This writes stone tiles for walls/floor/ceiling on the foreground layer,
/// fills the entire interior with stone on the background layer, and places
/// functional objects (airlock, fuel tank, autopilot console) on the floor.
///
/// Returns `true` if the hull was generated, `false` if it was skipped
/// (e.g. already present).
pub fn generate_starter_hull(
    world_map: &mut WorldMap,
    tile_registry: &crate::registry::tile::TileRegistry,
    object_registry: &ObjectRegistry,
    ctx: &crate::world::ctx::WorldCtxRef,
    dirty_chunks: &mut DirtyChunks,
) -> bool {
    let stone = tile_registry.by_name("stone");
    let (ox, oy) = hull_origin(ctx.config.width_tiles, ctx.config.height_tiles);

    // Quick check: if the bottom-left corner is already stone, skip.
    // This prevents regenerating when revisiting the ship.
    if let Some(tile) = world_map.get_tile(ox, oy, Layer::Fg, ctx) {
        if tile == stone {
            return false;
        }
    }

    // --- Ensure all relevant chunks exist ---
    let cs = ctx.config.chunk_size;
    for ty in oy..(oy + HULL_HEIGHT) {
        for tx in ox..(ox + HULL_WIDTH) {
            let (cx, cy) = tile_to_chunk(tx, ty, cs);
            world_map.get_or_generate_chunk(cx, cy, ctx);
        }
    }

    // --- Place foreground tiles (walls, floor, ceiling) ---
    for ty in oy..(oy + HULL_HEIGHT) {
        for tx in ox..(ox + HULL_WIDTH) {
            let is_floor = ty == oy;
            let is_ceiling = ty == oy + HULL_HEIGHT - 1;
            let is_left_wall = tx == ox;
            let is_right_wall = tx == ox + HULL_WIDTH - 1;

            if is_floor || is_ceiling || is_left_wall || is_right_wall {
                world_map.set_tile(tx, ty, Layer::Fg, stone, ctx);
            }
            // Interior remains AIR on foreground (already default for ship worlds)
        }
    }

    // --- Place background tiles (entire hull interior filled with stone) ---
    for ty in oy..(oy + HULL_HEIGHT) {
        for tx in ox..(ox + HULL_WIDTH) {
            world_map.set_tile(tx, ty, Layer::Bg, stone, ctx);
        }
    }

    // --- Mark affected chunks as dirty ---
    for ty in oy..(oy + HULL_HEIGHT) {
        for tx in ox..(ox + HULL_WIDTH) {
            let (cx, cy) = tile_to_chunk(tx, ty, cs);
            dirty_chunks.0.insert((cx, cy));
        }
    }

    // --- Place objects on the floor (y = oy + 1, the first interior row) ---
    let floor_y = oy + 1;

    // Object positions: spread across the 14-tile interior (x = ox+1 to ox+14)
    // Airlock (2x3) at left side
    if let Some(airlock_id) = object_registry.by_name("airlock") {
        place_object(world_map, object_registry, airlock_id, ox + 2, floor_y, ctx);
    }

    // Fuel tank (2x3) in the middle-left
    if let Some(fuel_tank_id) = object_registry.by_name("fuel_tank") {
        place_object(
            world_map,
            object_registry,
            fuel_tank_id,
            ox + 6,
            floor_y,
            ctx,
        );
    }

    // Autopilot console (2x2) on the right side
    if let Some(console_id) = object_registry.by_name("autopilot_console") {
        place_object(
            world_map,
            object_registry,
            console_id,
            ox + 11,
            floor_y,
            ctx,
        );
    }

    info!(
        "Generated starter hull at ({}, {}) — {}x{} tiles",
        ox, oy, HULL_WIDTH, HULL_HEIGHT
    );
    true
}

// ---------------------------------------------------------------------------
// Bevy system — runs once on OnEnter(InGame) for new ship worlds
// ---------------------------------------------------------------------------

/// Marker resource indicating that the ship hull has already been generated
/// for the current world. Prevents double-generation within the same session.
#[derive(Resource)]
pub struct ShipHullGenerated;

/// System that generates the starter hull for new ship worlds.
///
/// Runs on `OnEnter(InGame)`. Checks:
/// 1. The active world is a ship world.
/// 2. The ship has no saved data in [`Universe`] (first visit).
/// 3. [`ShipHullGenerated`] marker is absent (not already done this session).
#[allow(clippy::too_many_arguments)]
pub fn generate_ship_hull_system(
    mut commands: Commands,
    active_world: Option<Res<ActiveWorld>>,
    universe: Res<Universe>,
    ctx: WorldCtx,
    mut world_map: ResMut<WorldMap>,
    object_registry: Option<Res<ObjectRegistry>>,
    mut dirty_chunks: ResMut<DirtyChunks>,
    existing_marker: Option<Res<ShipHullGenerated>>,
) {
    // Only for ship worlds
    let Some(ref aw) = active_world else { return };
    if !matches!(aw.address, CelestialAddress::Ship { .. }) {
        return;
    }

    // Skip if already generated this session
    if existing_marker.is_some() {
        return;
    }

    // Skip if the ship has been saved before (has data in Universe)
    if universe.planets.contains_key(&aw.address) {
        commands.insert_resource(ShipHullGenerated);
        return;
    }

    let Some(ref obj_reg) = object_registry else {
        return;
    };

    let ctx_ref = ctx.as_ref();
    let generated = generate_starter_hull(
        &mut world_map,
        ctx_ref.tile_registry,
        obj_reg,
        &ctx_ref,
        &mut dirty_chunks,
    );

    if generated {
        commands.insert_resource(ShipHullGenerated);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cosmos::address::{CelestialAddress, CelestialSeeds};
    use crate::cosmos::persistence::DirtyChunks;
    use crate::object::definition::{ObjectDef, ObjectType, PlacementRule};
    use crate::object::placement::get_object_at;
    use crate::object::registry::ObjectRegistry;
    use crate::registry::biome::{
        BiomeDef, BiomeRegistry, LayerBoundaries, LayerConfig, LayerConfigs, PlanetConfig,
    };
    use crate::registry::tile::{TileDef, TileId, TileRegistry};
    use crate::registry::world::ActiveWorld;
    use crate::world::biome_map::BiomeMap;
    use crate::world::chunk::WorldMap;
    use crate::world::ctx::WorldCtxRef;
    use crate::world::terrain_gen::TerrainNoiseCache;

    /// Build a ship-like ActiveWorld (128x64, wrap_x=false).
    fn ship_world() -> ActiveWorld {
        let address = CelestialAddress::Ship { owner_id: 1 };
        let seeds = CelestialSeeds::derive(42, &address);
        ActiveWorld {
            address,
            seeds,
            width_tiles: 128,
            height_tiles: 64,
            chunk_size: 32,
            tile_size: 16.0,
            chunk_load_radius: 3,
            seed: 42,
            planet_type: "ship".into(),
            wrap_x: false,
        }
    }

    fn ship_biome_registry() -> BiomeRegistry {
        let mut reg = BiomeRegistry::default();
        // Ship worlds use a single "deep_space" biome; use a minimal stand-in.
        reg.insert(
            "deep_space",
            BiomeDef {
                id: "deep_space".into(),
                surface_block: TileId::AIR,
                subsurface_block: TileId::AIR,
                subsurface_depth: 0,
                fill_block: TileId::AIR,
                cave_threshold: 1.0,
                parallax_path: None,
            },
        );
        reg
    }

    fn ship_planet_config() -> PlanetConfig {
        let layers = LayerConfigs {
            surface: LayerConfig {
                primary_biome: Some("deep_space".into()),
                terrain_frequency: 0.0,
                terrain_amplitude: 0.0,
                depth_ratio: 1.0,
            },
            underground: LayerConfig {
                primary_biome: Some("deep_space".into()),
                terrain_frequency: 0.0,
                terrain_amplitude: 0.0,
                depth_ratio: 0.0,
            },
            deep_underground: LayerConfig {
                primary_biome: Some("deep_space".into()),
                terrain_frequency: 0.0,
                terrain_amplitude: 0.0,
                depth_ratio: 0.0,
            },
            core: LayerConfig {
                primary_biome: Some("deep_space".into()),
                terrain_frequency: 0.0,
                terrain_amplitude: 0.0,
                depth_ratio: 0.0,
            },
        };
        let layer_boundaries = LayerBoundaries::from_layers(&layers, 64);
        PlanetConfig {
            id: "ship".into(),
            primary_biome: "deep_space".into(),
            secondary_biomes: vec![],
            layers,
            layer_boundaries,
            region_width_min: 128,
            region_width_max: 128,
            primary_region_ratio: 1.0,
        }
    }

    fn ship_tile_registry() -> TileRegistry {
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
                flicker_speed: 0.0,
                flicker_strength: 0.0,
                flicker_min: 1.0,
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
                light_opacity: 15,
                albedo: [128, 128, 128],
                flicker_speed: 0.0,
                flicker_strength: 0.0,
                flicker_min: 1.0,
                drops: vec![],
            },
        ])
    }

    fn ship_object_registry() -> ObjectRegistry {
        ObjectRegistry::from_defs(vec![
            ObjectDef {
                id: "none".into(),
                display_name: "None".into(),
                size: (1, 1),
                sprite: "".into(),
                solid_mask: vec![false],
                placement: PlacementRule::Any,
                light_emission: [0, 0, 0],
                object_type: ObjectType::Decoration,
                drops: vec![],
                sprite_columns: 1,
                sprite_rows: 1,
                sprite_fps: 0.0,
                flicker_speed: 0.0,
                flicker_strength: 0.0,
                flicker_min: 1.0,
                auto_item: None,
                background: false,
            },
            ObjectDef {
                id: "airlock".into(),
                display_name: "Airlock".into(),
                size: (2, 3),
                sprite: "".into(),
                solid_mask: vec![true; 6],
                placement: PlacementRule::Floor,
                light_emission: [0, 0, 0],
                object_type: ObjectType::Airlock,
                drops: vec![],
                sprite_columns: 1,
                sprite_rows: 1,
                sprite_fps: 0.0,
                flicker_speed: 0.0,
                flicker_strength: 0.0,
                flicker_min: 1.0,
                auto_item: None,
                background: false,
            },
            ObjectDef {
                id: "fuel_tank".into(),
                display_name: "Fuel Tank".into(),
                size: (2, 3),
                sprite: "".into(),
                solid_mask: vec![true; 6],
                placement: PlacementRule::Floor,
                light_emission: [0, 0, 0],
                object_type: ObjectType::FuelTank { capacity: 100.0 },
                drops: vec![],
                sprite_columns: 1,
                sprite_rows: 1,
                sprite_fps: 0.0,
                flicker_speed: 0.0,
                flicker_strength: 0.0,
                flicker_min: 1.0,
                auto_item: None,
                background: false,
            },
            ObjectDef {
                id: "autopilot_console".into(),
                display_name: "Autopilot Console".into(),
                size: (2, 2),
                sprite: "".into(),
                solid_mask: vec![true; 4],
                placement: PlacementRule::Floor,
                light_emission: [0, 0, 0],
                object_type: ObjectType::AutopilotConsole,
                drops: vec![],
                sprite_columns: 1,
                sprite_rows: 1,
                sprite_fps: 0.0,
                flicker_speed: 0.0,
                flicker_strength: 0.0,
                flicker_min: 1.0,
                auto_item: None,
                background: false,
            },
        ])
    }

    fn make_ship_ctx<'a>(
        wc: &'a ActiveWorld,
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

    #[test]
    fn hull_origin_is_centered() {
        let (ox, oy) = hull_origin(128, 64);
        assert_eq!(ox, 56);
        assert_eq!(oy, 28);
    }

    #[test]
    fn hull_generates_stone_walls() {
        let wc = ship_world();
        let br = ship_biome_registry();
        let bm = BiomeMap::generate("deep_space", &["deep_space"], 42, 128, 128, 128, 1.0, &br);
        let tr = ship_tile_registry();
        let pc = ship_planet_config();
        let nc = TerrainNoiseCache::new(42);
        let ctx = make_ship_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let obj_reg = ship_object_registry();
        let mut world_map = WorldMap::default();
        let mut dirty = DirtyChunks::default();

        let result = generate_starter_hull(&mut world_map, &tr, &obj_reg, &ctx, &mut dirty);
        assert!(result);

        let stone = tr.by_name("stone");
        let (ox, oy) = hull_origin(128, 64);

        // Floor row (y = oy) should be all stone
        for x in ox..(ox + HULL_WIDTH) {
            assert_eq!(
                world_map.get_tile(x, oy, Layer::Fg, &ctx),
                Some(stone),
                "floor at ({x}, {oy})"
            );
        }

        // Ceiling row (y = oy + 7) should be all stone
        for x in ox..(ox + HULL_WIDTH) {
            assert_eq!(
                world_map.get_tile(x, oy + HULL_HEIGHT - 1, Layer::Fg, &ctx),
                Some(stone),
                "ceiling at ({x}, {})",
                oy + HULL_HEIGHT - 1
            );
        }

        // Left wall
        for y in oy..(oy + HULL_HEIGHT) {
            assert_eq!(
                world_map.get_tile(ox, y, Layer::Fg, &ctx),
                Some(stone),
                "left wall at ({ox}, {y})"
            );
        }

        // Right wall
        for y in oy..(oy + HULL_HEIGHT) {
            assert_eq!(
                world_map.get_tile(ox + HULL_WIDTH - 1, y, Layer::Fg, &ctx),
                Some(stone),
                "right wall at ({}, {y})",
                ox + HULL_WIDTH - 1
            );
        }

        // Interior should be air on foreground
        for y in (oy + 1)..(oy + HULL_HEIGHT - 1) {
            for x in (ox + 1)..(ox + HULL_WIDTH - 1) {
                // Skip tiles occupied by objects
                if get_object_at(&world_map, x, y, &ctx).is_some() {
                    continue;
                }
                assert_eq!(
                    world_map.get_tile(x, y, Layer::Fg, &ctx),
                    Some(TileId::AIR),
                    "interior fg at ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn hull_background_is_stone() {
        let wc = ship_world();
        let br = ship_biome_registry();
        let bm = BiomeMap::generate("deep_space", &["deep_space"], 42, 128, 128, 128, 1.0, &br);
        let tr = ship_tile_registry();
        let pc = ship_planet_config();
        let nc = TerrainNoiseCache::new(42);
        let ctx = make_ship_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let obj_reg = ship_object_registry();
        let mut world_map = WorldMap::default();
        let mut dirty = DirtyChunks::default();

        generate_starter_hull(&mut world_map, &tr, &obj_reg, &ctx, &mut dirty);

        let stone = tr.by_name("stone");
        let (ox, oy) = hull_origin(128, 64);

        // Entire hull area should have stone background
        for y in oy..(oy + HULL_HEIGHT) {
            for x in ox..(ox + HULL_WIDTH) {
                assert_eq!(
                    world_map.get_tile(x, y, Layer::Bg, &ctx),
                    Some(stone),
                    "bg at ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn hull_objects_are_placed() {
        let wc = ship_world();
        let br = ship_biome_registry();
        let bm = BiomeMap::generate("deep_space", &["deep_space"], 42, 128, 128, 128, 1.0, &br);
        let tr = ship_tile_registry();
        let pc = ship_planet_config();
        let nc = TerrainNoiseCache::new(42);
        let ctx = make_ship_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let obj_reg = ship_object_registry();
        let mut world_map = WorldMap::default();
        let mut dirty = DirtyChunks::default();

        generate_starter_hull(&mut world_map, &tr, &obj_reg, &ctx, &mut dirty);

        let (ox, oy) = hull_origin(128, 64);
        let floor_y = oy + 1;

        // Airlock at ox+2
        let airlock_id = obj_reg.by_name("airlock").unwrap();
        let result = get_object_at(&world_map, ox + 2, floor_y, &ctx);
        assert!(result.is_some(), "airlock should be placed");
        assert_eq!(result.unwrap().3, airlock_id);

        // Fuel tank at ox+6
        let fuel_tank_id = obj_reg.by_name("fuel_tank").unwrap();
        let result = get_object_at(&world_map, ox + 6, floor_y, &ctx);
        assert!(result.is_some(), "fuel tank should be placed");
        assert_eq!(result.unwrap().3, fuel_tank_id);

        // Autopilot console at ox+11
        let console_id = obj_reg.by_name("autopilot_console").unwrap();
        let result = get_object_at(&world_map, ox + 11, floor_y, &ctx);
        assert!(result.is_some(), "autopilot console should be placed");
        assert_eq!(result.unwrap().3, console_id);
    }

    #[test]
    fn hull_not_regenerated_twice() {
        let wc = ship_world();
        let br = ship_biome_registry();
        let bm = BiomeMap::generate("deep_space", &["deep_space"], 42, 128, 128, 128, 1.0, &br);
        let tr = ship_tile_registry();
        let pc = ship_planet_config();
        let nc = TerrainNoiseCache::new(42);
        let ctx = make_ship_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let obj_reg = ship_object_registry();
        let mut world_map = WorldMap::default();
        let mut dirty = DirtyChunks::default();

        assert!(generate_starter_hull(
            &mut world_map,
            &tr,
            &obj_reg,
            &ctx,
            &mut dirty
        ));
        assert!(!generate_starter_hull(
            &mut world_map,
            &tr,
            &obj_reg,
            &ctx,
            &mut dirty
        ));
    }

    #[test]
    fn dirty_chunks_are_marked() {
        let wc = ship_world();
        let br = ship_biome_registry();
        let bm = BiomeMap::generate("deep_space", &["deep_space"], 42, 128, 128, 128, 1.0, &br);
        let tr = ship_tile_registry();
        let pc = ship_planet_config();
        let nc = TerrainNoiseCache::new(42);
        let ctx = make_ship_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let obj_reg = ship_object_registry();
        let mut world_map = WorldMap::default();
        let mut dirty = DirtyChunks::default();

        generate_starter_hull(&mut world_map, &tr, &obj_reg, &ctx, &mut dirty);

        // Hull spans chunks (1,0) and (2,0) for a 128x64 world with chunk_size=32
        assert!(dirty.0.contains(&(1, 0)), "chunk (1,0) should be dirty");
        assert!(!dirty.0.is_empty());
    }
}
