# Object Layer Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a placeable objects layer (furniture, containers, light sources) to the world, with multi-tile support, per-object collision, and placement rules.

**Architecture:** Hybrid storage — data in `ChunkData` (Vec<PlacedObject> + occupancy grid), entities spawned at runtime for rendering/interaction. Objects are grid-snapped, multi-tile (anchor = bottom-left), with configurable solid_mask and PlacementRule.

**Tech Stack:** Bevy 0.18 (Rust), RON for data definitions, existing custom chunk/mesh rendering pipeline.

---

### Task 1: ObjectDef and ObjectRegistry

**Files:**
- Create: `src/object/definition.rs`
- Create: `src/object/registry.rs`
- Create: `src/object/mod.rs`
- Modify: `src/main.rs:1` (add `pub mod object;`)

**Step 1: Create `src/object/definition.rs` with types**

```rust
use bevy::prelude::*;
use serde::Deserialize;

use crate::item::DropDef;

/// Compact object identifier. Index into ObjectRegistry.defs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ObjectId(pub u16);

impl ObjectId {
    pub const NONE: ObjectId = ObjectId(0);
}

#[derive(Debug, Clone, Deserialize)]
pub enum PlacementRule {
    Floor,
    Wall,
    Ceiling,
    Any,
}

#[derive(Debug, Clone, Deserialize)]
pub enum ObjectType {
    Decoration,
    Container { slots: u16 },
    LightSource,
}

fn default_solid_mask() -> Vec<bool> {
    vec![true]
}

fn default_light_emission() -> [u8; 3] {
    [0, 0, 0]
}

#[derive(Debug, Clone, Deserialize)]
pub struct ObjectDef {
    pub id: String,
    pub display_name: String,
    pub size: (u32, u32),              // (width, height) in tiles
    pub sprite: String,
    #[serde(default = "default_solid_mask")]
    pub solid_mask: Vec<bool>,         // len = size.0 * size.1, row-major bottom-up
    pub placement: PlacementRule,
    #[serde(default = "default_light_emission")]
    pub light_emission: [u8; 3],
    pub object_type: ObjectType,
    #[serde(default)]
    pub drops: Vec<DropDef>,
}

impl ObjectDef {
    /// Check if a specific local tile within this object is solid.
    /// `rel_x` and `rel_y` are relative to the anchor (bottom-left).
    pub fn is_tile_solid(&self, rel_x: u32, rel_y: u32) -> bool {
        let idx = (rel_y * self.size.0 + rel_x) as usize;
        self.solid_mask.get(idx).copied().unwrap_or(false)
    }
}
```

**Step 2: Create `src/object/registry.rs`**

```rust
use std::collections::HashMap;

use bevy::prelude::*;

use super::definition::{ObjectDef, ObjectId};

#[derive(Resource, Debug)]
pub struct ObjectRegistry {
    defs: Vec<ObjectDef>,
    name_to_id: HashMap<String, ObjectId>,
}

impl ObjectRegistry {
    pub fn from_defs(defs: Vec<ObjectDef>) -> Self {
        let name_to_id = defs
            .iter()
            .enumerate()
            .map(|(i, d)| (d.id.clone(), ObjectId(i as u16)))
            .collect();
        Self { defs, name_to_id }
    }

    pub fn get(&self, id: ObjectId) -> &ObjectDef {
        &self.defs[id.0 as usize]
    }

    pub fn by_name(&self, name: &str) -> Option<ObjectId> {
        self.name_to_id.get(name).copied()
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}
```

**Step 3: Create `src/object/mod.rs`**

```rust
pub mod definition;
pub mod registry;

pub use definition::*;
pub use registry::*;
```

**Step 4: Add `pub mod object;` to `src/main.rs`**

Add after `pub mod math;` line:
```rust
pub mod object;
```

**Step 5: Write tests in `src/object/definition.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_id_none_is_zero() {
        assert_eq!(ObjectId::NONE, ObjectId(0));
    }

    #[test]
    fn is_tile_solid_1x1() {
        let def = ObjectDef {
            id: "barrel".into(),
            display_name: "Barrel".into(),
            size: (1, 1),
            sprite: "objects/barrel.png".into(),
            solid_mask: vec![true],
            placement: PlacementRule::Floor,
            light_emission: [0, 0, 0],
            object_type: ObjectType::Decoration,
            drops: vec![],
        };
        assert!(def.is_tile_solid(0, 0));
    }

    #[test]
    fn is_tile_solid_multi_tile() {
        // Table 3x2: legs solid, top passable
        // Row 0 (bottom): [true, false, true]
        // Row 1 (top):    [false, false, false]
        let def = ObjectDef {
            id: "table".into(),
            display_name: "Table".into(),
            size: (3, 2),
            sprite: "objects/table.png".into(),
            solid_mask: vec![true, false, true, false, false, false],
            placement: PlacementRule::Floor,
            light_emission: [0, 0, 0],
            object_type: ObjectType::Decoration,
            drops: vec![],
        };
        assert!(def.is_tile_solid(0, 0));   // left leg
        assert!(!def.is_tile_solid(1, 0));  // gap between legs
        assert!(def.is_tile_solid(2, 0));   // right leg
        assert!(!def.is_tile_solid(0, 1));  // top row passable
        assert!(!def.is_tile_solid(1, 1));
        assert!(!def.is_tile_solid(2, 1));
    }

    #[test]
    fn is_tile_solid_out_of_bounds_returns_false() {
        let def = ObjectDef {
            id: "torch".into(),
            display_name: "Torch".into(),
            size: (1, 1),
            sprite: "objects/torch.png".into(),
            solid_mask: vec![false],
            placement: PlacementRule::Wall,
            light_emission: [240, 180, 80],
            object_type: ObjectType::LightSource,
            drops: vec![],
        };
        assert!(!def.is_tile_solid(5, 5));
    }
}
```

