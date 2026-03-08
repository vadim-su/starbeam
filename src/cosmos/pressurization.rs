//! Pressurization detection via flood-fill for ship worlds.
//!
//! Determines which tiles are pressurized (inside sealed hull) vs vacuum
//! (connected to world edges). Only active on ship worlds.

use bevy::prelude::*;
use std::collections::{HashMap, VecDeque};

use crate::cosmos::address::CelestialAddress;
use crate::registry::tile::{TileId, TileRegistry};
use crate::registry::world::ActiveWorld;
use crate::sets::GameSet;
use crate::world::chunk::{world_to_tile, WorldMap};

// ---------------------------------------------------------------------------
// Resources & Components
// ---------------------------------------------------------------------------

/// Per-tile pressure state. `true` = pressurized (inside sealed hull).
#[derive(Resource, Default)]
pub struct PressureMap {
    /// key: (tile_x, tile_y), value: true = pressurized
    tiles: HashMap<(i32, i32), bool>,
    pub dirty: bool,
}

impl PressureMap {
    /// Create a new PressureMap marked as dirty for initial calculation.
    pub fn new_dirty() -> Self {
        Self {
            tiles: HashMap::new(),
            dirty: true,
        }
    }

    pub fn is_pressurized(&self, tile_x: i32, tile_y: i32) -> bool {
        self.tiles.get(&(tile_x, tile_y)).copied().unwrap_or(false)
    }
}

/// Component on the player tracking whether they are in vacuum.
#[derive(Component, Debug, Default)]
pub struct InVacuum(pub bool);

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct PressurizationPlugin;

impl Plugin for PressurizationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (recalculate_pressure, update_in_vacuum)
                .chain()
                .in_set(GameSet::Physics),
        );
    }
}

// ---------------------------------------------------------------------------
// Flood-fill algorithm
// ---------------------------------------------------------------------------

