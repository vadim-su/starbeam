use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, MeshVertexAttribute, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::VertexFormat;

use super::atlas::{atlas_uv, AtlasParams};
use super::autotile::{select_variant, AutotileRegistry, CHUNK_TILE_COUNT};
use crate::registry::tile::{TileId, TileRegistry};
use crate::world::chunk::Layer;

/// Custom vertex attribute for per-vertex RGB light (0.0 = dark, 1.0 = full light per channel).
pub const ATTRIBUTE_LIGHT: MeshVertexAttribute =
    MeshVertexAttribute::new("Light", 988_540_917, VertexFormat::Float32x3);

/// Reusable buffers for building chunk meshes, avoiding per-frame allocations.
#[derive(Resource)]
pub struct MeshBuildBuffers {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub lights: Vec<[f32; 3]>,
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

/// Get light at a local chunk position, clamping out-of-bounds to nearest edge tile.
fn get_light(light_levels: &[[u8; 3]], chunk_size: u32, lx: i32, ly: i32) -> [u8; 3] {
    let cx = lx.clamp(0, chunk_size as i32 - 1) as u32;
    let cy = ly.clamp(0, chunk_size as i32 - 1) as u32;
    light_levels[(cy * chunk_size + cx) as usize]
}

/// Compute smoothed light for one vertex by averaging 4 tiles sharing that corner.
/// `dx`, `dy`: direction to the 3 neighbor tiles (-1 or +1).
fn corner_light(
    light_levels: &[[u8; 3]],
    chunk_size: u32,
    local_x: i32,
    local_y: i32,
    dx: i32,
    dy: i32,
) -> [f32; 3] {
    let s0 = get_light(light_levels, chunk_size, local_x, local_y);
    let s1 = get_light(light_levels, chunk_size, local_x + dx, local_y);
    let s2 = get_light(light_levels, chunk_size, local_x, local_y + dy);
    let s3 = get_light(light_levels, chunk_size, local_x + dx, local_y + dy);
    [
        (s0[0] as f32 + s1[0] as f32 + s2[0] as f32 + s3[0] as f32) / (4.0 * 255.0),
        (s0[1] as f32 + s1[1] as f32 + s2[1] as f32 + s3[1] as f32) / (4.0 * 255.0),
        (s0[2] as f32 + s1[2] as f32 + s2[2] as f32 + s3[2] as f32) / (4.0 * 255.0),
    ]
}

/// Dim factor for bg tiles with maximum fg shadow (all 4 corner neighbors are fg solid).
const FG_SHADOW_DIM: f32 = 0.5;

/// Compute shadow factor for a bg vertex from nearby foreground tiles.
/// Uses corner averaging: checks 4 tiles sharing the vertex corner.
/// Returns multiplier in [FG_SHADOW_DIM, 1.0] where 1.0 = no shadow.
fn corner_shadow(
    fg_tiles: &[TileId],
    chunk_size: u32,
    local_x: i32,
    local_y: i32,
    dx: i32,
    dy: i32,
    tile_registry: &TileRegistry,
) -> f32 {
    let positions = [
        (local_x, local_y),
        (local_x + dx, local_y),
        (local_x, local_y + dy),
        (local_x + dx, local_y + dy),
    ];
    let mut shadow_count = 0u32;
    for (nx, ny) in positions {
        let cx = nx.clamp(0, chunk_size as i32 - 1) as u32;
        let cy = ny.clamp(0, chunk_size as i32 - 1) as u32;
        let idx = (cy * chunk_size + cx) as usize;
        if tile_registry.is_solid(fg_tiles[idx]) {
            shadow_count += 1;
        }
    }
    let ratio = shadow_count as f32 / 4.0;
    1.0 - ratio * (1.0 - FG_SHADOW_DIM)
}

/// Build a Bevy `Mesh` for a single chunk from its tile and bitmask data.
///
/// Each non-air tile becomes a textured quad. The mesh uses the combined atlas
/// for UV coordinates, selecting the correct autotile variant per tile.
#[allow(clippy::too_many_arguments)]
pub fn build_chunk_mesh(
    tiles: &[TileId],
    bitmasks: &[u8],
    light_levels: &[[u8; 3]],
    fg_tiles: Option<&[TileId]>,
    display_chunk_x: i32,
    chunk_y: i32,
    chunk_size: u32,
    tile_size: f32,
    seed: u32,
    layer: Layer,
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
            let layer_val = match layer {
                Layer::Fg => 0,
                Layer::Bg => 1,
            };
            let sprite_row = select_variant(variants, world_x, world_y, seed, layer_val);

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

            let lx = local_x as i32;
            let ly = local_y as i32;
            let mut bl = corner_light(light_levels, chunk_size, lx, ly, -1, -1);
            let mut br_light = corner_light(light_levels, chunk_size, lx, ly, 1, -1);
            let mut tr_light = corner_light(light_levels, chunk_size, lx, ly, 1, 1);
            let mut tl = corner_light(light_levels, chunk_size, lx, ly, -1, 1);

            // Apply fg→bg shadow if building bg mesh
            if let Some(fg) = fg_tiles {
                let s_bl = corner_shadow(fg, chunk_size, lx, ly, -1, -1, tile_registry);
                let s_br = corner_shadow(fg, chunk_size, lx, ly, 1, -1, tile_registry);
                let s_tr = corner_shadow(fg, chunk_size, lx, ly, 1, 1, tile_registry);
                let s_tl = corner_shadow(fg, chunk_size, lx, ly, -1, 1, tile_registry);
                bl = [bl[0] * s_bl, bl[1] * s_bl, bl[2] * s_bl];
                br_light = [br_light[0] * s_br, br_light[1] * s_br, br_light[2] * s_br];
                tr_light = [tr_light[0] * s_tr, tr_light[1] * s_tr, tr_light[2] * s_tr];
                tl = [tl[0] * s_tl, tl[1] * s_tl, tl[2] * s_tl];
            }

            buffers
                .lights
                .extend_from_slice(&[bl, br_light, tr_light, tl]);

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
    use crate::world::chunk::Layer;
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
                light_emission: [0, 0, 0],
                light_opacity: 0,
                albedo: [0, 0, 0],
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
                light_emission: [0, 0, 0],
                light_opacity: 15,
                albedo: [139, 90, 43],
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
        let light_levels = vec![[255u8, 255, 255]; 4];
        let chunk_size = 2;
        let tile_size = 8.0;

        let mesh = build_chunk_mesh(
            &tiles,
            &bitmasks,
            &light_levels,
            None,
            0,
            0,
            chunk_size,
            tile_size,
            42,
            Layer::Fg,
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

        // All lights should be [1.0, 1.0, 1.0] (uniform 255 → corner avg = 1.0)
        for l in &buffers.lights {
            assert!(
                (l[0] - 1.0).abs() < f32::EPSILON,
                "R should be 1.0, got {}",
                l[0]
            );
            assert!(
                (l[1] - 1.0).abs() < f32::EPSILON,
                "G should be 1.0, got {}",
                l[1]
            );
            assert!(
                (l[2] - 1.0).abs() < f32::EPSILON,
                "B should be 1.0, got {}",
                l[2]
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
        let light_levels = vec![[255u8, 255, 255]; 4];

        build_chunk_mesh(
            &tiles,
            &bitmasks,
            &light_levels,
            None,
            0,
            0,
            2,
            8.0,
            42,
            Layer::Fg,
            &tile_reg,
            &autotile_reg,
            &params,
            &mut buffers,
        );

        assert_eq!(buffers.positions.len(), 0, "all air = no vertices");
        assert_eq!(buffers.indices.len(), 0, "all air = no indices");
    }

    #[test]
    fn corner_averaging_uniform_light() {
        let lights = vec![[200u8, 100, 50]; 4]; // 2x2 chunk, all same
        let bl = corner_light(&lights, 2, 0, 0, -1, -1);
        assert!((bl[0] - 200.0 / 255.0).abs() < 0.01);
        assert!((bl[1] - 100.0 / 255.0).abs() < 0.01);
        assert!((bl[2] - 50.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn corner_averaging_gradient() {
        // 2x2: (0,0)=[128,128,128], (1,0)=[0,0,0], (0,1)=[255,255,255], (1,1)=[128,128,128]
        let lights = vec![
            [128, 128, 128], // (0,0)
            [0, 0, 0],       // (1,0)
            [255, 255, 255], // (0,1)
            [128, 128, 128], // (1,1)
        ];
        // top-right vertex of (0,0): averages tiles (0,0),(1,0),(0,1),(1,1) = avg(128,0,255,128) = 127.75
        let tr = corner_light(&lights, 2, 0, 0, 1, 1);
        let expected = (128.0 + 0.0 + 255.0 + 128.0) / (4.0 * 255.0);
        assert!((tr[0] - expected).abs() < 0.01);
    }

    #[test]
    fn corner_shadow_no_fg_returns_one() {
        let fg_tiles = vec![TileId::AIR; 4]; // 2x2 all air
        let reg = test_registry();
        let s = corner_shadow(&fg_tiles, 2, 0, 0, 1, 1, &reg);
        assert!((s - 1.0).abs() < f32::EPSILON, "no fg = no shadow, got {s}");
    }

    #[test]
    fn corner_shadow_full_fg_returns_dim() {
        let reg = test_registry();
        let fg_tiles = vec![TileId(1); 4]; // 2x2 all dirt (solid)
        let s = corner_shadow(&fg_tiles, 2, 0, 0, 1, 1, &reg);
        assert!(
            (s - FG_SHADOW_DIM).abs() < 0.01,
            "all solid fg = full shadow dim, got {s}"
        );
    }

    #[test]
    fn corner_shadow_partial_fg() {
        let reg = test_registry();
        // 2x2: dirt at (0,0), air elsewhere
        let fg_tiles = vec![TileId(1), TileId::AIR, TileId::AIR, TileId::AIR];
        let s = corner_shadow(&fg_tiles, 2, 0, 0, 1, 1, &reg);
        // 1 of 4 tiles is solid, ratio = 0.25
        let expected = 1.0 - 0.25 * (1.0 - FG_SHADOW_DIM);
        assert!((s - expected).abs() < 0.01, "expected {expected}, got {s}");
    }
}