**Step 6: Write tests in `src/object/registry.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{ObjectType, PlacementRule};

    fn test_registry() -> ObjectRegistry {
        ObjectRegistry::from_defs(vec![
            ObjectDef {
                id: "torch".into(),
                display_name: "Torch".into(),
                size: (1, 1),
                sprite: "objects/torch.png".into(),
                solid_mask: vec![false],
                placement: PlacementRule::Wall,
                light_emission: [240, 180, 80],
                object_type: ObjectType::LightSource,
                drops: vec![],
            },
            ObjectDef {
                id: "chest".into(),
                display_name: "Wooden Chest".into(),
                size: (2, 1),
                sprite: "objects/chest.png".into(),
                solid_mask: vec![true, true],
                placement: PlacementRule::Floor,
                light_emission: [0, 0, 0],
                object_type: ObjectType::Container { slots: 16 },
                drops: vec![],
            },
        ])
    }

    #[test]
    fn lookup_by_name() {
        let reg = test_registry();
        assert_eq!(reg.by_name("torch"), Some(ObjectId(0)));
        assert_eq!(reg.by_name("chest"), Some(ObjectId(1)));
    }

    #[test]
    fn by_name_returns_none_for_unknown() {
        let reg = test_registry();
        assert_eq!(reg.by_name("nonexistent"), None);
    }

    #[test]
    fn get_returns_def() {
        let reg = test_registry();
        let torch = reg.get(ObjectId(0));
        assert_eq!(torch.id, "torch");
        assert_eq!(torch.size, (1, 1));
    }

    #[test]
    fn registry_len() {
        let reg = test_registry();
        assert_eq!(reg.len(), 2);
        assert!(!reg.is_empty());
    }
}
```

**Step 7: Run tests**

Run: `cargo test --lib object`
Expected: All tests PASS

**Step 8: Commit**

```bash
git add src/object/ src/main.rs
git commit -m "feat(object): add ObjectDef, ObjectRegistry with multi-tile support"
```

---

### Task 2: ChunkData Extension — PlacedObject and Occupancy Grid

**Files:**
- Create: `src/object/placed.rs`
- Modify: `src/world/chunk.rs` — add `objects` and `occupancy` to `ChunkData`
- Modify: `src/object/mod.rs` — add `pub mod placed;`

**Step 1: Create `src/object/placed.rs`**

```rust
use crate::inventory::InventorySlot;

use super::definition::ObjectId;

/// Reference from an occupancy grid cell to the object that occupies it.
#[derive(Debug, Clone, Copy)]
pub struct OccupancyRef {
    pub object_index: u16,
    pub is_anchor: bool,
}

/// State of a placed object (varies by ObjectType).
#[derive(Debug, Clone, Default)]
pub enum ObjectState {
    #[default]
    Default,
    Container {
        contents: Vec<Option<InventorySlot>>,
    },
}

/// A single object placed in a chunk, stored in ChunkData.
#[derive(Debug, Clone)]
pub struct PlacedObject {
    pub object_id: ObjectId,
    pub local_x: u32,
    pub local_y: u32,
    pub state: ObjectState,
}
```

**Step 2: Add `pub mod placed;` and re-export to `src/object/mod.rs`**

```rust
pub mod definition;
pub mod placed;
pub mod registry;

pub use definition::*;
pub use placed::*;
pub use registry::*;
```

**Step 3: Modify `src/world/chunk.rs` — extend `ChunkData`**

Add import at top:
```rust
use crate::object::placed::{OccupancyRef, PlacedObject};
```

Modify `ChunkData` struct:
```rust
pub struct ChunkData {
    pub fg: TileLayer,
    pub bg: TileLayer,
    pub objects: Vec<PlacedObject>,
    pub occupancy: Vec<Option<OccupancyRef>>,
    #[allow(dead_code)]
    pub damage: Vec<u8>,
}
```

Modify `get_or_generate_chunk` to initialize empty objects/occupancy:
```rust
ChunkData {
    fg: TileLayer { tiles: chunk_tiles.fg, bitmasks: vec![0; len] },
    bg: TileLayer { tiles: chunk_tiles.bg, bitmasks: vec![0; len] },
    objects: Vec::new(),
    occupancy: vec![None; len],
    damage: vec![0; len],
}
```

