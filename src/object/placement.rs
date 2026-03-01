use crate::object::definition::{ObjectId, ObjectType, PlacementRule};
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

            match world_map.get_tile(tx, ty, Layer::Fg, ctx) {
                Some(tile) if tile != TileId::AIR => return false,
                None => return false,
                _ => {}
            }

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

    // 2. Placement rule
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
        PlacementRule::FloorOrWall => {
            let floor_ok = (0..w).all(|dx| world_map.is_solid(anchor_x + dx, anchor_y - 1, ctx));
            let left_solid = world_map.is_solid(anchor_x - 1, anchor_y, ctx);
            let right_solid = world_map.is_solid(anchor_x + w, anchor_y, ctx);
            if !floor_ok && !left_solid && !right_solid {
                return false;
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

    let wrapped_anchor_x = ctx.config.wrap_tile_x(anchor_x);
    let (anchor_cx, anchor_cy) = tile_to_chunk(wrapped_anchor_x, anchor_y, ctx.config.chunk_size);
    let (anchor_lx, anchor_ly) = tile_to_local(wrapped_anchor_x, anchor_y, ctx.config.chunk_size);

    let state = match &def.object_type {
        ObjectType::Container { slots } => ObjectState::Container {
            contents: vec![None; *slots as usize],
        },
        _ => ObjectState::Default,
    };

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
                    data_chunk: (anchor_cx, anchor_cy),
                });
            }
        }
    }

    true
}

/// Remove an object from the world map by its anchor world coords and object index.
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

    let obj = world_map
        .chunks
        .get(&(anchor_cx, anchor_cy))?
        .objects
        .get(object_index as usize)?
        .clone();

    if obj.object_id == ObjectId::NONE {
        return None;
    }

    let def = object_registry.get(obj.object_id);
    let w = def.size.0;
    let h = def.size.1;

    // Clear occupancy
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

    // Mark object as removed (don't delete from Vec — would invalidate indices)
    // TODO: compact tombstones on chunk save/unload to prevent unbounded growth
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