/// Run BFS from all edge tiles to find vacuum (reachable from edges through air).
/// Returns a HashMap where `true` = pressurized, for all air tiles.
/// Solid tiles are not included (they are walls).
pub fn compute_pressure(
    world_map: &WorldMap,
    tile_registry: &TileRegistry,
    width: i32,
    height: i32,
) -> HashMap<(i32, i32), bool> {
    let mut vacuum: HashMap<(i32, i32), bool> = HashMap::new();
    let mut visited: HashMap<(i32, i32), bool> = HashMap::new();
    let mut queue: VecDeque<(i32, i32)> = VecDeque::new();

    // Helper: check if a tile is air (not solid) at given coords.
    // For tiles outside the world or in unloaded chunks, treat as air (vacuum).
    let is_air = |tx: i32, ty: i32| -> bool {
        if tx < 0 || tx >= width || ty < 0 || ty >= height {
            return true; // outside world = vacuum
        }
        // Read fg tile directly from chunks (no wrap since ship has wrap_x=false)
        let chunk_size = 32_u32; // ship default
        let cx = tx.div_euclid(chunk_size as i32);
        let cy = ty.div_euclid(chunk_size as i32);
        let lx = tx.rem_euclid(chunk_size as i32) as u32;
        let ly = ty.rem_euclid(chunk_size as i32) as u32;
        match world_map.chunks.get(&(cx, cy)) {
            Some(chunk) => {
                let tile = chunk.fg.get(lx, ly, chunk_size);
                !tile_registry.is_solid(tile)
            }
            None => true, // unloaded = treat as vacuum
        }
    };

    // Seed BFS with all edge tiles that are air
    for x in 0..width {
        for &y in &[0, height - 1] {
            if is_air(x, y) && !visited.contains_key(&(x, y)) {
                visited.insert((x, y), true);
                queue.push_back((x, y));
            }
        }
    }
    for y in 0..height {
        for &x in &[0, width - 1] {
            if is_air(x, y) && !visited.contains_key(&(x, y)) {
                visited.insert((x, y), true);
                queue.push_back((x, y));
            }
        }
    }

    // BFS through air tiles
    while let Some((x, y)) = queue.pop_front() {
        vacuum.insert((x, y), false); // reachable from edge = NOT pressurized

        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || nx >= width || ny < 0 || ny >= height {
                continue;
            }
            if visited.contains_key(&(nx, ny)) {
                continue;
            }
            if is_air(nx, ny) {
                visited.insert((nx, ny), true);
                queue.push_back((nx, ny));
            }
        }
    }

    // Build result: all air tiles not reached by BFS = pressurized
    let mut result = HashMap::new();
    for y in 0..height {
        for x in 0..width {
            if is_air(x, y) {
                let pressurized = !vacuum.contains_key(&(x, y));
                result.insert((x, y), pressurized);
            }
            // Solid tiles are not inserted — they are walls
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Recalculate the pressure map when dirty. Only runs on ship worlds.
fn recalculate_pressure(
    mut pressure_map: Option<ResMut<PressureMap>>,
    world_map: Res<WorldMap>,
    active_world: Option<Res<ActiveWorld>>,
    tile_registry: Res<TileRegistry>,
) {
    let Some(ref mut pressure_map) = pressure_map else {
        return;
    };
    if !pressure_map.dirty {
        return;
    }
    let Some(active_world) = active_world else {
        return;
    };
    if !matches!(active_world.address, CelestialAddress::Ship { .. }) {
        return;
    }

    pressure_map.dirty = false;
    pressure_map.tiles = compute_pressure(
        &world_map,
        &tile_registry,
        active_world.width_tiles,
        active_world.height_tiles,
    );
}

/// Update InVacuum component on entities based on PressureMap.
fn update_in_vacuum(
    pressure_map: Option<Res<PressureMap>>,
    active_world: Option<Res<ActiveWorld>>,
    mut query: Query<(&Transform, &mut InVacuum)>,
) {
    let Some(ref pressure_map) = pressure_map else {
        // No pressure map = planet world, everything pressurized
        for (_, mut in_vacuum) in &mut query {
            in_vacuum.0 = false;
        }
        return;
    };
    let Some(ref active_world) = active_world else {
        return;
    };

    let ts = active_world.tile_size;
    for (transform, mut in_vacuum) in &mut query {
        let (tx, ty) = world_to_tile(transform.translation.x, transform.translation.y, ts);
        in_vacuum.0 = !pressure_map.is_pressurized(tx, ty);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::tile::{TileDef, TileRegistry};

    fn test_tile_registry() -> TileRegistry {
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
                id: "hull".into(),
                autotile: None,
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

    /// Build a small WorldMap with explicit tile layout for testing.
    /// `tiles` is row-major, height rows of width columns.
    /// `true` = solid, `false` = air.
    fn build_test_world(
        tiles: &[Vec<bool>],
        width: i32,
        height: i32,
        chunk_size: u32,
    ) -> WorldMap {
        use crate::world::chunk::{ChunkData, TileLayer};
        use crate::liquid::LiquidLayer;

        let mut world_map = WorldMap::default();

        // Determine which chunks we need
        let chunks_w = (width as u32 + chunk_size - 1) / chunk_size;
        let chunks_h = (height as u32 + chunk_size - 1) / chunk_size;

        for cy in 0..chunks_h as i32 {
            for cx in 0..chunks_w as i32 {
                let len = (chunk_size * chunk_size) as usize;
                let mut fg_tiles = vec![TileId::AIR; len];

                for ly in 0..chunk_size {
                    for lx in 0..chunk_size {
                        let tx = cx * chunk_size as i32 + lx as i32;
                        let ty = cy * chunk_size as i32 + ly as i32;
                        if tx < width && ty < height {
                            let is_solid = tiles[ty as usize][tx as usize];
                            fg_tiles[(ly * chunk_size + lx) as usize] =
                                if is_solid { TileId(1) } else { TileId::AIR };
                        }
                    }
                }

                let chunk = ChunkData {
                    fg: TileLayer {
                        tiles: fg_tiles,
                        bitmasks: vec![0; len],
                    },
                    bg: TileLayer::new_air(len),
                    liquid: LiquidLayer {
                        cells: vec![crate::liquid::data::LiquidCell::EMPTY; len],
                    },
                    objects: Vec::new(),
                    occupancy: vec![None; len],
                    damage: vec![0; len],
                };
                world_map.chunks.insert((cx, cy), chunk);
            }
        }

        world_map
    }

    #[test]
    fn sealed_room_is_pressurized() {
        // 8x8 world with a sealed 4x4 room in the center:
        //   All edges are air (vacuum)
        //   Walls at rows 2-5, columns 2-5
        //   Interior at rows 3-4, columns 3-4
        let width = 8;
        let height = 8;
        let mut tiles = vec![vec![false; width as usize]; height as usize];

        // Build walls (hollow box from (2,2) to (5,5))
        for x in 2..=5 {
            tiles[2][x] = true;
            tiles[5][x] = true;
        }
        for y in 2..=5 {
            tiles[y][2] = true;
            tiles[y][5] = true;
        }

        let tr = test_tile_registry();
        let world_map = build_test_world(&tiles, width, height, 32);
        let result = compute_pressure(&world_map, &tr, width, height);

        // Interior tiles (3,3), (3,4), (4,3), (4,4) should be pressurized
        assert!(result.get(&(3, 3)).copied().unwrap_or(false), "Interior (3,3) should be pressurized");
        assert!(result.get(&(4, 3)).copied().unwrap_or(false), "Interior (4,3) should be pressurized");
        assert!(result.get(&(3, 4)).copied().unwrap_or(false), "Interior (3,4) should be pressurized");
        assert!(result.get(&(4, 4)).copied().unwrap_or(false), "Interior (4,4) should be pressurized");

        // Edge tile (0,0) should NOT be pressurized (vacuum)
        assert!(!result.get(&(0, 0)).copied().unwrap_or(true), "Edge (0,0) should be vacuum");
        // Tile outside the walls should be vacuum
        assert!(!result.get(&(1, 1)).copied().unwrap_or(true), "Outside (1,1) should be vacuum");
    }

    #[test]
    fn open_room_is_vacuum() {
        // Same as above but with a gap in the wall
        let width = 8;
        let height = 8;
        let mut tiles = vec![vec![false; width as usize]; height as usize];

        // Build walls (hollow box from (2,2) to (5,5))
        for x in 2..=5 {
            tiles[2][x] = true;
            tiles[5][x] = true;
        }
        for y in 2..=5 {
            tiles[y][2] = true;
            tiles[y][5] = true;
        }
        // Open a gap in the top wall
        tiles[2][3] = false;

        let tr = test_tile_registry();
        let world_map = build_test_world(&tiles, width, height, 32);
        let result = compute_pressure(&world_map, &tr, width, height);

        // Interior tiles should NOT be pressurized (air reaches through gap)
        assert!(!result.get(&(3, 3)).copied().unwrap_or(true), "Interior (3,3) should be vacuum");
        assert!(!result.get(&(4, 3)).copied().unwrap_or(true), "Interior (4,3) should be vacuum");
        assert!(!result.get(&(3, 4)).copied().unwrap_or(true), "Interior (3,4) should be vacuum");
        assert!(!result.get(&(4, 4)).copied().unwrap_or(true), "Interior (4,4) should be vacuum");
    }

    #[test]
    fn single_tile_gap_depressurizes() {
        // Start with sealed room, then remove one wall tile
        let width = 8;
        let height = 8;
        let mut tiles = vec![vec![false; width as usize]; height as usize];

        // Build sealed walls
        for x in 2..=5 {
            tiles[2][x] = true;
            tiles[5][x] = true;
        }
        for y in 2..=5 {
            tiles[y][2] = true;
            tiles[y][5] = true;
        }

        let tr = test_tile_registry();

        // First: sealed room is pressurized
        let world_map = build_test_world(&tiles, width, height, 32);
        let result = compute_pressure(&world_map, &tr, width, height);
        assert!(result.get(&(3, 3)).copied().unwrap_or(false), "Sealed interior should be pressurized");

        // Now: remove one wall tile
        tiles[2][4] = false;
        let world_map = build_test_world(&tiles, width, height, 32);
        let result = compute_pressure(&world_map, &tr, width, height);
        assert!(!result.get(&(3, 3)).copied().unwrap_or(true), "After gap, interior should be vacuum");
    }

    #[test]
    fn entirely_open_world_is_vacuum() {
        // All air, no walls
        let width = 8;
        let height = 8;
        let tiles = vec![vec![false; width as usize]; height as usize];

        let tr = test_tile_registry();
        let world_map = build_test_world(&tiles, width, height, 32);
        let result = compute_pressure(&world_map, &tr, width, height);

        // Everything should be vacuum
        for y in 0..height {
            for x in 0..width {
                assert!(
                    !result.get(&(x, y)).copied().unwrap_or(true),
                    "({},{}) should be vacuum in open world",
                    x, y
                );
            }
        }
    }

    #[test]
    fn solid_tiles_not_in_pressure_map() {
        let width = 4;
        let height = 4;
        let mut tiles = vec![vec![false; width as usize]; height as usize];
        tiles[1][1] = true; // single solid tile

        let tr = test_tile_registry();
        let world_map = build_test_world(&tiles, width, height, 32);
        let result = compute_pressure(&world_map, &tr, width, height);

        // Solid tile should not be in the map at all
        assert!(!result.contains_key(&(1, 1)), "Solid tile should not be in pressure map");
    }
}