**Step 4: Write tests for `src/object/placed.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placed_object_stores_position() {
        let obj = PlacedObject {
            object_id: ObjectId(1),
            local_x: 5,
            local_y: 10,
            state: ObjectState::Default,
        };
        assert_eq!(obj.local_x, 5);
        assert_eq!(obj.local_y, 10);
    }

    #[test]
    fn object_state_default() {
        let state = ObjectState::Default;
        assert!(matches!(state, ObjectState::Default));
    }

    #[test]
    fn object_state_container() {
        let state = ObjectState::Container {
            contents: vec![None; 16],
        };
        match state {
            ObjectState::Container { contents } => assert_eq!(contents.len(), 16),
            _ => panic!("expected Container"),
        }
    }

    #[test]
    fn occupancy_ref_tracks_anchor() {
        let occ = OccupancyRef {
            object_index: 0,
            is_anchor: true,
        };
        assert!(occ.is_anchor);
        assert_eq!(occ.object_index, 0);
    }
}
```

**Step 5: Run tests**

Run: `cargo test --lib`
Expected: All existing tests PASS + new tests PASS

**Step 6: Commit**

```bash
git add src/object/placed.rs src/object/mod.rs src/world/chunk.rs
git commit -m "feat(object): add PlacedObject, occupancy grid, extend ChunkData"
```

---

### Task 3: Placement Logic

**Files:**
- Create: `src/object/placement.rs`
- Modify: `src/object/mod.rs` — add module

**Step 1: Create `src/object/placement.rs`**

