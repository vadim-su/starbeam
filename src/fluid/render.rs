use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, MeshVertexAttribute, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::VertexFormat;

use super::cell::FluidCell;
use super::registry::FluidRegistry;

/// Z-position for fluid quads: between tiles (z=0) and entities.
const FLUID_Z: f32 = 0.5;

/// Maximum number of cells to scan when computing depth_in_fluid.
const MAX_DEPTH_SCAN: u32 = 16;

/// Custom vertex attribute carrying per-vertex emission and flags.
/// Layout: `[emission_r, emission_g, emission_b, flags]`
///   - emission: from `FluidDef.light_emission` (each / 255.0)
///   - flags: `is_wave_vertex * 1.0 + is_gas * 2.0`
pub const ATTRIBUTE_FLUID_DATA: MeshVertexAttribute =
    MeshVertexAttribute::new("FluidData", 982301567, VertexFormat::Float32x4);

/// Determine whether a liquid cell is at the surface (exposed to air above).
///
/// A liquid cell at `(local_x, local_y)` is "surface" when the cell directly
/// above `(local_x, local_y + 1)` is empty, out-of-bounds, or a different fluid.
fn is_liquid_surface(
    fluids: &[FluidCell],
    local_x: u32,
    local_y: u32,
    chunk_size: u32,
    fluid_id: super::cell::FluidId,
) -> bool {
    let above_y = local_y + 1;
    if above_y >= chunk_size {
        return true; // out-of-bounds → surface
    }
    let above_idx = (above_y * chunk_size + local_x) as usize;
    let above = &fluids[above_idx];
    above.is_empty() || above.fluid_id != fluid_id
}

/// Determine whether a gas cell is at the surface (exposed to air below).
///
/// A gas cell at `(local_x, local_y)` is "surface" when the cell directly
/// below `(local_x, local_y - 1)` is empty, out-of-bounds, or a different fluid.
fn is_gas_surface(
    fluids: &[FluidCell],
    local_x: u32,
    local_y: u32,
    chunk_size: u32,
    fluid_id: super::cell::FluidId,
) -> bool {
    if local_y == 0 {
        return true; // out-of-bounds → surface
    }
    let below_idx = ((local_y - 1) * chunk_size + local_x) as usize;
    let below = &fluids[below_idx];
    below.is_empty() || below.fluid_id != fluid_id
}

/// Compute depth_in_fluid: normalized 0..1 (0 = surface, 1 = deepest).
///
/// For liquids: scan upward from cell to find the surface (max 16 cells).
/// For gases: scan downward from cell to find the surface (max 16 cells).
fn compute_depth(
    fluids: &[FluidCell],
    local_x: u32,
    local_y: u32,
    chunk_size: u32,
    fluid_id: super::cell::FluidId,
    is_gas: bool,
) -> f32 {
    let mut distance: u32 = 0;

    if is_gas {
        // Scan downward to find surface
        let mut sy = local_y;
        while distance < MAX_DEPTH_SCAN {
            if sy == 0 {
                break; // hit bottom boundary → this is surface
            }
            sy -= 1;
            let idx = (sy * chunk_size + local_x) as usize;
            let neighbor = &fluids[idx];
            if neighbor.is_empty() || neighbor.fluid_id != fluid_id {
                break; // found surface
            }
            distance += 1;
        }
    } else {
        // Scan upward to find surface
        let mut sy = local_y;
        while distance < MAX_DEPTH_SCAN {
            sy += 1;
            if sy >= chunk_size {
                break; // hit top boundary → this is surface
            }
            let idx = (sy * chunk_size + local_x) as usize;
            let neighbor = &fluids[idx];
            if neighbor.is_empty() || neighbor.fluid_id != fluid_id {
                break; // found surface
            }
            distance += 1;
        }
    }

    distance as f32 / MAX_DEPTH_SCAN as f32
}

