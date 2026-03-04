use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::cell::FluidCell;
use super::registry::FluidRegistry;

/// Z-position for fluid quads: between tiles (z=0) and entities.
const FLUID_Z: f32 = 0.5;

// ─────────────────────────────────────────────────────────────────────────────
// Cross-chunk surface height helpers (used by wave system)
// ─────────────────────────────────────────────────────────────────────────────

/// Compute the liquid surface height for a single column in **local tile units**.
///
/// Scans top-down to find the highest liquid cell. Returns `local_y + fill`
/// (sub-tile precision), or `None` if the column has no liquid.
///
/// Used by `fluid_rebuild_meshes` to extract the edge column surface height
/// from horizontally adjacent chunks so that surface-vertex smoothing can
/// cross chunk boundaries.
pub fn column_liquid_surface_h(
    fluids: &[FluidCell],
    local_x: u32,
    chunk_size: u32,
    fluid_registry: &FluidRegistry,
) -> Option<f32> {
    for local_y in (0..chunk_size).rev() {
        let idx = (local_y * chunk_size + local_x) as usize;
        let cell = &fluids[idx];
        if !cell.is_empty() {
            let def = fluid_registry.get(cell.fluid_id);
            if !def.is_gas {
                return Some(local_y as f32 + cell.mass.min(1.0));
            }
        }
    }
    None
}

/// Compute the gas surface height for a single column in **local tile units**.
///
/// Scans bottom-up to find the lowest gas cell. Returns `local_y + (1 - fill)`
/// (sub-tile precision), or `None` if the column has no gas.
pub fn column_gas_surface_h(
    fluids: &[FluidCell],
    local_x: u32,
    chunk_size: u32,
    fluid_registry: &FluidRegistry,
) -> Option<f32> {
    for local_y in 0..chunk_size {
        let idx = (local_y * chunk_size + local_x) as usize;
        let cell = &fluids[idx];
        if !cell.is_empty() {
            let def = fluid_registry.get(cell.fluid_id);
            if def.is_gas {
                return Some(local_y as f32 + (1.0 - cell.mass.min(1.0)));
            }
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Metaball density-texture rendering
// ─────────────────────────────────────────────────────────────────────────────

/// Build density and fluid_id textures for a chunk's fluid data.
///
/// Returns `(density_data, fluid_id_data, tex_size)` where tex_size = chunk_size + 2.
/// The textures include 1-cell padding from neighboring chunks for seamless
/// metaball field evaluation at chunk boundaries.
/// Returns None if the chunk has no fluid at all.
pub fn build_fluid_textures(
    fluids: &[FluidCell],
    chunk_size: u32,
    neighbor_left: Option<&[FluidCell]>,
    neighbor_right: Option<&[FluidCell]>,
    neighbor_above: Option<&[FluidCell]>,
    neighbor_below: Option<&[FluidCell]>,
) -> Option<(Vec<u8>, Vec<u8>, u32)> {
    if !fluids.iter().any(|c| !c.is_empty()) {
        return None;
    }

    let tex_size = chunk_size + 2;
    let total = (tex_size * tex_size) as usize;
    let mut density = vec![0u8; total];
    let mut fluid_id = vec![0u8; total];

    // Fill center region (offset by 1 for padding)
    for ly in 0..chunk_size {
        for lx in 0..chunk_size {
            let src_idx = (ly * chunk_size + lx) as usize;
            let dst_idx = ((ly + 1) * tex_size + (lx + 1)) as usize;
            let cell = &fluids[src_idx];
            if !cell.is_empty() {
                density[dst_idx] = (cell.mass.clamp(0.0, 1.0) * 255.0) as u8;
                fluid_id[dst_idx] = cell.fluid_id.0;
            }
        }
    }

    // Left padding (rightmost column of left neighbor)
    if let Some(left) = neighbor_left {
        for ly in 0..chunk_size {
            let src_idx = (ly * chunk_size + (chunk_size - 1)) as usize;
            let dst_idx = ((ly + 1) * tex_size) as usize;
            let cell = &left[src_idx];
            if !cell.is_empty() {
                density[dst_idx] = (cell.mass.clamp(0.0, 1.0) * 255.0) as u8;
                fluid_id[dst_idx] = cell.fluid_id.0;
            }
        }
    }

    // Right padding (leftmost column of right neighbor)
    if let Some(right) = neighbor_right {
        for ly in 0..chunk_size {
            let src_idx = (ly * chunk_size) as usize;
            let dst_idx = ((ly + 1) * tex_size + tex_size - 1) as usize;
            let cell = &right[src_idx];
            if !cell.is_empty() {
                density[dst_idx] = (cell.mass.clamp(0.0, 1.0) * 255.0) as u8;
                fluid_id[dst_idx] = cell.fluid_id.0;
            }
        }
    }

    // Bottom padding (top row of below neighbor)
    if let Some(below) = neighbor_below {
        for lx in 0..chunk_size {
            let src_idx = ((chunk_size - 1) * chunk_size + lx) as usize;
            let dst_idx = (lx + 1) as usize;
            let cell = &below[src_idx];
            if !cell.is_empty() {
                density[dst_idx] = (cell.mass.clamp(0.0, 1.0) * 255.0) as u8;
                fluid_id[dst_idx] = cell.fluid_id.0;
            }
        }
    }

    // Top padding (bottom row of above neighbor)
    if let Some(above) = neighbor_above {
        for lx in 0..chunk_size {
            let src_idx = lx as usize;
            let dst_idx = ((tex_size - 1) * tex_size + (lx + 1)) as usize;
            let cell = &above[src_idx];
            if !cell.is_empty() {
                density[dst_idx] = (cell.mass.clamp(0.0, 1.0) * 255.0) as u8;
                fluid_id[dst_idx] = cell.fluid_id.0;
            }
        }
    }

    Some((density, fluid_id, tex_size))
}

/// Create a Bevy Image from raw R8Unorm data.
pub fn make_r8_texture(data: Vec<u8>, width: u32, height: u32) -> Image {
    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Build a static quad mesh covering a chunk's world-space area.
pub fn build_chunk_quad(chunk_x: i32, chunk_y: i32, chunk_size: u32, tile_size: f32) -> Mesh {
    let world_x = chunk_x as f32 * chunk_size as f32 * tile_size;
    let world_y = chunk_y as f32 * chunk_size as f32 * tile_size;
    let size = chunk_size as f32 * tile_size;

    let positions = vec![
        [world_x, world_y, FLUID_Z],
        [world_x + size, world_y, FLUID_Z],
        [world_x + size, world_y + size, FLUID_Z],
        [world_x, world_y + size, FLUID_Z],
    ];
    let uvs: Vec<[f32; 2]> = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let indices = vec![0u32, 1, 2, 0, 2, 3];

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