```rust
use crate::object::definition::{ObjectDef, ObjectId, ObjectType, PlacementRule};
use crate::object::placed::{ObjectState, OccupancyRef, PlacedObject};
use crate::object::registry::ObjectRegistry;
use crate::registry::tile::TileId;
use crate::world::chunk::{tile_to_chunk, tile_to_local, Layer, WorldMap};
use crate::world::ctx::WorldCtxRef;

/// Check if an object can be placed at the given world tile coordinates (anchor = bottom-left).
pub fn can_place_object(
    world_map: &WorldMap,
    object_registry: &ObjectRegistry,
    object_id: ObjectId,
    anchor_x: i32,
    anchor_y: i32,
    ctx: &WorldCtxRef,
) -> bool {
    let def = object_registry.get(object_id);
    let w = def.size.0 as i32;
    let h = def.size.1 as i32;

    // 1. All tiles in the area must be air (fg) and unoccupied
    for dy in 0..h {
        for dx in 0..w {
            let tx = anchor_x + dx;
            let ty = anchor_y + dy;

            // Check fg tile is air
            match world_map.get_tile(tx, ty, Layer::Fg, ctx) {
                Some(tile) if tile != TileId::AIR => return false,
                None => return false, // chunk not loaded
                _ => {}
            }

            // Check occupancy is free
            let wrapped_x = ctx.config.wrap_tile_x(tx);
            let (cx, cy) = tile_to_chunk(wrapped_x, ty, ctx.config.chunk_size);
            let (lx, ly) = tile_to_local(wrapped_x, ty, ctx.config.chunk_size);
            if let Some(chunk) = world_map.chunk(cx, cy) {
                let idx = (ly * ctx.config.chunk_size + lx) as usize;
                if chunk.occupancy[idx].is_some() {
                    return false;
                }
            }
        }
    }

    // 2. Placement rule check
    match def.placement {
        PlacementRule::Floor => {
            for dx in 0..w {
                if !world_map.is_solid(anchor_x + dx, anchor_y - 1, ctx) {
                    return false;
                }
            }
        }
        PlacementRule::Wall => {
            let left_solid = world_map.is_solid(anchor_x - 1, anchor_y, ctx);
            let right_solid = world_map.is_solid(anchor_x + w, anchor_y, ctx);
            if !left_solid && !right_solid {
                return false;
            }
        }
        PlacementRule::Ceiling => {
            for dx in 0..w {
                if !world_map.is_solid(anchor_x + dx, anchor_y + h, ctx) {
                    return false;
                }
            }
        }
        PlacementRule::Any => {}
    }

    true
}

/// Place an object into the world map. Returns true on success.
/// Caller must verify `can_place_object` before calling.
pub fn place_object(
    world_map: &mut WorldMap,
    object_registry: &ObjectRegistry,
    object_id: ObjectId,
    anchor_x: i32,
    anchor_y: i32,
    ctx: &WorldCtxRef,
) -> bool {
    let def = object_registry.get(object_id);
    let w = def.size.0;
    let h = def.size.1;

    // Determine anchor chunk
    let wrapped_anchor_x = ctx.config.wrap_tile_x(anchor_x);
    let (anchor_cx, anchor_cy) = tile_to_chunk(wrapped_anchor_x, anchor_y, ctx.config.chunk_size);
    let (anchor_lx, anchor_ly) =
        tile_to_local(wrapped_anchor_x, anchor_y, ctx.config.chunk_size);

    // Create placed object
    let state = match &def.object_type {
        ObjectType::Container { slots } => ObjectState::Container {
            contents: vec![None; *slots as usize],
        },
        _ => ObjectState::Default,
    };

    // Add to anchor chunk
    let chunk = match world_map.chunks.get_mut(&(anchor_cx, anchor_cy)) {
        Some(c) => c,
        None => return false,
    };
    let object_index = chunk.objects.len() as u16;
    chunk.objects.push(PlacedObject {
        object_id,
        local_x: anchor_lx,
        local_y: anchor_ly,
        state,
    });

    // Write occupancy for all tiles
    for dy in 0..h {
        for dx in 0..w {
            let tx = anchor_x + dx as i32;
            let ty = anchor_y + dy as i32;
            let wrapped_x = ctx.config.wrap_tile_x(tx);
            let (cx, cy) = tile_to_chunk(wrapped_x, ty, ctx.config.chunk_size);
            let (lx, ly) = tile_to_local(wrapped_x, ty, ctx.config.chunk_size);
            let idx = (ly * ctx.config.chunk_size + lx) as usize;

            if let Some(c) = world_map.chunks.get_mut(&(cx, cy)) {
                c.occupancy[idx] = Some(OccupancyRef {
                    object_index,
                    is_anchor: dx == 0 && dy == 0,
                });
            }
        }
    }

    true
}

/// Remove an object from the world map by its anchor chunk coords and object index.
/// Returns the removed PlacedObject if successful.
pub fn remove_object(
    world_map: &mut WorldMap,
    object_registry: &ObjectRegistry,
    anchor_x: i32,
    anchor_y: i32,
    object_index: u16,
    ctx: &WorldCtxRef,
) -> Option<PlacedObject> {
    let wrapped_anchor_x = ctx.config.wrap_tile_x(anchor_x);
    let (anchor_cx, anchor_cy) = tile_to_chunk(wrapped_anchor_x, anchor_y, ctx.config.chunk_size);

    // Get the object info before removing
    let obj = world_map
        .chunks
        .get(&(anchor_cx, anchor_cy))?
        .objects
        .get(object_index as usize)?
        .clone();

    let def = object_registry.get(obj.object_id);
    let w = def.size.0;
    let h = def.size.1;

    // Clear occupancy for all tiles
    for dy in 0..h {
        for dx in 0..w {
            let tx = anchor_x + dx as i32;
            let ty = anchor_y + dy as i32;
            let wrapped_x = ctx.config.wrap_tile_x(tx);
            let (cx, cy) = tile_to_chunk(wrapped_x, ty, ctx.config.chunk_size);
            let (lx, ly) = tile_to_local(wrapped_x, ty, ctx.config.chunk_size);
            let idx = (ly * ctx.config.chunk_size + lx) as usize;

            if let Some(c) = world_map.chunks.get_mut(&(cx, cy)) {
                c.occupancy[idx] = None;
            }
        }
    }

    // Mark object as removed (swap with NONE id — avoid vec reindex which invalidates occupancy refs)
    if let Some(chunk) = world_map.chunks.get_mut(&(anchor_cx, anchor_cy)) {
        if let Some(slot) = chunk.objects.get_mut(object_index as usize) {
            *slot = PlacedObject {
                object_id: ObjectId::NONE,
                local_x: 0,
                local_y: 0,
                state: ObjectState::Default,
            };
        }
    }

    Some(obj)
}

/// Look up which object occupies a given world tile. Returns (anchor_world_x, anchor_world_y, object_index, ObjectId).
pub fn get_object_at(
    world_map: &WorldMap,
    tile_x: i32,
    tile_y: i32,
    ctx: &WorldCtxRef,
) -> Option<(i32, i32, u16, ObjectId)> {
    let wrapped_x = ctx.config.wrap_tile_x(tile_x);
    let (cx, cy) = tile_to_chunk(wrapped_x, tile_y, ctx.config.chunk_size);
    let (lx, ly) = tile_to_local(wrapped_x, tile_y, ctx.config.chunk_size);
    let idx = (ly * ctx.config.chunk_size + lx) as usize;

    let chunk = world_map.chunk(cx, cy)?;
    let occ = chunk.occupancy.get(idx)?.as_ref()?;
    let obj = chunk.objects.get(occ.object_index as usize)?;

    if obj.object_id == ObjectId::NONE {
        return None;
    }

    // Calculate anchor world position
    let base_x = cx * ctx.config.chunk_size as i32;
    let base_y = cy * ctx.config.chunk_size as i32;
    let anchor_world_x = base_x + obj.local_x as i32;
    let anchor_world_y = base_y + obj.local_y as i32;

    Some((anchor_world_x, anchor_world_y, occ.object_index, obj.object_id))
}
```

**Step 2: Add `pub mod placement;` to `src/object/mod.rs`**

```rust
pub mod definition;
pub mod placed;
pub mod placement;
pub mod registry;

pub use definition::*;
pub use placed::*;
pub use registry::*;
```

