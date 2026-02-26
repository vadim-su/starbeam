use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use super::atlas::{atlas_uv, AtlasParams};
use super::autotile::{select_variant, AutotileRegistry, CHUNK_TILE_COUNT};
use crate::registry::tile::{TileId, TileRegistry};

/// Reusable buffers for building chunk meshes, avoiding per-frame allocations.
#[derive(Resource)]
pub struct MeshBuildBuffers {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl Default for MeshBuildBuffers {
    fn default() -> Self {
        Self {
            positions: Vec::with_capacity(CHUNK_TILE_COUNT * 4),
            uvs: Vec::with_capacity(CHUNK_TILE_COUNT * 4),
            indices: Vec::with_capacity(CHUNK_TILE_COUNT * 6),
        }
    }
}

/// Build a Bevy `Mesh` for a single chunk from its tile and bitmask data.
///
/// Each non-air tile becomes a textured quad. The mesh uses the combined atlas
/// for UV coordinates, selecting the correct autotile variant per tile.
#[allow(clippy::too_many_arguments)]
pub fn build_chunk_mesh(
    tiles: &[TileId],
    bitmasks: &[u8],
    display_chunk_x: i32,
    chunk_y: i32,
    chunk_size: u32,
    tile_size: f32,
    seed: u32,
    tile_registry: &TileRegistry,
    autotile_registry: &AutotileRegistry,
    atlas_params: &AtlasParams,
    buffers: &mut MeshBuildBuffers,
) -> Mesh {
    buffers.positions.clear();
    buffers.uvs.clear();
    buffers.indices.clear();

    let base_x = display_chunk_x * chunk_size as i32;
    let base_y = chunk_y * chunk_size as i32;

    for local_y in 0..chunk_size {
        for local_x in 0..chunk_size {
            let idx = (local_y * chunk_size + local_x) as usize;
            let tile_id = tiles[idx];

            if tile_id == TileId::AIR {
                continue;
            }

            let autotile_name = match tile_registry.autotile_name(tile_id) {
                Some(name) => name,
                None => continue,
            };

            let entry = match autotile_registry.entries.get(autotile_name) {
                Some(e) => e,
                None => continue,
            };

            let bitmask = bitmasks[idx];
            let variants = entry.variants_for(bitmask);

            let world_x = base_x + local_x as i32;
            let world_y = base_y + local_y as i32;
            let sprite_row = select_variant(variants, world_x, world_y, seed);

            let px = world_x as f32 * tile_size;
            let py = world_y as f32 * tile_size;

            let (u_min, u_max, v_min, v_max) =
                atlas_uv(entry.column_index, sprite_row, atlas_params);

            let vi = buffers.positions.len() as u32;

            buffers.positions.extend_from_slice(&[
                [px, py, 0.0],
                [px + tile_size, py, 0.0],
                [px + tile_size, py + tile_size, 0.0],
                [px, py + tile_size, 0.0],
            ]);

            buffers.uvs.extend_from_slice(&[
                [u_min, v_max],
                [u_max, v_max],
                [u_max, v_min],
                [u_min, v_min],
            ]);

            buffers
                .indices
                .extend_from_slice(&[vi, vi + 1, vi + 2, vi, vi + 2, vi + 3]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, buffers.positions.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, buffers.uvs.clone());
    mesh.insert_indices(Indices::U32(buffers.indices.clone()));
    mesh
}
