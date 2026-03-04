use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, MeshVertexAttribute, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::VertexFormat;

use super::cell::{FluidCell, FluidId, FluidSlot};
use super::registry::FluidRegistry;
use crate::registry::tile::{TileId, TileRegistry};

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

/// Per-vertex dynamic wave height from wave propagation simulation.
pub const ATTRIBUTE_WAVE_HEIGHT: MeshVertexAttribute =
    MeshVertexAttribute::new("WaveHeight", 982301568, VertexFormat::Float32);

/// Per-vertex wave parameters from FluidDef: `[amplitude, speed]`.
/// Used by the shader to customise ripple strength and frequency per fluid.
pub const ATTRIBUTE_WAVE_PARAMS: MeshVertexAttribute =
    MeshVertexAttribute::new("WaveParams", 982301569, VertexFormat::Float32x2);

/// Per-vertex edge flags: bitflags indicating which sides border solid tiles or air.
/// Bit 0 = left solid, Bit 1 = right solid, Bit 2 = above is air/empty, Bit 3 = below solid.
/// Used by shader for shore foam effect.
pub const ATTRIBUTE_EDGE_FLAGS: MeshVertexAttribute =
    MeshVertexAttribute::new("EdgeFlags", 982301570, VertexFormat::Float32);

/// Determine whether a liquid cell is at the surface (exposed to air/gas above).
///
/// A liquid cell at `(local_x, local_y)` is "surface" when the cell directly
/// above `(local_x, local_y + 1)` is empty, out-of-bounds, or a gas. A different
/// **liquid** above means this cell is submerged, not a surface (prevents wave
/// displacement and surface effects at internal lava-water boundaries).
fn is_liquid_surface(
    fluids: &[FluidCell],
    local_x: u32,
    local_y: u32,
    chunk_size: u32,
    _fluid_id: super::cell::FluidId,
    neighbor_above_row: Option<&[FluidCell]>,
    fluid_registry: &FluidRegistry,
) -> bool {
    let above_y = local_y + 1;
    if above_y >= chunk_size {
        return match neighbor_above_row {
            Some(row) => {
                let above = &row[local_x as usize];
                above.is_empty() || fluid_registry.get(above.fluid_id()).is_gas
            }
            None => true,
        };
    }
    let above_idx = (above_y * chunk_size + local_x) as usize;
    let above = &fluids[above_idx];
    above.is_empty() || fluid_registry.get(above.fluid_id()).is_gas
}

/// Determine whether a gas cell is at the surface (exposed to air/liquid below).
///
/// A gas cell at `(local_x, local_y)` is "surface" when the cell directly
/// below `(local_x, local_y - 1)` is empty, out-of-bounds, or a liquid. A different
/// **gas** below means this cell is interior, not a surface.
fn is_gas_surface(
    fluids: &[FluidCell],
    local_x: u32,
    local_y: u32,
    chunk_size: u32,
    _fluid_id: super::cell::FluidId,
    neighbor_below_row: Option<&[FluidCell]>,
    fluid_registry: &FluidRegistry,
) -> bool {
    if local_y == 0 {
        return match neighbor_below_row {
            Some(row) => {
                let below = &row[local_x as usize];
                below.is_empty() || !fluid_registry.get(below.fluid_id()).is_gas
            }
            None => true,
        };
    }
    let below_idx = ((local_y - 1) * chunk_size + local_x) as usize;
    let below = &fluids[below_idx];
    below.is_empty() || !fluid_registry.get(below.fluid_id()).is_gas
}

/// Compute depth_in_fluid: normalized 0..1 (0 = surface, 1 = deepest).
///
/// For liquids: scan upward from cell to find the surface (max MAX_DEPTH_SCAN cells).
/// For gases: scan downward from cell to find the surface (max MAX_DEPTH_SCAN cells).
///
/// `neighbor_above_row`: bottom row of the chunk directly above (local_y=0 of upper chunk).
/// `neighbor_below_row`: top row of the chunk directly below (local_y=chunk_size-1 of lower chunk).
/// Used to determine whether fluid truly continues beyond the chunk boundary.
///
/// `chunk_base_y`: the absolute Y coordinate (in tiles) of this chunk's local_y=0.
///   Equals `chunk_y * chunk_size`.
///
/// `above_surface_world_y`: for liquids, the world-tile Y of the fluid surface in the
///   column's chunk above (= `(cy+1)*chunk_size + local_surface_y + fill`). When provided,
///   used to compute accurate depth for cells whose scan reaches the top boundary. Without
///   this, cells at the top boundary fallback to MAX_DEPTH_SCAN (may create a brightness
///   seam at the chunk boundary when the surface is within MAX_DEPTH_SCAN tiles above).
fn compute_depth(
    fluids: &[FluidCell],
    local_x: u32,
    local_y: u32,
    chunk_size: u32,
    fluid_id: super::cell::FluidId,
    is_gas: bool,
    neighbor_above_row: Option<&[FluidCell]>,
    neighbor_below_row: Option<&[FluidCell]>,
    chunk_base_y: i32,
    above_surface_world_y: Option<f32>,
) -> f32 {
    let mut distance: u32 = 0;
    let mut hit_chunk_boundary = false;

    if is_gas {
        // Scan downward to find surface
        let mut sy = local_y;
        while distance < MAX_DEPTH_SCAN {
            if sy == 0 {
                hit_chunk_boundary = true;
                break; // hit bottom boundary
            }
            sy -= 1;
            let idx = (sy * chunk_size + local_x) as usize;
            let neighbor = &fluids[idx];
            if neighbor.is_empty() || neighbor.fluid_id() != fluid_id {
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
                hit_chunk_boundary = true;
                break; // hit top boundary
            }
            let idx = (sy * chunk_size + local_x) as usize;
            let neighbor = &fluids[idx];
            if neighbor.is_empty() || neighbor.fluid_id() != fluid_id {
                break; // found surface
            }
            distance += 1;
        }
    }

    // When we hit a chunk boundary, compute accurate depth using the absolute
    // surface position when available, otherwise fall back to the old heuristic.
    if hit_chunk_boundary {
        if !is_gas {
            // Liquid: surface is somewhere in the chunk above.
            if let Some(surf_y) = above_surface_world_y {
                // Compute exact distance from this cell to the water surface.
                let cell_world_y = chunk_base_y as f32 + local_y as f32;
                let actual_dist = (surf_y - cell_world_y).max(0.0);
                distance = actual_dist.min(MAX_DEPTH_SCAN as f32) as u32;
            } else {
                // No surface data available: check if fluid continues above and
                // fall back to MAX_DEPTH_SCAN so the cell appears deep.
                let fluid_continues = neighbor_above_row.is_some_and(|row| {
                    let cell = &row[local_x as usize];
                    !cell.is_empty() && cell.fluid_id() == fluid_id
                });
                if fluid_continues {
                    distance = MAX_DEPTH_SCAN;
                }
                // else: no fluid above → distance stays as computed (surface cell).
            }
        } else {
            // Gas: surface is somewhere in the chunk below.
            let fluid_continues = neighbor_below_row.is_some_and(|row| {
                let cell = &row[local_x as usize];
                !cell.is_empty() && cell.fluid_id() == fluid_id
            });
            if fluid_continues {
                distance = MAX_DEPTH_SCAN;
            }
        }
    }

    distance as f32 / MAX_DEPTH_SCAN as f32
}