**Step 3: Write tests in `src/object/placement.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{ObjectDef, ObjectType, PlacementRule};
    use crate::test_helpers::fixtures;
    use crate::world::chunk::{Layer, WorldMap};

    fn test_object_registry() -> ObjectRegistry {
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
            },
            ObjectDef {
                id: "barrel".into(),
                display_name: "Barrel".into(),
                size: (1, 1),
                sprite: "objects/barrel.png".into(),
                solid_mask: vec![true],
                placement: PlacementRule::Floor,
                light_emission: [0, 0, 0],
                object_type: ObjectType::Decoration,
                drops: vec![],
            },
            ObjectDef {
                id: "torch".into(),
                display_name: "Torch".into(),
                size: (1, 1),
                sprite: "objects/torch.png".into(),
                solid_mask: vec![false],
                placement: PlacementRule::Wall,
                light_emission: [240, 180, 80],
                object_type: ObjectType::LightSource,
                drops: vec![],
            },
            ObjectDef {
                id: "chest".into(),
                display_name: "Chest".into(),
                size: (2, 1),
                sprite: "objects/chest.png".into(),
                solid_mask: vec![true, true],
                placement: PlacementRule::Floor,
                light_emission: [0, 0, 0],
                object_type: ObjectType::Container { slots: 16 },
                drops: vec![],
            },
        ])
    }

    #[test]
    fn can_place_on_solid_floor() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let mut map = WorldMap::default();

        // Generate a chunk with terrain
        map.get_or_generate_chunk(0, 0, &ctx);

        let obj_reg = test_object_registry();
        let barrel_id = ObjectId(1);

        // Place barrel on top of a solid tile — find surface first
        // Tile at (0, 0) in chunk 0 should be solid (underground)
        // We need a tile that is air with solid below it
        // Use a known position: set up manually
        let test_y = 5;
        map.set_tile(0, test_y - 1, Layer::Fg, crate::registry::tile::TileId(1), &ctx); // solid below
        map.set_tile(0, test_y, Layer::Fg, crate::registry::tile::TileId::AIR, &ctx); // air at placement

        assert!(can_place_object(&map, &obj_reg, barrel_id, 0, test_y, &ctx));
    }

    #[test]
    fn cannot_place_floor_object_in_air() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let mut map = WorldMap::default();
        map.get_or_generate_chunk(0, 0, &ctx);

        let obj_reg = test_object_registry();
        let barrel_id = ObjectId(1);

        // High up in the air — no solid below
        let test_y = (wc.height_tiles - 5) as i32;
        map.set_tile(0, test_y, Layer::Fg, TileId::AIR, &ctx);
        map.set_tile(0, test_y - 1, Layer::Fg, TileId::AIR, &ctx);

        assert!(!can_place_object(&map, &obj_reg, barrel_id, 0, test_y, &ctx));
    }

    #[test]
    fn cannot_place_on_occupied_tile() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let mut map = WorldMap::default();
        map.get_or_generate_chunk(0, 0, &ctx);

        let obj_reg = test_object_registry();
        let barrel_id = ObjectId(1);

        let test_y = 5;
        map.set_tile(0, test_y - 1, Layer::Fg, crate::registry::tile::TileId(1), &ctx);
        map.set_tile(0, test_y, Layer::Fg, TileId::AIR, &ctx);

        // Place first barrel
        assert!(place_object(&mut map, &obj_reg, barrel_id, 0, test_y, &ctx));

        // Try to place second barrel on same tile — should fail
        assert!(!can_place_object(&map, &obj_reg, barrel_id, 0, test_y, &ctx));
    }

    #[test]
    fn place_and_remove_object() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let mut map = WorldMap::default();
        map.get_or_generate_chunk(0, 0, &ctx);

        let obj_reg = test_object_registry();
        let barrel_id = ObjectId(1);

        let test_y = 5;
        map.set_tile(0, test_y - 1, Layer::Fg, crate::registry::tile::TileId(1), &ctx);
        map.set_tile(0, test_y, Layer::Fg, TileId::AIR, &ctx);

        assert!(place_object(&mut map, &obj_reg, barrel_id, 0, test_y, &ctx));

        // Verify object is placed
        let result = get_object_at(&map, 0, test_y, &ctx);
        assert!(result.is_some());
        let (ax, ay, idx, oid) = result.unwrap();
        assert_eq!(oid, barrel_id);

        // Remove it
        let removed = remove_object(&mut map, &obj_reg, ax, ay, idx, &ctx);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().object_id, barrel_id);

        // Verify tile is free again
        assert!(get_object_at(&map, 0, test_y, &ctx).is_none());
    }

    #[test]
    fn place_multi_tile_object() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let mut map = WorldMap::default();
        map.get_or_generate_chunk(0, 0, &ctx);

        let obj_reg = test_object_registry();
        let chest_id = ObjectId(3); // 2x1

        let test_y = 5;
        // Need solid under both tiles of chest
        map.set_tile(0, test_y - 1, Layer::Fg, crate::registry::tile::TileId(1), &ctx);
        map.set_tile(1, test_y - 1, Layer::Fg, crate::registry::tile::TileId(1), &ctx);
        map.set_tile(0, test_y, Layer::Fg, TileId::AIR, &ctx);
        map.set_tile(1, test_y, Layer::Fg, TileId::AIR, &ctx);

        assert!(can_place_object(&map, &obj_reg, chest_id, 0, test_y, &ctx));
        assert!(place_object(&mut map, &obj_reg, chest_id, 0, test_y, &ctx));

        // Both tiles should be occupied
        assert!(get_object_at(&map, 0, test_y, &ctx).is_some());
        assert!(get_object_at(&map, 1, test_y, &ctx).is_some());

        // Both point to same object
        let (_, _, idx0, _) = get_object_at(&map, 0, test_y, &ctx).unwrap();
        let (_, _, idx1, _) = get_object_at(&map, 1, test_y, &ctx).unwrap();
        assert_eq!(idx0, idx1);
    }

    #[test]
    fn wall_placement_needs_adjacent_solid() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let mut map = WorldMap::default();
        map.get_or_generate_chunk(0, 0, &ctx);

        let obj_reg = test_object_registry();
        let torch_id = ObjectId(2); // Wall placement

        let test_y = 5;
        let test_x = 5;
        map.set_tile(test_x, test_y, Layer::Fg, TileId::AIR, &ctx);

        // No solid neighbors — should fail
        map.set_tile(test_x - 1, test_y, Layer::Fg, TileId::AIR, &ctx);
        map.set_tile(test_x + 1, test_y, Layer::Fg, TileId::AIR, &ctx);
        assert!(!can_place_object(&map, &obj_reg, torch_id, test_x, test_y, &ctx));

        // Add solid to the left — should pass
        map.set_tile(test_x - 1, test_y, Layer::Fg, crate::registry::tile::TileId(1), &ctx);
        assert!(can_place_object(&map, &obj_reg, torch_id, test_x, test_y, &ctx));
    }

    #[test]
    fn container_object_initializes_slots() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let mut map = WorldMap::default();
        map.get_or_generate_chunk(0, 0, &ctx);

        let obj_reg = test_object_registry();
        let chest_id = ObjectId(3);

        let test_y = 5;
        map.set_tile(0, test_y - 1, Layer::Fg, crate::registry::tile::TileId(1), &ctx);
        map.set_tile(1, test_y - 1, Layer::Fg, crate::registry::tile::TileId(1), &ctx);
        map.set_tile(0, test_y, Layer::Fg, TileId::AIR, &ctx);
        map.set_tile(1, test_y, Layer::Fg, TileId::AIR, &ctx);

        place_object(&mut map, &obj_reg, chest_id, 0, test_y, &ctx);

        // Check container state
        let wrapped_x = ctx.config.wrap_tile_x(0);
        let (cx, cy) = tile_to_chunk(wrapped_x, test_y, ctx.config.chunk_size);
        let chunk = map.chunk(cx, cy).unwrap();
        let obj = &chunk.objects.last().unwrap();
        match &obj.state {
            ObjectState::Container { contents } => {
                assert_eq!(contents.len(), 16);
                assert!(contents.iter().all(|s| s.is_none()));
            }
            _ => panic!("expected Container state"),
        }
    }
}
```

