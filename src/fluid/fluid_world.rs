use std::collections::HashMap;

use crate::fluid::cell::{FluidCell, FluidId, FluidSlot};
use crate::fluid::registry::FluidRegistry;
use crate::registry::tile::{TileId, TileRegistry};
use crate::world::chunk::WorldMap;

/// Virtual global grid over the chunk-based `WorldMap`.
///
/// Converts `(global_x, global_y)` coordinates to `(chunk_cx, chunk_cy, local_x, local_y)`
/// with horizontal wrapping. Holds a snapshot of all active chunks' fluids at construction
/// time so that simulation reads are consistent (double-buffer pattern).
pub struct FluidWorld<'a> {
    pub world_map: &'a mut WorldMap,
    pub snapshots: HashMap<(i32, i32), Vec<FluidCell>>,
    pub chunk_size: u32,
    pub width_chunks: i32,
    pub height_chunks: i32,
    pub tile_registry: &'a TileRegistry,
    pub fluid_registry: &'a FluidRegistry,
}

impl<'a> FluidWorld<'a> {
    /// Create a new FluidWorld, snapshotting all active chunks' fluids for consistent reads.
    pub fn new(
        world_map: &'a mut WorldMap,
        chunk_size: u32,
        width_chunks: i32,
        height_chunks: i32,
        tile_registry: &'a TileRegistry,
        fluid_registry: &'a FluidRegistry,
    ) -> Self {
        let snapshots: HashMap<(i32, i32), Vec<FluidCell>> = world_map
            .chunks
            .iter()
            .map(|(&key, chunk)| (key, chunk.fluids.clone()))
            .collect();

        Self {
            world_map,
            snapshots,
            chunk_size,
            width_chunks,
            height_chunks,
            tile_registry,
            fluid_registry,
        }
    }

    /// Convert global tile coordinates to (chunk_x, chunk_y, local_x, local_y).
    ///
    /// Applies horizontal wrapping via `rem_euclid`. Returns `None` if the
    /// position is out of vertical bounds (gy < 0 or gy >= height_chunks * chunk_size).
    pub fn resolve(&self, gx: i32, gy: i32) -> Option<(i32, i32, u32, u32)> {
        let cs = self.chunk_size as i32;
        let total_height = self.height_chunks * cs;

        if gy < 0 || gy >= total_height {
            return None;
        }

        let cx = gx.div_euclid(cs).rem_euclid(self.width_chunks);
        let cy = gy.div_euclid(cs);
        let lx = gx.rem_euclid(cs) as u32;
        let ly = gy.rem_euclid(cs) as u32;

        Some((cx, cy, lx, ly))
    }

    /// Read fluid from the snapshot (old state at tick start) for consistent simulation reads.
    /// Returns `FluidCell::EMPTY` if the chunk is not in snapshots or position is out of bounds.
    pub fn read(&self, gx: i32, gy: i32) -> FluidCell {
        let Some((cx, cy, lx, ly)) = self.resolve(gx, gy) else {
            return FluidCell::EMPTY;
        };
        self.snapshots
            .get(&(cx, cy))
            .map(|fluids| {
                let idx = (ly * self.chunk_size + lx) as usize;
                fluids[idx]
            })
            .unwrap_or(FluidCell::EMPTY)
    }

    /// Read current (potentially modified) fluid state from the live chunk data.
    /// Returns `FluidCell::EMPTY` if the chunk is not found or position is out of bounds.
    pub fn read_current(&self, gx: i32, gy: i32) -> FluidCell {
        let Some((cx, cy, lx, ly)) = self.resolve(gx, gy) else {
            return FluidCell::EMPTY;
        };
        self.world_map
            .chunk(cx, cy)
            .map(|chunk| {
                let idx = (ly * self.chunk_size + lx) as usize;
                chunk.fluids[idx]
            })
            .unwrap_or(FluidCell::EMPTY)
    }

    /// Write a fluid cell directly to the live chunk data.
    pub fn write(&mut self, gx: i32, gy: i32, cell: FluidCell) {
        let Some((cx, cy, lx, ly)) = self.resolve(gx, gy) else {
            return;
        };
        if let Some(chunk) = self.world_map.chunks.get_mut(&(cx, cy)) {
            let idx = (ly * self.chunk_size + lx) as usize;
            chunk.fluids[idx] = cell;
        }
    }