/// Look up which object occupies a given world tile.
/// Returns (anchor_world_x, anchor_world_y, object_index, ObjectId).
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

    // Read the PlacedObject from the chunk where it was stored (may differ for multi-tile objects)
    let (dcx, dcy) = occ.data_chunk;
    let data_chunk = world_map.chunk(dcx, dcy)?;
    let obj = data_chunk.objects.get(occ.object_index as usize)?;

    if obj.object_id == ObjectId::NONE {
        return None;
    }

    let base_x = dcx * ctx.config.chunk_size as i32;
    let base_y = dcy * ctx.config.chunk_size as i32;
    let anchor_world_x = base_x + obj.local_x as i32;
    let anchor_world_y = base_y + obj.local_y as i32;

    Some((
        anchor_world_x,
        anchor_world_y,
        occ.object_index,
        obj.object_id,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{ObjectDef, ObjectType, PlacementRule};
    use crate::test_helpers::fixtures;
    use crate::world::chunk::WorldMap;

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
                sprite_columns: 1,
                sprite_rows: 1,
                sprite_fps: 0.0,
                flicker_speed: 0.0,
                flicker_strength: 0.0,
                flicker_min: 1.0,
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
                sprite_columns: 1,
                sprite_rows: 1,
                sprite_fps: 0.0,
                flicker_speed: 0.0,
                flicker_strength: 0.0,
                flicker_min: 1.0,
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
                sprite_columns: 1,
                sprite_rows: 1,
                sprite_fps: 0.0,
                flicker_speed: 0.0,
                flicker_strength: 0.0,
                flicker_min: 1.0,
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
                sprite_columns: 1,
                sprite_rows: 1,
                sprite_fps: 0.0,
                flicker_speed: 0.0,
                flicker_strength: 0.0,
                flicker_min: 1.0,
            },
            // Index 4: FloorOrWall placement
            ObjectDef {
                id: "lantern".into(),
                display_name: "Lantern".into(),
                size: (1, 1),
                sprite: "objects/lantern.png".into(),
                solid_mask: vec![false],
                placement: PlacementRule::FloorOrWall,
                light_emission: [200, 160, 60],
                object_type: ObjectType::LightSource,
                drops: vec![],
                sprite_columns: 1,
                sprite_rows: 1,
                sprite_fps: 0.0,
                flicker_speed: 0.0,
                flicker_strength: 0.0,
                flicker_min: 1.0,
            },
        ])
    }

    #[test]
    fn can_place_on_solid_floor() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let mut map = WorldMap::default();
        map.get_or_generate_chunk(0, 0, &ctx);

        let obj_reg = test_object_registry();
        let barrel_id = ObjectId(1);

        let test_y = 5;
        map.set_tile(0, test_y - 1, Layer::Fg, TileId(1), &ctx); // solid below
        map.set_tile(0, test_y, Layer::Fg, TileId::AIR, &ctx); // air at placement

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

        // Both tiles air — no floor
        let test_y = 5;
        map.set_tile(0, test_y, Layer::Fg, TileId::AIR, &ctx);
        map.set_tile(0, test_y - 1, Layer::Fg, TileId::AIR, &ctx);

        assert!(!can_place_object(
            &map, &obj_reg, barrel_id, 0, test_y, &ctx
        ));
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
        map.set_tile(0, test_y - 1, Layer::Fg, TileId(1), &ctx);
        map.set_tile(0, test_y, Layer::Fg, TileId::AIR, &ctx);

        assert!(place_object(&mut map, &obj_reg, barrel_id, 0, test_y, &ctx));
        assert!(!can_place_object(
            &map, &obj_reg, barrel_id, 0, test_y, &ctx
        ));
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
        map.set_tile(0, test_y - 1, Layer::Fg, TileId(1), &ctx);
        map.set_tile(0, test_y, Layer::Fg, TileId::AIR, &ctx);

        assert!(place_object(&mut map, &obj_reg, barrel_id, 0, test_y, &ctx));

        let result = get_object_at(&map, 0, test_y, &ctx);
        assert!(result.is_some());
        let (ax, ay, idx, oid) = result.unwrap();
        assert_eq!(oid, barrel_id);

        let removed = remove_object(&mut map, &obj_reg, ax, ay, idx, &ctx);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().object_id, barrel_id);

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
        map.set_tile(0, test_y - 1, Layer::Fg, TileId(1), &ctx);
        map.set_tile(1, test_y - 1, Layer::Fg, TileId(1), &ctx);
        map.set_tile(0, test_y, Layer::Fg, TileId::AIR, &ctx);
        map.set_tile(1, test_y, Layer::Fg, TileId::AIR, &ctx);

        assert!(can_place_object(&map, &obj_reg, chest_id, 0, test_y, &ctx));
        assert!(place_object(&mut map, &obj_reg, chest_id, 0, test_y, &ctx));

        assert!(get_object_at(&map, 0, test_y, &ctx).is_some());
        assert!(get_object_at(&map, 1, test_y, &ctx).is_some());

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
        let torch_id = ObjectId(2);

        let test_y = 5;
        let test_x = 5;
        map.set_tile(test_x, test_y, Layer::Fg, TileId::AIR, &ctx);
        map.set_tile(test_x - 1, test_y, Layer::Fg, TileId::AIR, &ctx);
        map.set_tile(test_x + 1, test_y, Layer::Fg, TileId::AIR, &ctx);

        assert!(!can_place_object(
            &map, &obj_reg, torch_id, test_x, test_y, &ctx
        ));

        map.set_tile(test_x - 1, test_y, Layer::Fg, TileId(1), &ctx);
        assert!(can_place_object(
            &map, &obj_reg, torch_id, test_x, test_y, &ctx
        ));
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
        map.set_tile(0, test_y - 1, Layer::Fg, TileId(1), &ctx);
        map.set_tile(1, test_y - 1, Layer::Fg, TileId(1), &ctx);
        map.set_tile(0, test_y, Layer::Fg, TileId::AIR, &ctx);
        map.set_tile(1, test_y, Layer::Fg, TileId::AIR, &ctx);

        place_object(&mut map, &obj_reg, chest_id, 0, test_y, &ctx);

        let wrapped_x = ctx.config.wrap_tile_x(0);
        let (cx, cy) = tile_to_chunk(wrapped_x, test_y, ctx.config.chunk_size);
        let chunk = map.chunk(cx, cy).unwrap();
        let obj = chunk.objects.last().unwrap();
        match &obj.state {
            ObjectState::Container { contents } => {
                assert_eq!(contents.len(), 16);
                assert!(contents.iter().all(|s| s.is_none()));
            }
            _ => panic!("expected Container state"),
        }
    }

    #[test]
    fn get_object_at_returns_none_for_empty() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let mut map = WorldMap::default();
        map.get_or_generate_chunk(0, 0, &ctx);

        assert!(get_object_at(&map, 5, 5, &ctx).is_none());
    }

    #[test]
    fn floor_or_wall_placement_accepts_floor_or_wall() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let mut map = WorldMap::default();
        map.get_or_generate_chunk(0, 0, &ctx);

        let obj_reg = test_object_registry();
        let lantern_id = ObjectId(4);

        let test_x = 5;
        let test_y = 5;

        // Clear all surrounding tiles
        map.set_tile(test_x, test_y, Layer::Fg, TileId::AIR, &ctx);
        map.set_tile(test_x, test_y - 1, Layer::Fg, TileId::AIR, &ctx);
        map.set_tile(test_x - 1, test_y, Layer::Fg, TileId::AIR, &ctx);
        map.set_tile(test_x + 1, test_y, Layer::Fg, TileId::AIR, &ctx);

        // No floor, no wall — should fail
        assert!(!can_place_object(
            &map, &obj_reg, lantern_id, test_x, test_y, &ctx
        ));

        // Add floor below — should succeed
        map.set_tile(test_x, test_y - 1, Layer::Fg, TileId(1), &ctx);
        assert!(can_place_object(
            &map, &obj_reg, lantern_id, test_x, test_y, &ctx
        ));

        // Remove floor, add wall to the left — should succeed
        map.set_tile(test_x, test_y - 1, Layer::Fg, TileId::AIR, &ctx);
        map.set_tile(test_x - 1, test_y, Layer::Fg, TileId(1), &ctx);
        assert!(can_place_object(
            &map, &obj_reg, lantern_id, test_x, test_y, &ctx
        ));

        // Remove left wall, add wall to the right — should succeed
        map.set_tile(test_x - 1, test_y, Layer::Fg, TileId::AIR, &ctx);
        map.set_tile(test_x + 1, test_y, Layer::Fg, TileId(1), &ctx);
        assert!(can_place_object(
            &map, &obj_reg, lantern_id, test_x, test_y, &ctx
        ));
    }
}