/// Build a Bevy `Mesh` for the fluid layer of a single chunk.
///
/// Each non-empty fluid cell becomes a colored quad whose height reflects
/// the fill level (`min(mass, 1.0)`). Liquids fill bottom-up; gases fill
/// top-down. Returns `None` when the chunk contains no visible fluids.
///
/// Emits four vertex attributes per quad:
/// - `POSITION`: world-space quad corners
/// - `COLOR`: RGBA from FluidDef, alpha scaled by fill
/// - `UV_0`: `[fill_level, depth_in_fluid]` per vertex
/// - `FLUID_DATA`: `[emission_r, emission_g, emission_b, flags]`
#[allow(clippy::too_many_arguments)]
pub fn build_fluid_mesh(
    fluids: &[FluidCell],
    chunk_x: i32,
    chunk_y: i32,
    chunk_size: u32,
    tile_size: f32,
    fluid_registry: &FluidRegistry,
) -> Option<Mesh> {
    let capacity = fluids.len();
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(capacity * 4);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(capacity * 4);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(capacity * 4);
    let mut fluid_data: Vec<[f32; 4]> = Vec::with_capacity(capacity * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(capacity * 6);

    let base_x = chunk_x * chunk_size as i32;
    let base_y = chunk_y * chunk_size as i32;

    for local_y in 0..chunk_size {
        for local_x in 0..chunk_size {
            let idx = (local_y * chunk_size + local_x) as usize;
            let cell = &fluids[idx];

            if cell.is_empty() {
                continue;
            }

            let def = fluid_registry.get(cell.fluid_id);
            let fill = cell.mass.min(1.0);

            let world_x = (base_x + local_x as i32) as f32 * tile_size;
            let world_y = (base_y + local_y as i32) as f32 * tile_size;

            // Vertical extent depends on fluid type:
            //   liquid → fills from bottom up
            //   gas    → fills from top down
            let (y0, y1) = if def.is_gas {
                let y0 = world_y + (1.0 - fill) * tile_size;
                let y1 = world_y + tile_size;
                (y0, y1)
            } else {
                let y0 = world_y;
                let y1 = world_y + fill * tile_size;
                (y0, y1)
            };

            // RGBA colour from definition, alpha scaled by fill level.
            let color = [
                def.color[0] as f32 / 255.0,
                def.color[1] as f32 / 255.0,
                def.color[2] as f32 / 255.0,
                (def.color[3] as f32 / 255.0) * fill,
            ];

            // UV_0: [fill_level, depth_in_fluid]
            let depth = compute_depth(
                fluids,
                local_x,
                local_y,
                chunk_size,
                cell.fluid_id,
                def.is_gas,
            );
            let uv = [fill, depth];

            // Emission from FluidDef.light_emission
            let emission = [
                def.light_emission[0] as f32 / 255.0,
                def.light_emission[1] as f32 / 255.0,
                def.light_emission[2] as f32 / 255.0,
            ];

            // Surface detection for wave vertices
            let is_surface = if def.is_gas {
                is_gas_surface(fluids, local_x, local_y, chunk_size, cell.fluid_id)
            } else {
                is_liquid_surface(fluids, local_x, local_y, chunk_size, cell.fluid_id)
            };

            let is_gas_flag = if def.is_gas { 2.0 } else { 0.0 };

            // Vertex indices in quad:
            //   0 = bottom-left, 1 = bottom-right, 2 = top-right, 3 = top-left
            // Liquid surface: wave on top vertices (2, 3)
            // Gas surface: wave on bottom vertices (0, 1)
            let wave_flags: [f32; 4] = if is_surface {
                if def.is_gas {
                    // Gas: bottom vertices (0, 1) are wave vertices
                    [1.0, 1.0, 0.0, 0.0]
                } else {
                    // Liquid: top vertices (2, 3) are wave vertices
                    [0.0, 0.0, 1.0, 1.0]
                }
            } else {
                [0.0, 0.0, 0.0, 0.0]
            };

            let vi = positions.len() as u32;

            positions.extend_from_slice(&[
                [world_x, y0, FLUID_Z],
                [world_x + tile_size, y0, FLUID_Z],
                [world_x + tile_size, y1, FLUID_Z],
                [world_x, y1, FLUID_Z],
            ]);

            colors.extend_from_slice(&[color, color, color, color]);

            uvs.extend_from_slice(&[uv, uv, uv, uv]);

            // FLUID_DATA: [emission_r, emission_g, emission_b, flags]
            for i in 0..4 {
                let flags = wave_flags[i] * 1.0 + is_gas_flag;
                fluid_data.push([emission[0], emission[1], emission[2], flags]);
            }

            indices.extend_from_slice(&[vi, vi + 1, vi + 2, vi, vi + 2, vi + 3]);
        }
    }

    if positions.is_empty() {
        return None;
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(ATTRIBUTE_FLUID_DATA, fluid_data);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::cell::{FluidCell, FluidId};
    use crate::fluid::registry::{FluidDef, FluidRegistry};

    fn test_fluid_registry() -> FluidRegistry {
        FluidRegistry::from_defs(vec![
            FluidDef {
                id: "water".to_string(),
                density: 1000.0,
                viscosity: 0.1,
                max_compress: 0.02,
                is_gas: false,
                color: [64, 128, 255, 200],
                damage_on_contact: 0.0,
                light_emission: [0, 0, 0],
                effects: vec![],
            },
            FluidDef {
                id: "steam".to_string(),
                density: 0.6,
                viscosity: 0.05,
                max_compress: 0.01,
                is_gas: true,
                color: [200, 200, 200, 100],
                damage_on_contact: 0.0,
                light_emission: [0, 0, 0],
                effects: vec![],
            },
            FluidDef {
                id: "lava".to_string(),
                density: 3000.0,
                viscosity: 0.6,
                max_compress: 0.01,
                is_gas: false,
                color: [255, 80, 20, 220],
                damage_on_contact: 10.0,
                light_emission: [255, 100, 20],
                effects: vec![],
            },
        ])
    }

    #[test]
    fn empty_chunk_returns_none() {
        let reg = test_fluid_registry();
        let fluids = vec![FluidCell::EMPTY; 4];
        let result = build_fluid_mesh(&fluids, 0, 0, 2, 8.0, &reg);
        assert!(result.is_none(), "all-empty chunk should return None");
    }

    #[test]
    fn single_liquid_cell_produces_quad() {
        let reg = test_fluid_registry();
        // 2×2 chunk: one water cell at (0,0), rest empty
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 0.5); // water, half full

        let mesh = build_fluid_mesh(&fluids, 0, 0, 2, 8.0, &reg).expect("should produce a mesh");

        // 1 quad → 4 vertices, 6 indices
        assert!(mesh.attribute(Mesh::ATTRIBUTE_POSITION).is_some());
        assert!(mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_some());
        assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
        assert!(mesh.attribute(ATTRIBUTE_FLUID_DATA).is_some());
        assert!(mesh.indices().is_some());

        if let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        {
            assert_eq!(pos.len(), 4, "1 quad = 4 vertices");
            // Liquid fills bottom-up: y0 = 0.0, y1 = 0.5 * 8.0 = 4.0
            assert_eq!(pos[0], [0.0, 0.0, 0.5]);
            assert_eq!(pos[1], [8.0, 0.0, 0.5]);
            assert_eq!(pos[2], [8.0, 4.0, 0.5]);
            assert_eq!(pos[3], [0.0, 4.0, 0.5]);
        } else {
            panic!("expected Float32x3 positions");
        }

        if let Some(Indices::U32(idx)) = mesh.indices() {
            assert_eq!(idx.len(), 6, "1 quad = 6 indices");
            assert_eq!(idx, &[0, 1, 2, 0, 2, 3]);
        } else {
            panic!("expected U32 indices");
        }
    }

    #[test]
    fn gas_fills_top_down() {
        let reg = test_fluid_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(2), 0.5); // steam, half full

        let mesh = build_fluid_mesh(&fluids, 0, 0, 2, 8.0, &reg).expect("should produce a mesh");

        if let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        {
            // Gas fills top-down: y0 = 0.0 + (1-0.5)*8 = 4.0, y1 = 0.0 + 8.0 = 8.0
            assert_eq!(pos[0], [0.0, 4.0, 0.5]);
            assert_eq!(pos[1], [8.0, 4.0, 0.5]);
            assert_eq!(pos[2], [8.0, 8.0, 0.5]);
            assert_eq!(pos[3], [0.0, 8.0, 0.5]);
        } else {
            panic!("expected Float32x3 positions");
        }
    }

    #[test]
    fn alpha_scaled_by_fill() {
        let reg = test_fluid_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 0.5); // water, half full

        let mesh = build_fluid_mesh(&fluids, 0, 0, 2, 8.0, &reg).expect("should produce a mesh");

        if let Some(bevy::mesh::VertexAttributeValues::Float32x4(cols)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        {
            // water color: [64, 128, 255, 200], fill = 0.5
            // expected alpha = (200/255) * 0.5 ≈ 0.39216
            let expected_alpha = (200.0 / 255.0) * 0.5;
            for c in cols {
                assert!((c[0] - 64.0 / 255.0).abs() < 1e-5, "red mismatch");
                assert!((c[1] - 128.0 / 255.0).abs() < 1e-5, "green mismatch");
                assert!((c[2] - 255.0 / 255.0).abs() < 1e-5, "blue mismatch");
                assert!((c[3] - expected_alpha).abs() < 1e-5, "alpha mismatch");
            }
        } else {
            panic!("expected Float32x4 colors");
        }
    }

    #[test]
    fn full_chunk_vertex_count() {
        let reg = test_fluid_registry();
        let chunk_size = 4u32;
        let total = (chunk_size * chunk_size) as usize;
        let fluids = vec![FluidCell::new(FluidId(1), 1.0); total];

        let mesh =
            build_fluid_mesh(&fluids, 0, 0, chunk_size, 8.0, &reg).expect("should produce a mesh");

        if let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        {
            assert_eq!(pos.len(), total * 4, "each cell = 4 vertices");
        } else {
            panic!("expected Float32x3 positions");
        }

        if let Some(Indices::U32(idx)) = mesh.indices() {
            assert_eq!(idx.len(), total * 6, "each cell = 6 indices");
        } else {
            panic!("expected U32 indices");
        }
    }

    #[test]
    fn chunk_offset_positions_correct() {
        let reg = test_fluid_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 1.0);

        // chunk at (1, 2), chunk_size=2, tile_size=8
        // base_x = 1*2 = 2, base_y = 2*2 = 4
        // world_x = 2.0 * 8.0 = 16.0, world_y = 4.0 * 8.0 = 32.0
        let mesh = build_fluid_mesh(&fluids, 1, 2, 2, 8.0, &reg).expect("should produce a mesh");

        if let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        {
            assert_eq!(pos[0], [16.0, 32.0, 0.5]);
            assert_eq!(pos[1], [24.0, 32.0, 0.5]);
            assert_eq!(pos[2], [24.0, 40.0, 0.5]);
            assert_eq!(pos[3], [16.0, 40.0, 0.5]);
        } else {
            panic!("expected Float32x3 positions");
        }
    }

    #[test]
    fn mass_clamped_to_one_for_fill() {
        let reg = test_fluid_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        // Pressurized cell: mass > 1.0 should still fill the full tile
        fluids[0] = FluidCell::new(FluidId(1), 2.5);

        let mesh = build_fluid_mesh(&fluids, 0, 0, 2, 8.0, &reg).expect("should produce a mesh");

        if let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        {
            // fill = min(2.5, 1.0) = 1.0 → full tile height
            assert_eq!(pos[0], [0.0, 0.0, 0.5]);
            assert_eq!(pos[2], [8.0, 8.0, 0.5]);
        } else {
            panic!("expected Float32x3 positions");
        }
    }

    // --- UV_0 attribute tests ---

    #[test]
    fn uv0_contains_fill_and_depth() {
        let reg = test_fluid_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 0.7); // water, 70% full

        let mesh = build_fluid_mesh(&fluids, 0, 0, 2, 8.0, &reg).expect("should produce a mesh");

        if let Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        {
            assert_eq!(uvs.len(), 4, "1 quad = 4 UV vertices");
            for uv in uvs {
                assert!((uv[0] - 0.7).abs() < 1e-5, "fill_level should be 0.7");
                // Single cell with no fluid above → depth = 0.0 (surface)
                assert!(
                    (uv[1] - 0.0).abs() < 1e-5,
                    "depth should be 0.0 for surface cell"
                );
            }
        } else {
            panic!("expected Float32x2 UVs");
        }
    }

    // --- Surface detection tests ---

    #[test]
    fn liquid_surface_detected_when_above_empty() {
        // 2×2 chunk layout (y increases upward):
        //   row 1: empty, empty
        //   row 0: water, empty
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 1.0); // (0,0) water

        assert!(
            is_liquid_surface(&fluids, 0, 0, 2, FluidId(1)),
            "cell (0,0) should be surface: above (0,1) is empty"
        );
    }

    #[test]
    fn liquid_not_surface_when_same_fluid_above() {
        // 2×2 chunk:
        //   row 1: water, empty
        //   row 0: water, empty
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 1.0); // (0,0)
        fluids[2] = FluidCell::new(FluidId(1), 1.0); // (0,1)

        assert!(
            !is_liquid_surface(&fluids, 0, 0, 2, FluidId(1)),
            "cell (0,0) should NOT be surface: same fluid above"
        );
    }

    #[test]
    fn liquid_surface_at_top_edge() {
        // 2×2 chunk: water at (0,1) — top row, above is out-of-bounds
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[2] = FluidCell::new(FluidId(1), 1.0); // (0,1)

        assert!(
            is_liquid_surface(&fluids, 0, 1, 2, FluidId(1)),
            "top-row cell should be surface (out-of-bounds above)"
        );
    }

    #[test]
    fn gas_surface_detected_when_below_empty() {
        // 2×2 chunk:
        //   row 1: steam, empty
        //   row 0: empty, empty
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[2] = FluidCell::new(FluidId(2), 1.0); // (0,1) steam

        assert!(
            is_gas_surface(&fluids, 0, 1, 2, FluidId(2)),
            "gas cell (0,1) should be surface: below (0,0) is empty"
        );
    }

    #[test]
    fn gas_not_surface_when_same_fluid_below() {
        // 2×2 chunk:
        //   row 1: steam, empty
        //   row 0: steam, empty
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(2), 1.0); // (0,0)
        fluids[2] = FluidCell::new(FluidId(2), 1.0); // (0,1)

        assert!(
            !is_gas_surface(&fluids, 0, 1, 2, FluidId(2)),
            "gas cell (0,1) should NOT be surface: same fluid below"
        );
    }

    #[test]
    fn gas_surface_at_bottom_edge() {
        // 2×2 chunk: steam at (0,0) — bottom row, below is out-of-bounds
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(2), 1.0); // (0,0)

        assert!(
            is_gas_surface(&fluids, 0, 0, 2, FluidId(2)),
            "bottom-row gas cell should be surface (out-of-bounds below)"
        );
    }

    // --- Depth calculation tests ---

    #[test]
    fn depth_zero_for_surface_liquid() {
        // 4×1 column: water at (0,0), empty above
        let mut fluids = vec![FluidCell::EMPTY; 16]; // 4×4
        fluids[0] = FluidCell::new(FluidId(1), 1.0); // (0,0)

        let depth = compute_depth(&fluids, 0, 0, 4, FluidId(1), false);
        assert!(
            depth.abs() < 1e-5,
            "surface liquid should have depth 0.0, got {depth}"
        );
    }

    #[test]
    fn depth_increases_for_deeper_liquid() {
        // 4×4 chunk: column of water at x=0, y=0..3
        let mut fluids = vec![FluidCell::EMPTY; 16];
        for y in 0..4u32 {
            fluids[(y * 4) as usize] = FluidCell::new(FluidId(1), 1.0);
        }

        // y=3 is top (surface) → depth = 0
        let d3 = compute_depth(&fluids, 0, 3, 4, FluidId(1), false);
        assert!(d3.abs() < 1e-5, "top cell should have depth 0.0");

        // y=2 → 1 cell from surface → depth = 1/16
        let d2 = compute_depth(&fluids, 0, 2, 4, FluidId(1), false);
        assert!(
            (d2 - 1.0 / 16.0).abs() < 1e-5,
            "expected depth 1/16, got {d2}"
        );

        // y=0 → 3 cells from surface → depth = 3/16
        let d0 = compute_depth(&fluids, 0, 0, 4, FluidId(1), false);
        assert!(
            (d0 - 3.0 / 16.0).abs() < 1e-5,
            "expected depth 3/16, got {d0}"
        );
    }

    #[test]
    fn depth_for_gas_scans_downward() {
        // 4×4 chunk: column of steam at x=0, y=0..3
        let mut fluids = vec![FluidCell::EMPTY; 16];
        for y in 0..4u32 {
            fluids[(y * 4) as usize] = FluidCell::new(FluidId(2), 1.0);
        }

        // y=0 is bottom (surface for gas) → depth = 0
        let d0 = compute_depth(&fluids, 0, 0, 4, FluidId(2), true);
        assert!(d0.abs() < 1e-5, "bottom gas cell should have depth 0.0");

        // y=1 → 1 cell from surface → depth = 1/16
        let d1 = compute_depth(&fluids, 0, 1, 4, FluidId(2), true);
        assert!(
            (d1 - 1.0 / 16.0).abs() < 1e-5,
            "expected depth 1/16, got {d1}"
        );

        // y=3 → 3 cells from surface → depth = 3/16
        let d3 = compute_depth(&fluids, 0, 3, 4, FluidId(2), true);
        assert!(
            (d3 - 3.0 / 16.0).abs() < 1e-5,
            "expected depth 3/16, got {d3}"
        );
    }

    // --- Emission data tests ---

    #[test]
    fn emission_data_for_non_emissive_fluid() {
        let reg = test_fluid_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 1.0); // water: emission [0,0,0]

        let mesh = build_fluid_mesh(&fluids, 0, 0, 2, 8.0, &reg).expect("should produce a mesh");

        if let Some(bevy::mesh::VertexAttributeValues::Float32x4(data)) =
            mesh.attribute(ATTRIBUTE_FLUID_DATA)
        {
            assert_eq!(data.len(), 4);
            for d in data {
                assert!((d[0]).abs() < 1e-5, "emission_r should be 0");
                assert!((d[1]).abs() < 1e-5, "emission_g should be 0");
                assert!((d[2]).abs() < 1e-5, "emission_b should be 0");
            }
        } else {
            panic!("expected Float32x4 fluid data");
        }
    }

    #[test]
    fn emission_data_for_emissive_fluid() {
        let reg = test_fluid_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(3), 1.0); // lava: emission [255, 100, 20]

        let mesh = build_fluid_mesh(&fluids, 0, 0, 2, 8.0, &reg).expect("should produce a mesh");

        if let Some(bevy::mesh::VertexAttributeValues::Float32x4(data)) =
            mesh.attribute(ATTRIBUTE_FLUID_DATA)
        {
            assert_eq!(data.len(), 4);
            for d in data {
                assert!(
                    (d[0] - 255.0 / 255.0).abs() < 1e-5,
                    "emission_r should be 1.0"
                );
                assert!(
                    (d[1] - 100.0 / 255.0).abs() < 1e-5,
                    "emission_g should be ~0.392"
                );
                assert!(
                    (d[2] - 20.0 / 255.0).abs() < 1e-5,
                    "emission_b should be ~0.078"
                );
            }
        } else {
            panic!("expected Float32x4 fluid data");
        }
    }

    #[test]
    fn wave_flags_on_liquid_surface_top_vertices() {
        let reg = test_fluid_registry();
        // 2×2 chunk: water at (0,0), nothing above → surface
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 1.0);

        let mesh = build_fluid_mesh(&fluids, 0, 0, 2, 8.0, &reg).expect("should produce a mesh");

        if let Some(bevy::mesh::VertexAttributeValues::Float32x4(data)) =
            mesh.attribute(ATTRIBUTE_FLUID_DATA)
        {
            // Vertex order: 0=BL, 1=BR, 2=TR, 3=TL
            // Liquid surface: wave on top vertices (2, 3) → flags includes 1.0
            // is_gas = false → no +2.0
            assert!(
                (data[0][3]).abs() < 1e-5,
                "bottom-left should have no wave flag"
            );
            assert!(
                (data[1][3]).abs() < 1e-5,
                "bottom-right should have no wave flag"
            );
            assert!(
                (data[2][3] - 1.0).abs() < 1e-5,
                "top-right should have wave flag = 1.0"
            );
            assert!(
                (data[3][3] - 1.0).abs() < 1e-5,
                "top-left should have wave flag = 1.0"
            );
        } else {
            panic!("expected Float32x4 fluid data");
        }
    }

    #[test]
    fn wave_flags_on_gas_surface_bottom_vertices() {
        let reg = test_fluid_registry();
        // 2×2 chunk: steam at (0,1), nothing below at (0,0) → surface
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[2] = FluidCell::new(FluidId(2), 1.0); // (0,1) steam

        let mesh = build_fluid_mesh(&fluids, 0, 0, 2, 8.0, &reg).expect("should produce a mesh");

        if let Some(bevy::mesh::VertexAttributeValues::Float32x4(data)) =
            mesh.attribute(ATTRIBUTE_FLUID_DATA)
        {
            // Gas surface: wave on bottom vertices (0, 1) → flags = 1.0 + 2.0 = 3.0
            // Non-wave gas vertices → flags = 0.0 + 2.0 = 2.0
            assert!(
                (data[0][3] - 3.0).abs() < 1e-5,
                "bottom-left gas surface should have flags = 3.0 (wave + gas)"
            );
            assert!(
                (data[1][3] - 3.0).abs() < 1e-5,
                "bottom-right gas surface should have flags = 3.0 (wave + gas)"
            );
            assert!(
                (data[2][3] - 2.0).abs() < 1e-5,
                "top-right gas should have flags = 2.0 (gas only)"
            );
            assert!(
                (data[3][3] - 2.0).abs() < 1e-5,
                "top-left gas should have flags = 2.0 (gas only)"
            );
        } else {
            panic!("expected Float32x4 fluid data");
        }
    }

    #[test]
    fn no_wave_flags_on_non_surface_liquid() {
        let reg = test_fluid_registry();
        // 2×2 chunk: water column at x=0 (both rows filled)
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 1.0); // (0,0)
        fluids[2] = FluidCell::new(FluidId(1), 1.0); // (0,1)

        let mesh = build_fluid_mesh(&fluids, 0, 0, 2, 8.0, &reg).expect("should produce a mesh");

        if let Some(bevy::mesh::VertexAttributeValues::Float32x4(data)) =
            mesh.attribute(ATTRIBUTE_FLUID_DATA)
        {
            // First quad is (0,0) — NOT surface (water above at (0,1))
            // All 4 vertices should have flags = 0.0
            for i in 0..4 {
                assert!(
                    (data[i][3]).abs() < 1e-5,
                    "non-surface liquid vertex {i} should have flags = 0.0, got {}",
                    data[i][3]
                );
            }
        } else {
            panic!("expected Float32x4 fluid data");
        }
    }

    #[test]
    fn mesh_has_all_four_attributes() {
        let reg = test_fluid_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 1.0);

        let mesh = build_fluid_mesh(&fluids, 0, 0, 2, 8.0, &reg).expect("should produce a mesh");

        assert!(
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).is_some(),
            "missing POSITION"
        );
        assert!(
            mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_some(),
            "missing COLOR"
        );
        assert!(
            mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some(),
            "missing UV_0"
        );
        assert!(
            mesh.attribute(ATTRIBUTE_FLUID_DATA).is_some(),
            "missing FLUID_DATA"
        );
        assert!(mesh.indices().is_some(), "missing indices");
    }
}