    /// Add mass to a cell's matching slot, or create a new slot if possible.
    /// Returns the amount actually added (0.0 if both slots are occupied by other fluids).
    pub fn add_mass(&mut self, gx: i32, gy: i32, fluid_id: FluidId, amount: f32) -> f32 {
        let Some((cx, cy, lx, ly)) = self.resolve(gx, gy) else {
            return 0.0;
        };
        let Some(chunk) = self.world_map.chunks.get_mut(&(cx, cy)) else {
            return 0.0;
        };
        let idx = (ly * self.chunk_size + lx) as usize;
        let cell = &mut chunk.fluids[idx];

        // 1. Primary already has this fluid
        if cell.primary.fluid_id == fluid_id && !cell.primary.is_empty() {
            cell.primary.mass += amount;
            return amount;
        }
        // 2. Secondary already has this fluid
        if cell.secondary.fluid_id == fluid_id && !cell.secondary.is_empty() {
            cell.secondary.mass += amount;
            return amount;
        }
        // 3. Cell is completely empty → create primary
        if cell.is_empty() {
            cell.primary = FluidSlot::new(fluid_id, amount);
            return amount;
        }
        // 4. Secondary slot is empty → create secondary
        if cell.secondary.is_empty() {
            cell.secondary = FluidSlot::new(fluid_id, amount);
            return amount;
        }
        // 5. Both slots occupied by other fluids
        0.0
    }

    /// Subtract mass from the slot matching `fluid_id`. If mass drops to zero or below,
    /// clears that slot and normalizes the cell.
    pub fn sub_mass(&mut self, gx: i32, gy: i32, fluid_id: FluidId, amount: f32) {
        let Some((cx, cy, lx, ly)) = self.resolve(gx, gy) else {
            return;
        };
        if let Some(chunk) = self.world_map.chunks.get_mut(&(cx, cy)) {
            let idx = (ly * self.chunk_size + lx) as usize;
            let cell = &mut chunk.fluids[idx];
            if let Some(slot) = cell.slot_for_mut(fluid_id) {
                slot.mass -= amount;
                if slot.mass <= 0.0 {
                    *slot = FluidSlot::EMPTY;
                }
            }
            cell.normalize();
        }
    }

    /// Check if the foreground tile at the given position is solid.
    /// Returns `true` for out-of-bounds positions (acts as a wall).
    pub fn is_solid(&self, gx: i32, gy: i32) -> bool {
        let Some((cx, cy, lx, ly)) = self.resolve(gx, gy) else {
            return true;
        };
        self.world_map
            .chunk(cx, cy)
            .map(|chunk| {
                let tile = chunk.fg.get(lx, ly, self.chunk_size);
                self.tile_registry.is_solid(tile)
            })
            .unwrap_or(true)
    }

    /// Get the foreground tile at the given position.
    /// Returns `TileId::AIR` for out-of-bounds positions.
    pub fn tile_at(&self, gx: i32, gy: i32) -> TileId {
        let Some((cx, cy, lx, ly)) = self.resolve(gx, gy) else {
            return TileId::AIR;
        };
        self.world_map
            .chunk(cx, cy)
            .map(|chunk| chunk.fg.get(lx, ly, self.chunk_size))
            .unwrap_or(TileId::AIR)
    }

    /// Set the foreground tile at the given position.
    pub fn set_tile(&mut self, gx: i32, gy: i32, tile: TileId) {
        let Some((cx, cy, lx, ly)) = self.resolve(gx, gy) else {
            return;
        };
        if let Some(chunk) = self.world_map.chunks.get_mut(&(cx, cy)) {
            chunk.fg.set(lx, ly, tile, self.chunk_size);
        }
    }

    /// Swap fluid cells at two global positions (used for density displacement).
    pub fn swap_fluids(&mut self, a: (i32, i32), b: (i32, i32)) {
        let cell_a = self.read_current(a.0, a.1);
        let cell_b = self.read_current(b.0, b.1);
        self.write(a.0, a.1, cell_b);
        self.write(b.0, b.1, cell_a);
    }

