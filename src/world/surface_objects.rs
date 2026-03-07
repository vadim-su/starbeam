//! Procedural surface decoration: places trees (and future objects) on
//! freshly generated chunks. Uses a deterministic per-column hash so results
//! are reproducible and independent of chunk load order.

use crate::object::definition::ObjectId;
use crate::object::placed::{ObjectState, OccupancyRef, PlacedObject};
use crate::object::registry::ObjectRegistry;
use crate::registry::tile::TileId;
use crate::world::chunk::{ChunkData, Layer};
use crate::world::ctx::WorldCtxRef;
use crate::world::terrain_gen::surface_height;

/// Minimum spacing between trees (in tiles).
const TREE_MIN_SPACING: i32 = 6;

/// Deterministic hash for a world column. Returns a value in 0..256.
fn column_hash(tile_x: i32, seed: u32) -> u32 {
    // Simple but effective hash: mix tile_x with seed using bit operations.
    let mut h = seed.wrapping_mul(2654435761);
    h ^= tile_x as u32;
    h = h.wrapping_mul(2246822519);
    h ^= h >> 13;
    h = h.wrapping_mul(3266489917);
    h ^= h >> 16;
    h & 0xFF
}

/// Populate a freshly generated chunk with surface objects (trees).
///
/// This function is deterministic: given the same world seed and chunk
/// coordinates, it will always produce the same object placements.
///
/// Only places objects whose anchor (bottom-left tile) falls within this chunk
/// and whose entire footprint fits within chunk boundaries. Objects at chunk
/// edges that would overflow are skipped — this means a few positions near
/// chunk borders won't get trees, which is acceptable.
pub fn populate_surface_objects(
    chunk: &mut ChunkData,
    chunk_x: i32,
    chunk_y: i32,
    ctx: &WorldCtxRef,
    object_registry: &ObjectRegistry,
) {
    let tree_id = match object_registry.by_name("tree_object") {
        Some(id) => id,
        None => return, // tree object not loaded yet
    };

    let tree_def = object_registry.get(tree_id);
    let tree_w = tree_def.size.0;
    let tree_h = tree_def.size.1;
    let chunk_size = ctx.config.chunk_size;
    let base_x = chunk_x * chunk_size as i32;
    let base_y = chunk_y * chunk_size as i32;

    // Track last tree x to enforce minimum spacing
    let mut last_tree_x: Option<i32> = None;

    // Scan each column in this chunk
    for local_x in 0..chunk_size {
        let world_x = base_x + local_x as i32;

        // Check if tree footprint fits horizontally in this chunk
        if local_x + tree_w > chunk_size {
            continue;
        }

        // Deterministic: does this column get a tree?
        let hash = column_hash(world_x, ctx.config.seed);
        // ~18% chance (46/256)
        if hash >= 46 {
            continue;
        }

        // Enforce minimum spacing
        if let Some(last_x) = last_tree_x {
            if world_x - last_x < TREE_MIN_SPACING {
                continue;
            }
        }

        // Find surface height at this x
        let surface_y = surface_height(
            ctx.noise_cache,
            world_x,
            ctx.config,
            ctx.planet_config.layers.surface.terrain_frequency,
            ctx.planet_config.layers.surface.terrain_amplitude,
        );

        // Anchor is at (world_x, surface_y + 1) — one tile above the surface
        let anchor_y = surface_y + 1;
        let local_y = anchor_y - base_y;

        // Check if entire tree fits vertically in this chunk
        if local_y < 0 || (local_y as u32 + tree_h) > chunk_size {
            continue;
        }

        // Verify anchor tiles: surface must be solid below all anchor columns
        let mut floor_ok = true;
        for dx in 0..tree_w as i32 {
            let tx = world_x + dx;
            let wrapped_tx = ctx.config.wrap_tile_x(tx);
            let sh = surface_height(
                ctx.noise_cache,
                wrapped_tx,
                ctx.config,
                ctx.planet_config.layers.surface.terrain_frequency,
                ctx.planet_config.layers.surface.terrain_amplitude,
            );
            // Surface must be at the same height (flat ground)
            if sh != surface_y {
                floor_ok = false;
                break;
            }
        }
        if !floor_ok {
            continue;
        }

        // Verify all tiles in the tree footprint are air
        let mut all_air = true;
        for dy in 0..tree_h {
            for dx in 0..tree_w {
                let lx = local_x + dx;
                let ly = local_y as u32 + dy;
                let idx = (ly * chunk_size + lx) as usize;
                if chunk.fg.tiles[idx] != TileId::AIR {
                    all_air = false;
                    break;
                }
                // Also check occupancy
                if chunk.occupancy[idx].is_some() {
                    all_air = false;
                    break;
                }
            }
            if !all_air {
                break;
            }
        }
        if !all_air {
            continue;
        }

        // Place the tree!
        let object_index = chunk.objects.len() as u16;
        chunk.objects.push(PlacedObject {
            object_id: tree_id,
            local_x,
            local_y: local_y as u32,
            state: ObjectState::Default,
        });

        // Write occupancy for all tiles
        for dy in 0..tree_h {
            for dx in 0..tree_w {
                let lx = local_x + dx;
                let ly = local_y as u32 + dy;
                let idx = (ly * chunk_size + lx) as usize;
                chunk.occupancy[idx] = Some(OccupancyRef {
                    object_index,
                    is_anchor: dx == 0 && dy == 0,
                    data_chunk: (chunk_x, chunk_y),
                });
            }
        }

        last_tree_x = Some(world_x);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::fixtures;
    use crate::world::chunk::WorldMap;
    use crate::world::terrain_gen::generate_chunk_tiles;

    fn test_object_registry_with_tree() -> ObjectRegistry {
        use crate::object::definition::{ObjectDef, ObjectType, PlacementRule};

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
                id: "tree_object".into(),
                display_name: "Tree".into(),
                size: (3, 5),
                sprite: "objects/tree.png".into(),
                solid_mask: vec![false; 15],
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
                auto_item: None,
                background: true,
            },
        ])
    }

    #[test]
    fn populate_is_deterministic() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let obj_reg = test_object_registry_with_tree();

        // Generate two identical chunks and populate them
        let chunk_x = 5;
        let chunk_y = {
            // Find a chunk that contains the surface
            let sh = surface_height(
                &nc,
                chunk_x * wc.chunk_size as i32,
                &wc,
                pc.layers.surface.terrain_frequency,
                pc.layers.surface.terrain_amplitude,
            );
            sh / wc.chunk_size as i32
        };

        let tiles1 = generate_chunk_tiles(chunk_x, chunk_y, &ctx);
        let len = tiles1.fg.len();
        let mut chunk1 = ChunkData {
            fg: crate::world::chunk::TileLayer {
                tiles: tiles1.fg,
                bitmasks: vec![0; len],
            },
            bg: crate::world::chunk::TileLayer {
                tiles: tiles1.bg,
                bitmasks: vec![0; len],
            },
            liquid: crate::liquid::LiquidLayer {
                cells: tiles1.liquid,
            },
            objects: Vec::new(),
            occupancy: vec![None; len],
            damage: vec![0; len],
        };

        let tiles2 = generate_chunk_tiles(chunk_x, chunk_y, &ctx);
        let mut chunk2 = ChunkData {
            fg: crate::world::chunk::TileLayer {
                tiles: tiles2.fg,
                bitmasks: vec![0; len],
            },
            bg: crate::world::chunk::TileLayer {
                tiles: tiles2.bg,
                bitmasks: vec![0; len],
            },
            liquid: crate::liquid::LiquidLayer {
                cells: tiles2.liquid,
            },
            objects: Vec::new(),
            occupancy: vec![None; len],
            damage: vec![0; len],
        };

        populate_surface_objects(&mut chunk1, chunk_x, chunk_y, &ctx, &obj_reg);
        populate_surface_objects(&mut chunk2, chunk_x, chunk_y, &ctx, &obj_reg);

        assert_eq!(chunk1.objects.len(), chunk2.objects.len());
        for (a, b) in chunk1.objects.iter().zip(chunk2.objects.iter()) {
            assert_eq!(a.object_id, b.object_id);
            assert_eq!(a.local_x, b.local_x);
            assert_eq!(a.local_y, b.local_y);
        }
    }

    #[test]
    fn trees_respect_minimum_spacing() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let obj_reg = test_object_registry_with_tree();

        // Generate a surface chunk and populate
        let chunk_x = 10;
        let chunk_y = {
            let sh = surface_height(
                &nc,
                chunk_x * wc.chunk_size as i32,
                &wc,
                pc.layers.surface.terrain_frequency,
                pc.layers.surface.terrain_amplitude,
            );
            sh / wc.chunk_size as i32
        };

        let tiles = generate_chunk_tiles(chunk_x, chunk_y, &ctx);
        let len = tiles.fg.len();
        let mut chunk = ChunkData {
            fg: crate::world::chunk::TileLayer {
                tiles: tiles.fg,
                bitmasks: vec![0; len],
            },
            bg: crate::world::chunk::TileLayer {
                tiles: tiles.bg,
                bitmasks: vec![0; len],
            },
            liquid: crate::liquid::LiquidLayer {
                cells: tiles.liquid,
            },
            objects: Vec::new(),
            occupancy: vec![None; len],
            damage: vec![0; len],
        };

        populate_surface_objects(&mut chunk, chunk_x, chunk_y, &ctx, &obj_reg);

        // Verify spacing between trees
        let base_x = chunk_x * wc.chunk_size as i32;
        let tree_positions: Vec<i32> = chunk
            .objects
            .iter()
            .filter(|o| o.object_id != ObjectId::NONE)
            .map(|o| base_x + o.local_x as i32)
            .collect();

        for window in tree_positions.windows(2) {
            let spacing = window[1] - window[0];
            assert!(
                spacing >= TREE_MIN_SPACING,
                "Tree spacing {} < minimum {}",
                spacing,
                TREE_MIN_SPACING
            );
        }
    }
}