// ─────────────────────────────────────────────────────────────────────────────
// Cross-chunk surface height helpers
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
            let def = fluid_registry.get(cell.fluid_id());
            if !def.is_gas {
                return Some(local_y as f32 + cell.total_mass().min(1.0));
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
            let def = fluid_registry.get(cell.fluid_id());
            if def.is_gas {
                return Some(local_y as f32 + (1.0 - cell.total_mass().min(1.0)));
            }
        }
    }
    None
}

/// Compute edge flags for a fluid cell: which sides border solid tiles or open air.
///
/// Returns a bitmask as f32:
///   Bit 0 = left solid
///   Bit 1 = right solid
///   Bit 2 = above is air/empty (no fluid, no solid)
///   Bit 3 = below solid
fn compute_edge_flags(
    fluids: &[FluidCell],
    tiles: &[TileId],
    local_x: u32,
    local_y: u32,
    chunk_size: u32,
    tile_registry: &TileRegistry,
) -> f32 {
    let mut flags: u32 = 0;

    // Left
    if local_x == 0 {
        flags |= 1; // chunk boundary = treat as solid for foam
    } else {
        let left_idx = (local_y * chunk_size + local_x - 1) as usize;
        if tile_registry.is_solid(tiles[left_idx]) {
            flags |= 1;
        }
    }

    // Right
    if local_x + 1 >= chunk_size {
        flags |= 2; // chunk boundary
    } else {
        let right_idx = (local_y * chunk_size + local_x + 1) as usize;
        if tile_registry.is_solid(tiles[right_idx]) {
            flags |= 2;
        }
    }

    // Above is air/empty (for surface foam check)
    if local_y + 1 >= chunk_size {
        flags |= 4; // chunk boundary = open above
    } else {
        let above_idx = ((local_y + 1) * chunk_size + local_x) as usize;
        let above_fluid_empty = fluids[above_idx].is_empty();
        let above_tile_solid = tile_registry.is_solid(tiles[above_idx]);
        if above_fluid_empty && !above_tile_solid {
            flags |= 4; // open air above
        }
    }

    // Below solid
    if local_y == 0 {
        // chunk bottom boundary — don't set, bedrock handled separately
    } else {
        let below_idx = ((local_y - 1) * chunk_size + local_x) as usize;
        if tile_registry.is_solid(tiles[below_idx]) {
            flags |= 8;
        }
    }

    flags as f32
}

