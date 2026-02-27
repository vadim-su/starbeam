use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, MeshVertexAttribute, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::VertexFormat;

use super::atlas::{atlas_uv, AtlasParams};
use super::autotile::{select_variant, AutotileRegistry, CHUNK_TILE_COUNT};
use crate::registry::tile::{TileId, TileRegistry};

/// Custom vertex attribute for per-tile light level (0.0 = dark, 1.0 = full light).
pub const ATTRIBUTE_LIGHT: MeshVertexAttribute =
    MeshVertexAttribute::new("Light", 988_540_917, VertexFormat::Float32);

/// Reusable buffers for building chunk meshes, avoiding per-frame allocations.
#[derive(Resource)]
pub struct MeshBuildBuffers {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub lights: Vec<f32>,
    pub indices: Vec<u32>,
}

impl Default for MeshBuildBuffers {
    fn default() -> Self {
        Self {
            positions: Vec::with_capacity(CHUNK_TILE_COUNT * 4),
            uvs: Vec::with_capacity(CHUNK_TILE_COUNT * 4),
            lights: Vec::with_capacity(CHUNK_TILE_COUNT * 4),
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
    light_levels: &[u8],
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
    buffers.lights.clear();
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

            let entry = match autotile_registry.get(autotile_name) {
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

            let light = light_levels[idx] as f32 / 255.0;
            buffers
                .lights
                .extend_from_slice(&[light, light, light, light]);

            buffers
                .indices
                .extend_from_slice(&[vi, vi + 1, vi + 2, vi, vi + 2, vi + 3]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    // Note: clone() is required because Mesh takes ownership. Cost is bounded
    // (~120KB for a full 32×32 chunk) and within frame budget.
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, buffers.positions.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, buffers.uvs.clone());
    mesh.insert_attribute(ATTRIBUTE_LIGHT, buffers.lights.clone());
    mesh.insert_indices(Indices::U32(buffers.indices.clone()));
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::assets::{AutotileAsset, BitmaskMapping, SpriteVariant};
    use crate::registry::tile::{TileDef, TileRegistry};
    use crate::world::atlas::AtlasParams;
    use crate::world::autotile::{AutotileEntry, AutotileRegistry};
    use std::collections::HashMap;

    fn test_registry() -> TileRegistry {
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
                id: "dirt".into(),
                autotile: Some("dirt".into()),
                solid: true,
                hardness: 1.0,
                friction: 0.7,
                viscosity: 0.0,
                damage_on_contact: 0.0,
                effects: vec![],
            },
        ])
    }

    fn test_autotile_registry() -> AutotileRegistry {
        let mut tiles = HashMap::new();
        tiles.insert(
            0u8,
            BitmaskMapping {
                description: "isolated".into(),
                variants: vec![SpriteVariant {
                    row: 0,
                    weight: 1.0,
                    col: 0,
                    index: 0,
                }],
            },
        );
        let asset = AutotileAsset {
            tile_size: 16,
            atlas_columns: 1,
            atlas_rows: 47,
            tiles,
        };
        let mut reg = AutotileRegistry::default();
        reg.insert("dirt".into(), AutotileEntry::from_asset(&asset, 0));
        reg
    }

    #[test]
    fn build_mesh_2x2_chunk() {
        let tile_reg = test_registry();
        let autotile_reg = test_autotile_registry();
        let params = AtlasParams {
            tile_size: 16,
            rows: 47,
            atlas_width: 16,
            atlas_height: 752,
        };
        let mut buffers = MeshBuildBuffers {
            positions: Vec::new(),
            uvs: Vec::new(),
            lights: Vec::new(),
            indices: Vec::new(),
        };

        // 2×2 chunk: [dirt, air, air, dirt]
        let tiles = vec![TileId(1), TileId(0), TileId(0), TileId(1)];
        let bitmasks = vec![0u8; 4];
        let light_levels = vec![255u8; 4];
        let chunk_size = 2;
        let tile_size = 8.0;

        let mesh = build_chunk_mesh(
            &tiles,
            &bitmasks,
            &light_levels,
            0,
            0,
            chunk_size,
            tile_size,
            42,
            &tile_reg,
            &autotile_reg,
            &params,
            &mut buffers,
        );

        // 2 solid tiles → 2 quads → 8 vertices, 12 indices
        assert_eq!(buffers.positions.len(), 8, "2 quads × 4 vertices");
        assert_eq!(buffers.uvs.len(), 8, "2 quads × 4 UVs");
        assert_eq!(buffers.lights.len(), 8, "2 quads × 4 lights");
        assert_eq!(buffers.indices.len(), 12, "2 quads × 6 indices");

        // All lights should be 1.0 (255/255)
        for &l in &buffers.lights {
            assert!(
                (l - 1.0).abs() < f32::EPSILON,
                "light should be 1.0, got {l}"
            );
        }

        // First quad at (0,0): world pos (0.0, 0.0)
        assert_eq!(buffers.positions[0], [0.0, 0.0, 0.0]);
        assert_eq!(buffers.positions[1], [8.0, 0.0, 0.0]);
        assert_eq!(buffers.positions[2], [8.0, 8.0, 0.0]);
        assert_eq!(buffers.positions[3], [0.0, 8.0, 0.0]);

        // Second quad at (1,1): world pos (8.0, 8.0)
        assert_eq!(buffers.positions[4], [8.0, 8.0, 0.0]);
        assert_eq!(buffers.positions[5], [16.0, 8.0, 0.0]);
        assert_eq!(buffers.positions[6], [16.0, 16.0, 0.0]);
        assert_eq!(buffers.positions[7], [8.0, 16.0, 0.0]);

        // UVs should be valid (within atlas bounds)
        for uv in &buffers.uvs {
            assert!(uv[0] >= 0.0 && uv[0] <= 1.0, "u out of range: {}", uv[0]);
            assert!(uv[1] >= 0.0 && uv[1] <= 1.0, "v out of range: {}", uv[1]);
        }

        // Mesh should have attributes set
        assert!(mesh.attribute(Mesh::ATTRIBUTE_POSITION).is_some());
        assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
        assert!(mesh.attribute(ATTRIBUTE_LIGHT).is_some());
        assert!(mesh.indices().is_some());
    }

    #[test]
    fn build_mesh_all_air_produces_empty_mesh() {
        let tile_reg = test_registry();
        let autotile_reg = test_autotile_registry();
        let params = AtlasParams {
            tile_size: 16,
            rows: 47,
            atlas_width: 16,
            atlas_height: 752,
        };
        let mut buffers = MeshBuildBuffers {
            positions: Vec::new(),
            uvs: Vec::new(),
            lights: Vec::new(),
            indices: Vec::new(),
        };

        let tiles = vec![TileId::AIR; 4];
        let bitmasks = vec![0u8; 4];
        let light_levels = vec![255u8; 4];

        build_chunk_mesh(
            &tiles,
            &bitmasks,
            &light_levels,
            0,
            0,
            2,
            8.0,
            42,
            &tile_reg,
            &autotile_reg,
            &params,
            &mut buffers,
        );

        assert_eq!(buffers.positions.len(), 0, "all air = no vertices");
        assert_eq!(buffers.indices.len(), 0, "all air = no indices");
    }
}