    /// Check if a chunk exists in the world map.
    pub fn has_chunk(&self, cx: i32, cy: i32) -> bool {
        self.world_map.chunks.contains_key(&(cx, cy))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::registry::{FluidDef, FluidRegistry};
    use crate::registry::tile::TileRegistry;
    use crate::world::chunk::{ChunkData, TileLayer, WorldMap};

    fn make_chunk(cs: u32) -> ChunkData {
        let len = (cs * cs) as usize;
        ChunkData {
            fg: TileLayer::new_air(len),
            bg: TileLayer::new_air(len),
            fluids: vec![FluidCell::EMPTY; len],
            objects: Vec::new(),
            occupancy: vec![None; len],
            damage: vec![0; len],
        }
    }

    fn test_tile_registry() -> TileRegistry {
        crate::test_helpers::fixtures::test_tile_registry()
    }

    fn test_fluid_registry() -> FluidRegistry {
        FluidRegistry::from_defs(vec![FluidDef {
            id: "water".to_string(),
            density: 1000.0,
            viscosity: 0.0,
            max_compress: 0.02,
            is_gas: false,
            color: [64, 128, 255, 180],
            damage_on_contact: 0.0,
            light_emission: [0, 0, 0],
            effects: vec![],
            wave_amplitude: 1.0,
            wave_speed: 1.0,
            light_absorption: 0.3,
        }])
    }

    #[test]
    fn test_resolve_basic() {
        let mut world_map = WorldMap::default();
        let cs = 4u32;
        world_map.chunks.insert((0, 0), make_chunk(cs));

        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let fw = FluidWorld::new(&mut world_map, cs, 4, 4, &tr, &fr);

        assert_eq!(fw.resolve(0, 0), Some((0, 0, 0, 0)));
    }

    #[test]
    fn test_resolve_wrapping() {
        let mut world_map = WorldMap::default();
        let cs = 4u32;
        // width_chunks=2, so world is 8 tiles wide
        world_map.chunks.insert((0, 0), make_chunk(cs));
        world_map.chunks.insert((1, 0), make_chunk(cs));

        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let fw = FluidWorld::new(&mut world_map, cs, 2, 4, &tr, &fr);

        // gx = -1 should wrap: div_euclid(-1, 4) = -1, rem_euclid(-1, 2) = 1
        // lx = rem_euclid(-1, 4) = 3
        let result = fw.resolve(-1, 0);
        assert_eq!(result, Some((1, 0, 3, 0)));
    }

    #[test]
    fn test_resolve_out_of_vertical_bounds() {
        let mut world_map = WorldMap::default();
        let cs = 4u32;
        world_map.chunks.insert((0, 0), make_chunk(cs));

        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let fw = FluidWorld::new(&mut world_map, cs, 2, 4, &tr, &fr);

        // gy = -1 is below bottom
        assert_eq!(fw.resolve(0, -1), None);
        // gy = 16 (4 chunks * 4 tiles) is above top
        assert_eq!(fw.resolve(0, 16), None);
    }

    #[test]
    fn test_read_write_roundtrip() {
        let mut world_map = WorldMap::default();
        let cs = 4u32;
        world_map.chunks.insert((0, 0), make_chunk(cs));

        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let mut fw = FluidWorld::new(&mut world_map, cs, 2, 4, &tr, &fr);

        let water = FluidCell::new(FluidId(1), 0.75);
        fw.write(1, 2, water);

        let read_back = fw.read_current(1, 2);
        assert_eq!(read_back.fluid_id(), FluidId(1));
        assert!((read_back.mass() - 0.75).abs() < f32::EPSILON);
    }

    #[test]
    fn test_read_snapshot_vs_current() {
        let mut world_map = WorldMap::default();
        let cs = 4u32;
        world_map.chunks.insert((0, 0), make_chunk(cs));

        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let mut fw = FluidWorld::new(&mut world_map, cs, 2, 4, &tr, &fr);

        // Snapshot was taken at construction — all cells are EMPTY
        assert!(fw.read(1, 2).is_empty());

        // Write a new value to the live data
        let water = FluidCell::new(FluidId(1), 0.5);
        fw.write(1, 2, water);

        // Snapshot still returns old (empty) state
        assert!(fw.read(1, 2).is_empty());

        // Current returns the new state
        let current = fw.read_current(1, 2);
        assert_eq!(current.fluid_id(), FluidId(1));
        assert!((current.mass() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_is_solid_out_of_bounds() {
        let mut world_map = WorldMap::default();
        let cs = 4u32;
        world_map.chunks.insert((0, 0), make_chunk(cs));

        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let fw = FluidWorld::new(&mut world_map, cs, 2, 4, &tr, &fr);

        // Out of vertical bounds should return true (solid wall)
        assert!(fw.is_solid(0, -1));
        assert!(fw.is_solid(0, 16));

        // In-bounds air tile should not be solid
        assert!(!fw.is_solid(0, 0));
    }

    #[test]
    fn test_swap_fluids() {
        let mut world_map = WorldMap::default();
        let cs = 4u32;
        world_map.chunks.insert((0, 0), make_chunk(cs));

        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let mut fw = FluidWorld::new(&mut world_map, cs, 2, 4, &tr, &fr);

        let water = FluidCell::new(FluidId(1), 0.8);
        fw.write(0, 0, water);
        // (1, 1) is EMPTY

        fw.swap_fluids((0, 0), (1, 1));

        // (0, 0) should now be empty
        let a = fw.read_current(0, 0);
        assert!(a.is_empty());

        // (1, 1) should now have water
        let b = fw.read_current(1, 1);
        assert_eq!(b.fluid_id(), FluidId(1));
        assert!((b.mass() - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn test_has_chunk() {
        let mut world_map = WorldMap::default();
        let cs = 4u32;
        world_map.chunks.insert((0, 0), make_chunk(cs));

        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let fw = FluidWorld::new(&mut world_map, cs, 2, 4, &tr, &fr);

        assert!(fw.has_chunk(0, 0));
        assert!(!fw.has_chunk(1, 1));
    }

    #[test]
    fn test_add_mass_to_empty_cell() {
        let mut world_map = WorldMap::default();
        let cs = 4u32;
        world_map.chunks.insert((0, 0), make_chunk(cs));

        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let mut fw = FluidWorld::new(&mut world_map, cs, 2, 4, &tr, &fr);

        let added = fw.add_mass(1, 1, FluidId(1), 0.5);
        assert!((added - 0.5).abs() < f32::EPSILON);
        let cell = fw.read_current(1, 1);
        assert_eq!(cell.fluid_id(), FluidId(1));
        assert!((cell.mass() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_add_mass_to_existing_cell() {
        let mut world_map = WorldMap::default();
        let cs = 4u32;
        world_map.chunks.insert((0, 0), make_chunk(cs));

        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let mut fw = FluidWorld::new(&mut world_map, cs, 2, 4, &tr, &fr);

        fw.write(1, 1, FluidCell::new(FluidId(1), 0.3));
        let added = fw.add_mass(1, 1, FluidId(1), 0.4);
        assert!((added - 0.4).abs() < f32::EPSILON);

        let cell = fw.read_current(1, 1);
        assert!((cell.mass() - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn test_sub_mass_clears_cell() {
        let mut world_map = WorldMap::default();
        let cs = 4u32;
        world_map.chunks.insert((0, 0), make_chunk(cs));

        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let mut fw = FluidWorld::new(&mut world_map, cs, 2, 4, &tr, &fr);

        fw.write(1, 1, FluidCell::new(FluidId(1), 0.5));
        fw.sub_mass(1, 1, FluidId(1), 0.5);

        let cell = fw.read_current(1, 1);
        assert!(cell.is_empty());
    }

    #[test]
    fn test_set_tile_and_tile_at() {
        let mut world_map = WorldMap::default();
        let cs = 4u32;
        world_map.chunks.insert((0, 0), make_chunk(cs));

        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let mut fw = FluidWorld::new(&mut world_map, cs, 2, 4, &tr, &fr);

        // Initially air
        assert_eq!(fw.tile_at(1, 1), TileId::AIR);

        // Set to stone (TileId(3) in test registry)
        fw.set_tile(1, 1, TileId(3));
        assert_eq!(fw.tile_at(1, 1), TileId(3));
        assert!(fw.is_solid(1, 1));
    }

    #[test]
    fn test_tile_at_out_of_bounds() {
        let mut world_map = WorldMap::default();
        let cs = 4u32;
        world_map.chunks.insert((0, 0), make_chunk(cs));

        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let fw = FluidWorld::new(&mut world_map, cs, 2, 4, &tr, &fr);

        assert_eq!(fw.tile_at(0, -1), TileId::AIR);
        assert_eq!(fw.tile_at(0, 16), TileId::AIR);
    }

    #[test]
    fn test_read_missing_chunk_returns_empty() {
        let mut world_map = WorldMap::default();
        let cs = 4u32;
        // No chunks inserted

        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let fw = FluidWorld::new(&mut world_map, cs, 2, 4, &tr, &fr);

        assert!(fw.read(0, 0).is_empty());
        assert!(fw.read_current(0, 0).is_empty());
    }
}
