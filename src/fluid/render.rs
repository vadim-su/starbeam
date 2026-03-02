use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use super::cell::FluidCell;
use super::registry::FluidRegistry;

/// Z-position for fluid quads: between tiles (z=0) and entities.
const FLUID_Z: f32 = 0.5;

/// Build a Bevy `Mesh` for the fluid layer of a single chunk.
///
/// Each non-empty fluid cell becomes a colored quad whose height reflects
/// the fill level (`min(mass, 1.0)`). Liquids fill bottom-up; gases fill
/// top-down. Returns `None` when the chunk contains no visible fluids.
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

            let vi = positions.len() as u32;

            positions.extend_from_slice(&[
                [world_x, y0, FLUID_Z],
                [world_x + tile_size, y0, FLUID_Z],
                [world_x + tile_size, y1, FLUID_Z],
                [world_x, y1, FLUID_Z],
            ]);

            colors.extend_from_slice(&[color, color, color, color]);

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
}