**Step 4: Run tests**

Run: `cargo test --lib object`
Expected: All tests PASS

**Step 5: Commit**

```bash
git add src/object/placement.rs src/object/mod.rs
git commit -m "feat(object): add placement validation and place/remove logic"
```

---

### Task 4: Collision Integration

**Files:**
- Modify: `src/physics.rs` — add object collision check

**Step 1: Add `ObjectRegistry` and object collision to `tile_collision`**

The simplest approach is to extend the existing `WorldMap::is_solid` method to also check occupancy. Add a new method to `WorldMap`:

In `src/world/chunk.rs`, add:

```rust
use crate::object::registry::ObjectRegistry;

impl WorldMap {
    /// Check if a tile is solid, considering both fg tiles and placed objects.
    pub fn is_solid_or_object(
        &self,
        tile_x: i32,
        tile_y: i32,
        ctx: &WorldCtxRef,
        object_registry: &ObjectRegistry,
    ) -> bool {
        if self.is_solid(tile_x, tile_y, ctx) {
            return true;
        }

        let wrapped_x = ctx.config.wrap_tile_x(tile_x);
        let (cx, cy) = tile_to_chunk(wrapped_x, tile_y, ctx.config.chunk_size);
        let (lx, ly) = tile_to_local(wrapped_x, tile_y, ctx.config.chunk_size);

        if let Some(chunk) = self.chunks.get(&(cx, cy)) {
            let idx = (ly * ctx.config.chunk_size + lx) as usize;
            if let Some(occ) = &chunk.occupancy[idx] {
                if let Some(obj) = chunk.objects.get(occ.object_index as usize) {
                    if obj.object_id == crate::object::ObjectId::NONE {
                        return false;
                    }
                    let def = object_registry.get(obj.object_id);
                    let obj_base_x = cx * ctx.config.chunk_size as i32 + obj.local_x as i32;
                    let obj_base_y = cy * ctx.config.chunk_size as i32 + obj.local_y as i32;
                    let rel_x = (wrapped_x - obj_base_x) as u32;
                    let rel_y = (tile_y - obj_base_y) as u32;
                    return def.is_tile_solid(rel_x, rel_y);
                }
            }
        }

        false
    }
}
```

**Step 2: Modify `tile_collision` system in `src/physics.rs`**

Add `ObjectRegistry` parameter and use `is_solid_or_object` instead of `is_solid`:

```rust
pub fn tile_collision(
    time: Res<Time>,
    ctx: WorldCtx,
    world_map: Res<WorldMap>,
    object_registry: Option<Res<ObjectRegistry>>,  // Option for backward compat in tests
    mut query: Query<(...)>,
) {
    // ...
    // Replace: world_map.is_solid(tx, ty, &ctx_ref)
    // With:
    let solid = match &object_registry {
        Some(reg) => world_map.is_solid_or_object(tx, ty, &ctx_ref, reg),
        None => world_map.is_solid(tx, ty, &ctx_ref),
    };
    // ...
}
```