/// Compute absolute surface height and surface fluid ID for each column.
///
/// For liquids: scan top-down to find the highest liquid cell.
///   Surface height = `row + fill`. Returns `None` for empty/gas-only columns.
///
/// For gas: scan bottom-up to find the lowest gas cell.
///   Surface height = `row + (1.0 - fill)`.
///
/// The fluid ID is returned alongside the height so that surface smoothing
/// can skip interpolation between columns with different surface fluids
/// (e.g. water column next to lava column).
fn compute_column_surface_data(
    fluids: &[FluidCell],
    chunk_size: u32,
    fluid_registry: &FluidRegistry,
    is_gas_query: bool,
) -> Vec<Option<(f32, FluidId)>> {
    let mut data = vec![None; chunk_size as usize];

    for local_x in 0..chunk_size {
        if is_gas_query {
            for local_y in 0..chunk_size {
                let idx = (local_y * chunk_size + local_x) as usize;
                let cell = &fluids[idx];
                if !cell.is_empty() {
                    let def = fluid_registry.get(cell.fluid_id());
                    if def.is_gas {
                        let fill = cell.total_mass().min(1.0);
                        data[local_x as usize] =
                            Some((local_y as f32 + (1.0 - fill), cell.fluid_id()));
                        break;
                    }
                }
            }
        } else {
            for local_y in (0..chunk_size).rev() {
                let idx = (local_y * chunk_size + local_x) as usize;
                let cell = &fluids[idx];
                if !cell.is_empty() {
                    let def = fluid_registry.get(cell.fluid_id());
                    if !def.is_gas {
                        let fill = cell.total_mass().min(1.0);
                        data[local_x as usize] = Some((local_y as f32 + fill, cell.fluid_id()));
                        break;
                    }
                }
            }
        }
    }

    data
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
/// - `EDGE_FLAGS`: bitmask of which sides border solid tiles or open air
///
/// `left_edge_liquid_h` / `right_edge_liquid_h`: liquid surface height
/// (in local tile units) of the rightmost column of the left neighbour chunk
/// and the leftmost column of the right neighbour chunk respectively.
/// When provided, these are used to smooth the surface vertex heights at the
/// horizontal chunk boundary, eliminating the staircase seam that otherwise
/// appears where the within-chunk surface heights don't align.
///
/// `left_edge_gas_h` / `right_edge_gas_h`: same for gas columns.
///
/// `above_surface_world_ys`: per-column absolute world-tile Y of the liquid surface in
///   the chunk directly above this one. Used by `compute_depth` to compute accurate depth
///   for cells whose scan reaches the top boundary, preventing a brightness seam.
///   Length must equal `chunk_size` when `Some`. Values are `None` for columns with no
///   liquid in the chunk above.
#[allow(clippy::too_many_arguments)]
pub fn build_fluid_mesh(
    fluids: &[FluidCell],
    tiles: &[TileId],
    chunk_x: i32,
    chunk_y: i32,
    chunk_size: u32,
    tile_size: f32,
    fluid_registry: &FluidRegistry,
    tile_registry: &TileRegistry,
    neighbor_above_row: Option<&[FluidCell]>,
    neighbor_below_row: Option<&[FluidCell]>,
    wave_heights: Option<&[f32]>,
    left_edge_liquid_h: Option<f32>,
    right_edge_liquid_h: Option<f32>,
    left_edge_gas_h: Option<f32>,
    right_edge_gas_h: Option<f32>,
    above_surface_world_ys: Option<&[Option<f32>]>,
    // Per-column seed: true = column above is already in "covered" state.
    emission_cover_seed: Option<&[bool]>,
) -> Option<Mesh> {
    let capacity = fluids.len();
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(capacity * 4);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(capacity * 4);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(capacity * 4);
    let mut fluid_data: Vec<[f32; 4]> = Vec::with_capacity(capacity * 4);
    let mut wave_data: Vec<f32> = Vec::with_capacity(capacity * 4);
    let mut wave_params_data: Vec<[f32; 2]> = Vec::with_capacity(capacity * 4);
    let mut edge_flags_data: Vec<f32> = Vec::with_capacity(capacity * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(capacity * 6);

    let base_x = chunk_x * chunk_size as i32;
    let base_y = chunk_y * chunk_size as i32;
    let chunk_base_y = base_y; // absolute Y of local_y=0 in world tiles

    // Pre-compute per-column surface data (height + fluid ID) for smooth interpolation.
    let liquid_surface = compute_column_surface_data(fluids, chunk_size, fluid_registry, false);
    let gas_surface = compute_column_surface_data(fluids, chunk_size, fluid_registry, true);

    // Pre-compute per-cell emission coverage: a cell is "covered" when there
    // is a contiguous column of fluid above it that contains a different fluid
    // type.  E.g. all lava cells beneath a water body are marked covered so
    // their emission glow doesn't bleed through the semi-transparent water.
    let mut emission_covered = vec![false; (chunk_size * chunk_size) as usize];
    for lx in 0..chunk_size {
        // Start from the row above the chunk (cross-chunk neighbour).
        let mut top_fluid = neighbor_above_row
            .map(|row| row[lx as usize].fluid_id())
            .filter(|id| *id != FluidId::NONE)
            .unwrap_or(FluidId::NONE);
        // Seed from calling code: if the chunk(s) above already determined
        // that this column is in "covered" state, propagate it.
        let mut cover_active = emission_cover_seed
            .map(|seeds| seeds[lx as usize])
            .unwrap_or(false);

        // Iterate top-down within the chunk.
        for ly in (0..chunk_size).rev() {
            let cidx = (ly * chunk_size + lx) as usize;
            let cell = &fluids[cidx];
            if cell.is_empty() {
                top_fluid = FluidId::NONE;
                cover_active = false;
            } else {
                if top_fluid == FluidId::NONE {
                    top_fluid = cell.fluid_id();
                } else if cell.fluid_id() != top_fluid {
                    cover_active = true;
                }
                if cover_active {
                    emission_covered[cidx] = true;
                }
            }
        }
    }

    // Helper closure: emit one quad for a single fluid slot within a cell.
    // `slot` is the FluidSlot to render, `slot_y0`/`slot_y1` are the
    // pre-computed vertical extents in world coordinates.
    // `is_surface_slot` indicates whether this slot is at the fluid surface.
    // `suppress_emission` suppresses light emission (e.g. primary covered by secondary).
    let emit_quad = |slot: &FluidSlot,
                         slot_def: &super::registry::FluidDef,
                         slot_fill: f32,
                         slot_y0: f32,
                         slot_y1: f32,
                         is_surface_slot: bool,
                         suppress_emission: bool,
                         world_x: f32,
                         local_x: u32,
                         local_y: u32,
                         idx: usize,
                         positions: &mut Vec<[f32; 3]>,
                         colors: &mut Vec<[f32; 4]>,
                         uvs: &mut Vec<[f32; 2]>,
                         fluid_data: &mut Vec<[f32; 4]>,
                         wave_data: &mut Vec<f32>,
                         wave_params_data: &mut Vec<[f32; 2]>,
                         edge_flags_data: &mut Vec<f32>,
                         indices: &mut Vec<u32>| {
        let color = [
            slot_def.color[0] as f32 / 255.0,
            slot_def.color[1] as f32 / 255.0,
            slot_def.color[2] as f32 / 255.0,
            (slot_def.color[3] as f32 / 255.0) * slot_fill,
        ];

        let col_above_surface_world_y = above_surface_world_ys
            .and_then(|arr| arr.get(local_x as usize))
            .and_then(|v| *v);
        let depth = compute_depth(
            fluids,
            local_x,
            local_y,
            chunk_size,
            slot.fluid_id,
            slot_def.is_gas,
            neighbor_above_row,
            neighbor_below_row,
            chunk_base_y,
            col_above_surface_world_y,
        );
        let uv = [slot_fill, depth];

        let mut emission = [
            slot_def.light_emission[0] as f32 / 255.0,
            slot_def.light_emission[1] as f32 / 255.0,
            slot_def.light_emission[2] as f32 / 255.0,
        ];
        if suppress_emission || emission_covered[idx] {
            emission = [0.0, 0.0, 0.0];
        }

        let is_gas_flag = if slot_def.is_gas { 2.0 } else { 0.0 };

        let wave_flags: [f32; 4] = if is_surface_slot {
            if slot_def.is_gas {
                [1.0, 1.0, 0.0, 0.0]
            } else {
                [0.0, 0.0, 1.0, 1.0]
            }
        } else {
            [0.0, 0.0, 0.0, 0.0]
        };

        // Surface smoothing
        let (y0_left, y0_right, y1_left, y1_right) = if is_surface_slot {
            let surface_data = if slot_def.is_gas {
                &gas_surface
            } else {
                &liquid_surface
            };
            let (this_h, this_fid) = surface_data[local_x as usize]
                .unwrap_or((0.0, slot.fluid_id));

            let left_h: Option<f32> = if local_x > 0 {
                surface_data[(local_x - 1) as usize]
                    .filter(|(_, fid)| *fid == this_fid)
                    .map(|(h, _)| h)
            } else if slot_def.is_gas {
                left_edge_gas_h
            } else {
                left_edge_liquid_h
            };
            let right_h: Option<f32> = if local_x + 1 < chunk_size {
                surface_data[(local_x + 1) as usize]
                    .filter(|(_, fid)| *fid == this_fid)
                    .map(|(h, _)| h)
            } else if slot_def.is_gas {
                right_edge_gas_h
            } else {
                right_edge_liquid_h
            };

            let base = base_y as f32;

            if slot_def.is_gas {
                let y0_l = match left_h {
                    Some(lh) => (base + (this_h + lh) / 2.0) * tile_size,
                    None => slot_y0,
                };
                let y0_r = match right_h {
                    Some(rh) => (base + (this_h + rh) / 2.0) * tile_size,
                    None => slot_y0,
                };
                (y0_l, y0_r, slot_y1, slot_y1)
            } else {
                let y1_l = match left_h {
                    Some(lh) => (base + (this_h + lh) / 2.0) * tile_size,
                    None => slot_y1,
                };
                let y1_r = match right_h {
                    Some(rh) => (base + (this_h + rh) / 2.0) * tile_size,
                    None => slot_y1,
                };
                (slot_y0, slot_y0, y1_l, y1_r)
            }
        } else {
            (slot_y0, slot_y0, slot_y1, slot_y1)
        };

        let vi = positions.len() as u32;

        positions.extend_from_slice(&[
            [world_x, y0_left, FLUID_Z],
            [world_x + tile_size, y0_right, FLUID_Z],
            [world_x + tile_size, y1_right, FLUID_Z],
            [world_x, y1_left, FLUID_Z],
        ]);

        colors.extend_from_slice(&[color, color, color, color]);
        uvs.extend_from_slice(&[uv, uv, uv, uv]);

        for i in 0..4 {
            let flags = wave_flags[i] * 1.0 + is_gas_flag;
            fluid_data.push([emission[0], emission[1], emission[2], flags]);
        }

        let wave_h = wave_heights.map(|wh| wh[idx] * tile_size).unwrap_or(0.0);
        wave_data.extend_from_slice(&[wave_h, wave_h, wave_h, wave_h]);

        let wp = [slot_def.wave_amplitude, slot_def.wave_speed];
        wave_params_data.extend_from_slice(&[wp, wp, wp, wp]);

        let edge_flags =
            compute_edge_flags(fluids, tiles, local_x, local_y, chunk_size, tile_registry);
        edge_flags_data.extend_from_slice(&[edge_flags, edge_flags, edge_flags, edge_flags]);

        indices.extend_from_slice(&[vi, vi + 1, vi + 2, vi, vi + 2, vi + 3]);
    };

    for local_y in 0..chunk_size {
        for local_x in 0..chunk_size {
            let idx = (local_y * chunk_size + local_x) as usize;
            let cell = &fluids[idx];

            if cell.is_empty() {
                continue;
            }

            let world_x = (base_x + local_x as i32) as f32 * tile_size;
            let world_y = (base_y + local_y as i32) as f32 * tile_size;

            let has_secondary = !cell.secondary.is_empty();

            // --- Primary slot ---
            if !cell.primary.is_empty() {
                let primary_def = fluid_registry.get(cell.primary.fluid_id);
                let primary_fill = cell.primary.mass.min(1.0);

                let (p_y0, p_y1) = if primary_def.is_gas {
                    let y0 = world_y + (1.0 - primary_fill) * tile_size;
                    let y1 = world_y + tile_size;
                    (y0, y1)
                } else {
                    let y0 = world_y;
                    let y1 = world_y + primary_fill * tile_size;
                    (y0, y1)
                };

                // Primary is surface only if secondary is empty AND the cell-level
                // surface check passes (cell above is empty or gas).
                let primary_is_surface = !has_secondary && if primary_def.is_gas {
                    is_gas_surface(
                        fluids, local_x, local_y, chunk_size,
                        cell.primary.fluid_id, neighbor_below_row, fluid_registry,
                    )
                } else {
                    is_liquid_surface(
                        fluids, local_x, local_y, chunk_size,
                        cell.primary.fluid_id, neighbor_above_row, fluid_registry,
                    )
                };

                // Suppress primary emission when secondary covers it
                let suppress_primary_emission = has_secondary;

                emit_quad(
                    &cell.primary, primary_def, primary_fill,
                    p_y0, p_y1, primary_is_surface, suppress_primary_emission,
                    world_x, local_x, local_y, idx,
                    &mut positions, &mut colors, &mut uvs, &mut fluid_data,
                    &mut wave_data, &mut wave_params_data, &mut edge_flags_data,
                    &mut indices,
                );
            }

            // --- Secondary slot ---
            if has_secondary {
                let secondary_def = fluid_registry.get(cell.secondary.fluid_id);
                let primary_fill = cell.primary.mass.min(1.0);
                let secondary_fill = cell.secondary.mass.min(1.0 - primary_fill);

                if secondary_fill > 0.0 {
                    let s_y0 = world_y + primary_fill * tile_size;
                    let s_y1 = s_y0 + secondary_fill * tile_size;

                    // Secondary is surface if cell above is empty or gas
                    let secondary_is_surface = if secondary_def.is_gas {
                        is_gas_surface(
                            fluids, local_x, local_y, chunk_size,
                            cell.secondary.fluid_id, neighbor_below_row, fluid_registry,
                        )
                    } else {
                        is_liquid_surface(
                            fluids, local_x, local_y, chunk_size,
                            cell.secondary.fluid_id, neighbor_above_row, fluid_registry,
                        )
                    };

                    emit_quad(
                        &cell.secondary, secondary_def, secondary_fill,
                        s_y0, s_y1, secondary_is_surface, false,
                        world_x, local_x, local_y, idx,
                        &mut positions, &mut colors, &mut uvs, &mut fluid_data,
                        &mut wave_data, &mut wave_params_data, &mut edge_flags_data,
                        &mut indices,
                    );
                }
            }
        }
    }

    if positions.is_empty() {
        return None;
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(ATTRIBUTE_FLUID_DATA, fluid_data);
    mesh.insert_attribute(ATTRIBUTE_WAVE_HEIGHT, wave_data);
    mesh.insert_attribute(ATTRIBUTE_WAVE_PARAMS, wave_params_data);
    mesh.insert_attribute(ATTRIBUTE_EDGE_FLAGS, edge_flags_data);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::cell::{FluidCell, FluidId};
    use crate::fluid::registry::{FluidDef, FluidRegistry};
    use crate::registry::tile::TileId;
    use crate::test_helpers::fixtures::test_tile_registry;

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
                wave_amplitude: 1.0,
                wave_speed: 1.0,
                light_absorption: 0.3,
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
                wave_amplitude: 0.6,
                wave_speed: 1.5,
                light_absorption: 0.05,
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
                wave_amplitude: 0.4,
                wave_speed: 0.3,
                light_absorption: 0.8,
            },
        ])
    }

    #[test]
    fn empty_chunk_returns_none() {
        let reg = test_fluid_registry();
        let tile_reg = test_tile_registry();
        let fluids = vec![FluidCell::EMPTY; 4];
        let tiles = vec![TileId::AIR; 4];
        let result = build_fluid_mesh(
            &fluids, &tiles, 0, 0, 2, 8.0, &reg, &tile_reg, None, None, None, None, None, None,
            None, None, None,
        );
        assert!(result.is_none(), "all-empty chunk should return None");
    }

    #[test]
    fn single_liquid_cell_produces_quad() {
        let reg = test_fluid_registry();
        let tile_reg = test_tile_registry();
        // 2×2 chunk: one water cell at (0,0), rest empty
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 0.5); // water, half full
        let tiles = vec![TileId::AIR; 4];

        let mesh = build_fluid_mesh(
            &fluids, &tiles, 0, 0, 2, 8.0, &reg, &tile_reg, None, None, None, None, None, None,
            None, None, None,
        )
        .expect("should produce a mesh");

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
        let tile_reg = test_tile_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(2), 0.5); // steam, half full
        let tiles = vec![TileId::AIR; 4];

        let mesh = build_fluid_mesh(
            &fluids, &tiles, 0, 0, 2, 8.0, &reg, &tile_reg, None, None, None, None, None, None,
            None, None, None,
        )
        .expect("should produce a mesh");

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
        let tile_reg = test_tile_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 0.5); // water, half full
        let tiles = vec![TileId::AIR; 4];

        let mesh = build_fluid_mesh(
            &fluids, &tiles, 0, 0, 2, 8.0, &reg, &tile_reg, None, None, None, None, None, None,
            None, None, None,
        )
        .expect("should produce a mesh");

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
        let tile_reg = test_tile_registry();
        let chunk_size = 4u32;
        let total = (chunk_size * chunk_size) as usize;
        let fluids = vec![FluidCell::new(FluidId(1), 1.0); total];
        let tiles = vec![TileId::AIR; total];

        let mesh = build_fluid_mesh(
            &fluids, &tiles, 0, 0, chunk_size, 8.0, &reg, &tile_reg, None, None, None, None, None,
            None, None, None, None,
        )
        .expect("should produce a mesh");

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
        let tile_reg = test_tile_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 1.0);
        let tiles = vec![TileId::AIR; 4];

        // chunk at (1, 2), chunk_size=2, tile_size=8
        // base_x = 1*2 = 2, base_y = 2*2 = 4
        // world_x = 2.0 * 8.0 = 16.0, world_y = 4.0 * 8.0 = 32.0
        let mesh = build_fluid_mesh(
            &fluids, &tiles, 1, 2, 2, 8.0, &reg, &tile_reg, None, None, None, None, None, None,
            None, None, None,
        )
        .expect("should produce a mesh");

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
        let tile_reg = test_tile_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        // Pressurized cell: mass > 1.0 should still fill the full tile
        fluids[0] = FluidCell::new(FluidId(1), 2.5);
        let tiles = vec![TileId::AIR; 4];

        let mesh = build_fluid_mesh(
            &fluids, &tiles, 0, 0, 2, 8.0, &reg, &tile_reg, None, None, None, None, None, None,
            None, None, None,
        )
        .expect("should produce a mesh");

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
        let tile_reg = test_tile_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 0.7); // water, 70% full
        let tiles = vec![TileId::AIR; 4];

        let mesh = build_fluid_mesh(
            &fluids, &tiles, 0, 0, 2, 8.0, &reg, &tile_reg, None, None, None, None, None, None,
            None, None, None,
        )
        .expect("should produce a mesh");

        if let Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        {
            assert_eq!(uvs.len(), 4, "1 quad = 4 UV vertices");
            // UV_0 is now uniform [fill, depth] for all cells.
            // This is a surface cell with nothing above, so depth = 0.0.
            for uv in uvs.iter() {
                assert!((uv[0] - 0.7).abs() < 1e-5, "fill_level should be 0.7");
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
        let reg = test_fluid_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 1.0); // (0,0) water

        assert!(
            is_liquid_surface(&fluids, 0, 0, 2, FluidId(1), None, &reg),
            "cell (0,0) should be surface: above (0,1) is empty"
        );
    }

    #[test]
    fn liquid_not_surface_when_same_fluid_above() {
        let reg = test_fluid_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 1.0); // (0,0)
        fluids[2] = FluidCell::new(FluidId(1), 1.0); // (0,1)

        assert!(
            !is_liquid_surface(&fluids, 0, 0, 2, FluidId(1), None, &reg),
            "cell (0,0) should NOT be surface: same fluid above"
        );
    }

    #[test]
    fn liquid_not_surface_when_different_liquid_above() {
        let reg = test_fluid_registry();
        // Lava (FluidId(3)) below water (FluidId(1))
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(3), 1.0); // (0,0) lava
        fluids[2] = FluidCell::new(FluidId(1), 1.0); // (0,1) water

        assert!(
            !is_liquid_surface(&fluids, 0, 0, 2, FluidId(3), None, &reg),
            "lava below water should NOT be surface (submerged)"
        );
    }

    #[test]
    fn liquid_surface_when_gas_above() {
        let reg = test_fluid_registry();
        // Water below steam: water IS surface (gas doesn't submerge liquid)
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 1.0); // (0,0) water
        fluids[2] = FluidCell::new(FluidId(2), 1.0); // (0,1) steam

        assert!(
            is_liquid_surface(&fluids, 0, 0, 2, FluidId(1), None, &reg),
            "water below gas should be surface"
        );
    }

    #[test]
    fn liquid_not_surface_at_top_edge() {
        let reg = test_fluid_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[2] = FluidCell::new(FluidId(1), 1.0); // (0,1)

        assert!(
            is_liquid_surface(&fluids, 0, 1, 2, FluidId(1), None, &reg),
            "top-edge cell with no neighbor should be surface (open sky assumed)"
        );

        let neighbor_row: Vec<FluidCell> = vec![
            FluidCell::new(FluidId(1), 1.0),
            FluidCell::new(FluidId(1), 1.0),
        ];
        assert!(
            !is_liquid_surface(&fluids, 0, 1, 2, FluidId(1), Some(&neighbor_row), &reg),
            "top-edge with fluid-filled neighbor should NOT be surface"
        );
    }

    #[test]
    fn gas_surface_detected_when_below_empty() {
        let reg = test_fluid_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[2] = FluidCell::new(FluidId(2), 1.0); // (0,1) steam

        assert!(
            is_gas_surface(&fluids, 0, 1, 2, FluidId(2), None, &reg),
            "gas cell (0,1) should be surface: below (0,0) is empty"
        );
    }

    #[test]
    fn gas_not_surface_when_same_fluid_below() {
        let reg = test_fluid_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(2), 1.0); // (0,0)
        fluids[2] = FluidCell::new(FluidId(2), 1.0); // (0,1)

        assert!(
            !is_gas_surface(&fluids, 0, 1, 2, FluidId(2), None, &reg),
            "gas cell (0,1) should NOT be surface: same fluid below"
        );
    }

    #[test]
    fn gas_not_surface_at_bottom_edge() {
        let reg = test_fluid_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(2), 1.0); // (0,0)

        assert!(
            is_gas_surface(&fluids, 0, 0, 2, FluidId(2), None, &reg),
            "bottom-edge gas cell with no neighbor should be surface (open space assumed)"
        );

        let neighbor_row: Vec<FluidCell> = vec![
            FluidCell::new(FluidId(2), 1.0),
            FluidCell::new(FluidId(2), 1.0),
        ];
        assert!(
            !is_gas_surface(&fluids, 0, 0, 2, FluidId(2), Some(&neighbor_row), &reg),
            "bottom-edge gas with fluid-filled neighbor should NOT be surface"
        );
    }

    // --- Depth calculation tests ---

    #[test]
    fn depth_zero_for_surface_liquid() {
        // 4×1 column: water at (0,0), empty above
        let mut fluids = vec![FluidCell::EMPTY; 16]; // 4×4
        fluids[0] = FluidCell::new(FluidId(1), 1.0); // (0,0)

        let depth = compute_depth(&fluids, 0, 0, 4, FluidId(1), false, None, None, 0, None);
        assert!(
            depth.abs() < 1e-5,
            "surface liquid should have depth 0.0, got {depth}"
        );
    }

    #[test]
    fn depth_increases_for_deeper_liquid() {
        // 8×8 chunk: column of water at x=0, y=2..5 (not touching boundaries)
        let mut fluids = vec![FluidCell::EMPTY; 64];
        for y in 2..6u32 {
            fluids[(y * 8) as usize] = FluidCell::new(FluidId(1), 1.0);
        }

        // y=5 is top (surface: above y=6 is empty) → depth = 0
        let d5 = compute_depth(&fluids, 0, 5, 8, FluidId(1), false, None, None, 0, None);
        assert!(d5.abs() < 1e-5, "top cell should have depth 0.0");

        // y=4 → 1 cell from surface → depth = 1/16
        let d4 = compute_depth(&fluids, 0, 4, 8, FluidId(1), false, None, None, 0, None);
        assert!(
            (d4 - 1.0 / 16.0).abs() < 1e-5,
            "expected depth 1/16, got {d4}"
        );

        // y=2 → 3 cells from surface → depth = 3/16
        let d2 = compute_depth(&fluids, 0, 2, 8, FluidId(1), false, None, None, 0, None);
        assert!(
            (d2 - 3.0 / 16.0).abs() < 1e-5,
            "expected depth 3/16, got {d2}"
        );
    }

    #[test]
    fn depth_for_gas_scans_downward() {
        // 8×8 chunk: column of steam at x=0, y=2..5 (not touching boundaries)
        let mut fluids = vec![FluidCell::EMPTY; 64];
        for y in 2..6u32 {
            fluids[(y * 8) as usize] = FluidCell::new(FluidId(2), 1.0);
        }

        // y=2 is bottom of gas column (surface for gas: below y=1 is empty) → depth = 0
        let d2 = compute_depth(&fluids, 0, 2, 8, FluidId(2), true, None, None, 0, None);
        assert!(d2.abs() < 1e-5, "bottom gas cell should have depth 0.0");

        // y=3 → 1 cell from surface → depth = 1/16
        let d3 = compute_depth(&fluids, 0, 3, 8, FluidId(2), true, None, None, 0, None);
        assert!(
            (d3 - 1.0 / 16.0).abs() < 1e-5,
            "expected depth 1/16, got {d3}"
        );

        // y=5 → 3 cells from surface → depth = 3/16
        let d5 = compute_depth(&fluids, 0, 5, 8, FluidId(2), true, None, None, 0, None);
        assert!(
            (d5 - 3.0 / 16.0).abs() < 1e-5,
            "expected depth 3/16, got {d5}"
        );
    }

    // --- compute_depth boundary fix tests ---

    #[test]
    fn depth_at_top_boundary_surface_cell_is_zero_when_no_fluid_above() {
        // 4×4 chunk: water column at x=0, rows 0..=3 (top row = row 3).
        // No neighbor above → top row IS a surface cell → depth should be 0.
        let mut fluids = vec![FluidCell::EMPTY; 16];
        for y in 0..4u32 {
            fluids[(y * 4) as usize] = FluidCell::new(FluidId(1), 1.0);
        }

        // Without the fix, row 3 would hit the boundary and return depth=1.0.
        // With the fix and neighbor_above_row=None → no fluid above → depth=0.
        let d = compute_depth(&fluids, 0, 3, 4, FluidId(1), false, None, None, 0, None);
        assert!(
            d.abs() < 1e-5,
            "top row surface cell with no neighbour above should have depth 0.0, got {d}"
        );
    }

    #[test]
    fn depth_at_top_boundary_is_max_when_fluid_continues_above() {
        // 4×4 chunk: water column at x=0, rows 0..=3 (fully filled).
        // Neighbour above also has water → cell is deep → depth = 1.0.
        let mut fluids = vec![FluidCell::EMPTY; 16];
        for y in 0..4u32 {
            fluids[(y * 4) as usize] = FluidCell::new(FluidId(1), 1.0);
        }
        let neighbor_row = vec![
            FluidCell::new(FluidId(1), 1.0),
            FluidCell::EMPTY,
            FluidCell::EMPTY,
            FluidCell::EMPTY,
        ];

        let d = compute_depth(
            &fluids,
            0,
            3,
            4,
            FluidId(1),
            false,
            Some(&neighbor_row),
            None,
            0,
            None,
        );
        assert!(
            (d - 1.0).abs() < 1e-5,
            "top row with fluid-filled neighbour above should have depth 1.0, got {d}"
        );
    }

    #[test]
    fn depth_at_top_boundary_is_correct_when_different_fluid_above() {
        // 4×4 chunk: water at x=0, rows 0..=3.
        // Neighbour above has DIFFERENT fluid (steam) → liquid surface is here → depth ≈ 0.
        let mut fluids = vec![FluidCell::EMPTY; 16];
        for y in 0..4u32 {
            fluids[(y * 4) as usize] = FluidCell::new(FluidId(1), 1.0);
        }
        let neighbor_row = vec![
            FluidCell::new(FluidId(2), 1.0), // steam, not water
            FluidCell::EMPTY,
            FluidCell::EMPTY,
            FluidCell::EMPTY,
        ];

        let d = compute_depth(
            &fluids,
            0,
            3,
            4,
            FluidId(1),
            false,
            Some(&neighbor_row),
            None,
            0,
            None,
        );
        assert!(
            d.abs() < 1e-5,
            "top row with different-fluid neighbour above should have depth 0.0, got {d}"
        );
    }

    // --- Cross-chunk surface smoothing helper tests ---

    #[test]
    fn column_liquid_surface_h_basic() {
        let reg = test_fluid_registry();
        // 2×2 chunk: water at (0,0) fill=0.7
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 0.7);

        let h = column_liquid_surface_h(&fluids, 0, 2, &reg);
        assert!(h.is_some(), "should find liquid surface height");
        assert!(
            (h.unwrap() - 0.7).abs() < 1e-5,
            "surface height should be 0.7 (row 0 + fill 0.7), got {:?}",
            h
        );
    }

    #[test]
    fn column_liquid_surface_h_returns_none_for_empty_column() {
        let reg = test_fluid_registry();
        let fluids = vec![FluidCell::EMPTY; 4];

        let h = column_liquid_surface_h(&fluids, 0, 2, &reg);
        assert!(h.is_none(), "empty column should return None");
    }

    #[test]
    fn surface_smoothing_uses_right_neighbour_edge_height() {
        // 2×2 chunk with water at (1,0) fill=1.0 (surface height = 1.0).
        // Right neighbour's leftmost column has liquid height = 1.5.
        // Expected: top-right vertex of column 1 (rightmost) is averaged:
        //   (base + (1.0 + 1.5) / 2.0) * tile_size = (0 + 1.25) * 8.0 = 10.0
        let reg = test_fluid_registry();
        let tile_reg = test_tile_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[1] = FluidCell::new(FluidId(1), 1.0); // column 1, row 0, full
        let tiles = vec![TileId::AIR; 4];

        let right_edge_liquid_h = Some(1.5_f32);

        let mesh = build_fluid_mesh(
            &fluids,
            &tiles,
            0,
            0,
            2,
            8.0,
            &reg,
            &tile_reg,
            None,
            None,
            None,
            None,
            right_edge_liquid_h,
            None,
            None,
            None,
            None,
        )
        .expect("should produce mesh");

        if let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        {
            // Quad for (1,0): vertices 0=BL, 1=BR, 2=TR, 3=TL
            // top-right (index 2): world_x+tile_size=16, y1_right should be avg(1.0,1.5)*8=10.0
            let tr_y = pos[2][1];
            assert!(
                (tr_y - 10.0).abs() < 0.01,
                "top-right vertex y should be averaged with right-neighbour edge: expected 10.0, got {tr_y}"
            );
        } else {
            panic!("expected Float32x3 positions");
        }
    }

    // --- Emission data tests ---

    #[test]
    fn emission_data_for_non_emissive_fluid() {
        let reg = test_fluid_registry();
        let tile_reg = test_tile_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 1.0); // water: emission [0,0,0]
        let tiles = vec![TileId::AIR; 4];

        let mesh = build_fluid_mesh(
            &fluids, &tiles, 0, 0, 2, 8.0, &reg, &tile_reg, None, None, None, None, None, None,
            None, None, None,
        )
        .expect("should produce a mesh");

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
        let tile_reg = test_tile_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(3), 1.0); // lava: emission [255, 100, 20]
        let tiles = vec![TileId::AIR; 4];

        let mesh = build_fluid_mesh(
            &fluids, &tiles, 0, 0, 2, 8.0, &reg, &tile_reg, None, None, None, None, None, None,
            None, None, None,
        )
        .expect("should produce a mesh");

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
        let tile_reg = test_tile_registry();
        // 2×2 chunk: water at (0,0), nothing above → surface
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 1.0);
        let tiles = vec![TileId::AIR; 4];

        let mesh = build_fluid_mesh(
            &fluids, &tiles, 0, 0, 2, 8.0, &reg, &tile_reg, None, None, None, None, None, None,
            None, None, None,
        )
        .expect("should produce a mesh");

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
        let tile_reg = test_tile_registry();
        // 2×2 chunk: steam at (0,1), nothing below at (0,0) → surface
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[2] = FluidCell::new(FluidId(2), 1.0); // (0,1) steam
        let tiles = vec![TileId::AIR; 4];

        let mesh = build_fluid_mesh(
            &fluids, &tiles, 0, 0, 2, 8.0, &reg, &tile_reg, None, None, None, None, None, None,
            None, None, None,
        )
        .expect("should produce a mesh");

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
        let tile_reg = test_tile_registry();
        // 2×2 chunk: water column at x=0 (both rows filled)
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 1.0); // (0,0)
        fluids[2] = FluidCell::new(FluidId(1), 1.0); // (0,1)
        let tiles = vec![TileId::AIR; 4];

        let mesh = build_fluid_mesh(
            &fluids, &tiles, 0, 0, 2, 8.0, &reg, &tile_reg, None, None, None, None, None, None,
            None, None, None,
        )
        .expect("should produce a mesh");

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
        let tile_reg = test_tile_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 1.0);
        let tiles = vec![TileId::AIR; 4];

        let mesh = build_fluid_mesh(
            &fluids, &tiles, 0, 0, 2, 8.0, &reg, &tile_reg, None, None, None, None, None, None,
            None, None, None,
        )
        .expect("should produce a mesh");

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
        assert!(
            mesh.attribute(ATTRIBUTE_WAVE_PARAMS).is_some(),
            "missing WAVE_PARAMS"
        );
        assert!(mesh.indices().is_some(), "missing indices");
    }

    #[test]
    fn surface_vertices_smoothed_across_rows() {
        let reg = test_fluid_registry();
        let tile_reg = test_tile_registry();
        // 4×4 chunk: staircase pattern at the surface.
        //   x=0: row 0 full + row 1 fill=0.4 → surface_height = 1.4
        //   x=1: row 0 full                   → surface_height = 1.0
        //   x=2: row 0 fill=0.6               → surface_height = 0.6
        //   x=3: empty
        let mut fluids = vec![FluidCell::EMPTY; 16]; // 4×4
                                                     // Column 0: row 0 full, row 1 partial
        fluids[0 * 4 + 0] = FluidCell::new(FluidId(1), 1.0); // (0,0)
        fluids[1 * 4 + 0] = FluidCell::new(FluidId(1), 0.4); // (0,1) surface
                                                             // Column 1: row 0 full
        fluids[0 * 4 + 1] = FluidCell::new(FluidId(1), 1.0); // (1,0) surface
                                                             // Column 2: row 0 partial
        fluids[0 * 4 + 2] = FluidCell::new(FluidId(1), 0.6); // (2,0) surface
        let tiles = vec![TileId::AIR; 16];

        let mesh = build_fluid_mesh(
            &fluids, &tiles, 0, 0, 4, 8.0, &reg, &tile_reg, None, None, None, None, None, None,
            None, None, None,
        )
        .expect("should produce a mesh");

        // Surface heights (in tile units): col0=1.4, col1=1.0, col2=0.6
        // Interpolation (chunk base_y=0):
        //   col0 surface cell at (0,1): top-left = no left → 1.4*8=11.2
        //                                top-right = avg(1.4,1.0)=1.2 → 1.2*8=9.6
        //   col1 surface cell at (1,0): top-left = avg(1.0,1.4)=1.2 → 1.2*8=9.6
        //                                top-right = avg(1.0,0.6)=0.8 → 0.8*8=6.4
        //   col2 surface cell at (2,0): top-left = avg(0.6,1.0)=0.8 → 0.8*8=6.4
        //                                top-right = no right → 0.6*8=4.8

        if let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        {
            // Find the surface cell quads by checking Y positions.
            // With 4 cells total (col0 has 2, col1 has 1, col2 has 1),
            // quad order: (0,0), (1,0), (2,0), (0,1) — row-major scan.

            // Quad 3 = cell (0,1) surface: vertices 12..15
            let q3_tl = pos[15][1]; // top-left
            let q3_tr = pos[14][1]; // top-right
            assert!((q3_tl - 11.2).abs() < 0.01, "col0 surface TL: got {q3_tl}");
            assert!((q3_tr - 9.6).abs() < 0.01, "col0 surface TR: got {q3_tr}");

            // Quad 1 = cell (1,0) surface: vertices 4..7
            let q1_tl = pos[7][1]; // top-left
            let q1_tr = pos[6][1]; // top-right
            assert!((q1_tl - 9.6).abs() < 0.01, "col1 surface TL: got {q1_tl}");
            assert!((q1_tr - 6.4).abs() < 0.01, "col1 surface TR: got {q1_tr}");

            // Quad 2 = cell (2,0) surface: vertices 8..11
            let q2_tl = pos[11][1]; // top-left
            let q2_tr = pos[10][1]; // top-right
            assert!((q2_tl - 6.4).abs() < 0.01, "col2 surface TL: got {q2_tl}");
            assert!((q2_tr - 4.8).abs() < 0.01, "col2 surface TR: got {q2_tr}");
        } else {
            panic!("expected Float32x3 positions");
        }
    }

    #[test]
    fn wave_height_attribute_present() {
        let reg = test_fluid_registry();
        let tile_reg = test_tile_registry();
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 1.0);
        let tiles = vec![TileId::AIR; 4];
        let wave = vec![0.5; 4];

        let mesh = build_fluid_mesh(
            &fluids,
            &tiles,
            0,
            0,
            2,
            8.0,
            &reg,
            &tile_reg,
            None,
            None,
            Some(&wave),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("should produce a mesh");

        assert!(
            mesh.attribute(ATTRIBUTE_WAVE_HEIGHT).is_some(),
            "mesh should have WAVE_HEIGHT attribute"
        );
    }

    // --- Edge flags tests ---

    #[test]
    fn edge_flags_left_boundary_treated_as_solid() {
        // 2×2 chunk: water at (0,0) — left is chunk boundary → bit 0 set
        let tile_reg = test_tile_registry();
        let tiles = vec![TileId::AIR; 4]; // all air tiles
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 1.0);

        let flags = compute_edge_flags(&fluids, &tiles, 0, 0, 2, &tile_reg);
        assert!(
            (flags as u32 & 1) != 0,
            "left chunk boundary should set bit 0"
        );
    }

    #[test]
    fn edge_flags_above_air_sets_bit2() {
        // 2×2 chunk: water at (1,0) — above (1,1) is empty air → bit 2 set
        let tile_reg = test_tile_registry();
        let tiles = vec![TileId::AIR; 4];
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[1] = FluidCell::new(FluidId(1), 1.0); // (1,0) water, nothing above

        let flags = compute_edge_flags(&fluids, &tiles, 1, 0, 2, &tile_reg);
        assert!((flags as u32 & 4) != 0, "open air above should set bit 2");
    }

    #[test]
    fn mesh_has_edge_flags_attribute() {
        // Verify build_fluid_mesh includes ATTRIBUTE_EDGE_FLAGS
        let reg = test_fluid_registry();
        let tile_reg = test_tile_registry();
        let tiles = vec![TileId::AIR; 4];
        let mut fluids = vec![FluidCell::EMPTY; 4];
        fluids[0] = FluidCell::new(FluidId(1), 1.0);

        let mesh = build_fluid_mesh(
            &fluids, &tiles, 0, 0, 2, 8.0, &reg, &tile_reg, None, None, None, None, None, None,
            None, None, None,
        )
        .expect("should produce mesh");
        assert!(
            mesh.attribute(ATTRIBUTE_EDGE_FLAGS).is_some(),
            "mesh must have ATTRIBUTE_EDGE_FLAGS"
        );
    }
}