**Step 3: Run tests**

Run: `cargo test --lib`
Expected: All existing physics tests still PASS (ObjectRegistry is Option, so tests without it continue to work)

**Step 4: Commit**

```bash
git add src/world/chunk.rs src/physics.rs
git commit -m "feat(object): integrate object collision with physics system"
```

---

### Task 5: Entity Spawn/Despawn with Chunks

**Files:**
- Create: `src/object/spawn.rs`
- Modify: `src/object/mod.rs` — add module
- Modify: `src/world/chunk.rs` — hook into chunk loading/unloading

**Step 1: Create `src/object/spawn.rs`**

```rust
use bevy::prelude::*;

use super::definition::{ObjectId, ObjectType};
use super::registry::ObjectRegistry;
use crate::world::chunk::{ChunkCoord, WorldMap};

/// Marker component linking a runtime entity to its ChunkData storage.
#[derive(Component)]
pub struct PlacedObjectEntity {
    pub data_chunk: (i32, i32),
    pub object_index: u16,
    pub object_id: ObjectId,
}

/// Marker linking to display chunk for despawn tracking.
#[derive(Component)]
pub struct ObjectDisplayChunk {
    pub display_chunk: (i32, i32),
}

/// Spawn entities for all objects in a chunk.
/// Called after chunk entity is spawned.
pub fn spawn_objects_for_chunk(
    commands: &mut Commands,
    world_map: &WorldMap,
    object_registry: &ObjectRegistry,
    data_chunk_x: i32,
    chunk_y: i32,
    display_chunk_x: i32,
    tile_size: f32,
    chunk_size: u32,
) {
    let Some(chunk) = world_map.chunk(data_chunk_x, chunk_y) else {
        return;
    };

    let display_offset_x = (display_chunk_x - data_chunk_x) as f32 * chunk_size as f32 * tile_size;

    for (idx, obj) in chunk.objects.iter().enumerate() {
        if obj.object_id == ObjectId::NONE {
            continue;
        }

        let def = object_registry.get(obj.object_id);

        // World position of the anchor tile center
        let world_x = (data_chunk_x * chunk_size as i32 + obj.local_x as i32) as f32 * tile_size
            + tile_size / 2.0
            + display_offset_x;
        let world_y = (chunk_y * chunk_size as i32 + obj.local_y as i32) as f32 * tile_size
            + tile_size / 2.0;

        // Sprite offset for multi-tile: center sprite over all tiles
        let offset_x = (def.size.0 as f32 - 1.0) * tile_size / 2.0;
        let offset_y = (def.size.1 as f32 - 1.0) * tile_size / 2.0;

        // Z = 0.5 (between fg at 0.0 and dropped items at 1.0)
        let z = 0.5;

        commands.spawn((
            PlacedObjectEntity {
                data_chunk: (data_chunk_x, chunk_y),
                object_index: idx as u16,
                object_id: obj.object_id,
            },
            ObjectDisplayChunk {
                display_chunk: (display_chunk_x, chunk_y),
            },
            Transform::from_translation(Vec3::new(
                world_x + offset_x,
                world_y + offset_y,
                z,
            )),
            Visibility::default(),
            // Sprite will be added when sprite loading is implemented
            // For now, just position marker entities
        ));
    }
}

/// Despawn all object entities for a given display chunk.
pub fn despawn_objects_for_chunk(
    commands: &mut Commands,
    query: &Query<(Entity, &ObjectDisplayChunk)>,
    display_chunk_x: i32,
    chunk_y: i32,
) {
    for (entity, display) in query.iter() {
        if display.display_chunk == (display_chunk_x, chunk_y) {
            commands.entity(entity).despawn();
        }
    }
}
```

**Step 2: Add to `src/object/mod.rs`**

```rust
pub mod definition;
pub mod placed;
pub mod placement;
pub mod registry;
pub mod spawn;

pub use definition::*;
pub use placed::*;
pub use registry::*;
```

**Step 3: Hook into chunk loading/unloading in `src/world/chunk.rs`**

Modify `spawn_chunk` to also call `spawn_objects_for_chunk` after spawning tile entities.
Modify `despawn_chunk` to also despawn object entities.

The cleanest approach is to add an `ObjectRegistry` parameter to `chunk_loading_system` and call the spawn/despawn functions there. This requires modifying the system signature.

In `chunk_loading_system`, add after `spawn_chunk(...)`:

```rust
if let Some(ref obj_reg) = object_registry {
    crate::object::spawn::spawn_objects_for_chunk(
        &mut commands,
        &world_map,
        obj_reg,
        ctx_ref.config.wrap_chunk_x(display_cx),
        cy,
        display_cx,
        ctx_ref.config.tile_size,
        ctx_ref.config.chunk_size,
    );
}
```

In the despawn loop, before `despawn_chunk(...)`:

```rust
crate::object::spawn::despawn_objects_for_chunk(
    &mut commands,
    &object_entity_query,
    cx,
    cy,
);
```

**Step 4: Run tests**

Run: `cargo test --lib`
Expected: All tests PASS

**Step 5: Commit**

```bash
git add src/object/spawn.rs src/object/mod.rs src/world/chunk.rs
git commit -m "feat(object): spawn/despawn object entities with chunk lifecycle"
```

---

### Task 6: ObjectPlugin and Registration

**Files:**
- Create: `src/object/plugin.rs`
- Modify: `src/object/mod.rs` — add plugin
- Modify: `src/main.rs` — register plugin

**Step 1: Create `src/object/plugin.rs`**

```rust
use bevy::prelude::*;

use super::definition::{ObjectDef, ObjectType, PlacementRule};
use super::registry::ObjectRegistry;

pub struct ObjectPlugin;

impl Plugin for ObjectPlugin {
    fn build(&self, app: &mut App) {
        // Hardcoded registry for now (will move to RON loading later)
        app.insert_resource(ObjectRegistry::from_defs(vec![
            // Index 0: NONE placeholder
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
            },
            ObjectDef {
                id: "torch_object".into(),
                display_name: "Torch".into(),
                size: (1, 1),
                sprite: "objects/torch.png".into(),
                solid_mask: vec![false],
                placement: PlacementRule::Wall,
                light_emission: [240, 180, 80],
                object_type: ObjectType::LightSource,
                drops: vec![],
            },
            ObjectDef {
                id: "wooden_chest".into(),
                display_name: "Wooden Chest".into(),
                size: (2, 1),
                sprite: "objects/wooden_chest.png".into(),
                solid_mask: vec![true, true],
                placement: PlacementRule::Floor,
                light_emission: [0, 0, 0],
                object_type: ObjectType::Container { slots: 16 },
                drops: vec![],
            },
            ObjectDef {
                id: "wooden_table".into(),
                display_name: "Wooden Table".into(),
                size: (3, 2),
                sprite: "objects/wooden_table.png".into(),
                solid_mask: vec![true, false, true, false, false, false],
                placement: PlacementRule::Floor,
                light_emission: [0, 0, 0],
                object_type: ObjectType::Decoration,
                drops: vec![],
            },
        ]));
    }
}
```

**Step 2: Update `src/object/mod.rs`**

```rust
pub mod definition;
pub mod placed;
pub mod placement;
pub mod plugin;
pub mod registry;
pub mod spawn;

pub use definition::*;
pub use placed::*;
pub use plugin::ObjectPlugin;
pub use registry::*;
```

**Step 3: Add to `src/main.rs`**

After `.add_plugins(item::ItemPlugin)`:
```rust
.add_plugins(object::ObjectPlugin)
```

**Step 4: Run build**

Run: `cargo build`
Expected: Compiles without errors

**Step 5: Run all tests**

Run: `cargo test --lib`
Expected: All tests PASS

**Step 6: Commit**

```bash
git add src/object/plugin.rs src/object/mod.rs src/main.rs
git commit -m "feat(object): add ObjectPlugin with hardcoded registry"
```

---

### Task 7: Block Interaction — Place/Break Objects

**Files:**
- Modify: `src/interaction/block_action.rs` — add object placement/breaking on middle-click or extend existing LMB/RMB

**Step 1: Extend `block_interaction_system`**

Add object placement/removal logic. When the player left-clicks on a tile with an object, break the object and spawn drops. When the player has a placeable object item in hotbar, place it.

This requires:
1. Checking `get_object_at` before breaking tiles
2. Adding `ObjectRegistry` to the system params
3. Calling `remove_object` and spawning drops
4. Adding `place_object` for items that reference objects

Add `placeable_object: Option<String>` field to `ItemDef` (or reuse existing `placeable` + new `placeable_object` field).

The simpler approach: add a new field `placeable_object: Option<String>` to `ItemDef` in `src/item/definition.rs`:

```rust
/// If set, placing this item creates an object (not a tile) in the world.
#[serde(default)]
pub placeable_object: Option<String>,
```

Then in `block_interaction_system`, check `placeable_object` first, and if set, use `place_object` instead of `set_tile`.

**Step 2: Run tests**

Run: `cargo test --lib`
Expected: All tests PASS

**Step 3: Commit**

```bash
git add src/interaction/block_action.rs src/item/definition.rs
git commit -m "feat(object): integrate object place/break with block interaction system"
```

---

## Notes

- **RON loading**: The hardcoded registry in ObjectPlugin should be moved to RON file loading in a follow-up task, following the same pattern as TileRegistryAsset.
- **Sprites**: Object entity rendering requires loading sprite images and attaching them to spawned entities. This can reuse the LitSpriteMaterial pattern from dropped items.
- **Light sources**: Integration with RC lighting pipeline is a separate task — objects with `light_emission != [0,0,0]` should register as point lights.
- **Cross-chunk objects**: The current implementation handles cross-chunk occupancy writes. For objects where the anchor is in one chunk but parts extend into a neighbor, the occupancy is correctly written to all relevant chunks. However, if the neighbor chunk isn't loaded, those occupancy cells won't be written until that chunk loads. This is acceptable for MVP.
