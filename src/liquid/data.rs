use serde::{Deserialize, Serialize};

/// Index into the liquid registry. 0 = no liquid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct LiquidId(pub u8);

impl LiquidId {
    pub const NONE: LiquidId = LiquidId(0);

    pub fn is_none(self) -> bool {
        self.0 == 0
    }
}

/// Per-tile liquid state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LiquidCell {
    pub liquid_type: LiquidId,
    pub level: f32,
}

impl LiquidCell {
    pub const EMPTY: LiquidCell = LiquidCell {
        liquid_type: LiquidId::NONE,
        level: 0.0,
    };

    pub fn is_empty(&self) -> bool {
        self.liquid_type.is_none() || self.level < MIN_LEVEL
    }
}

// ---------------------------------------------------------------------------
// jgallant / Starbound cellular-automata constants
// ---------------------------------------------------------------------------

/// Minimum level; cells below this are considered empty and zeroed out.
pub const MIN_LEVEL: f32 = 0.005;
/// Normal cell capacity (1 full tile of liquid).
pub const MAX_LEVEL: f32 = 1.0;
/// Extra liquid a bottom cell can hold beyond its top neighbor.
/// Creates the implicit pressure gradient that drives communicating vessels.
pub const MAX_COMPRESSION: f32 = 0.25;
/// Minimum flow threshold for applying the speed multiplier.
pub const MIN_FLOW: f32 = 0.005;
/// Maximum flow per cell per iteration.
pub const MAX_SPEED: f32 = 4.0;
/// Base flow speed multiplier (0.0–1.0). Applied when flow > MIN_FLOW.
pub const FLOW_SPEED: f32 = 1.0;
/// Fraction of lighter liquid displaced upward per tick when adjacent to
/// a denser liquid horizontally. Smooths staircase boundaries.
pub const DISPLACEMENT_RATE: f32 = 0.15;

// ---------------------------------------------------------------------------
// Core algorithm functions (shared by system.rs and simulation.rs)
// ---------------------------------------------------------------------------

/// How much liquid the **bottom** cell should hold given the combined amount
/// between two vertically-adjacent cells.
///
/// This is jgallant's `CalculateVerticalFlowValue`. The bottom cell is allowed
/// to hold up to `MAX_COMPRESSION` more than the top, creating a natural
/// pressure gradient without explicit pressure propagation.
pub fn vertical_flow_target(remaining: f32, dest_liquid: f32) -> f32 {
    let sum = remaining + dest_liquid;
    if sum <= MAX_LEVEL {
        MAX_LEVEL
    } else if sum < 2.0 * MAX_LEVEL + MAX_COMPRESSION {
        (MAX_LEVEL * MAX_LEVEL + sum * MAX_COMPRESSION) / (MAX_LEVEL + MAX_COMPRESSION)
    } else {
        (sum + MAX_COMPRESSION) / 2.0
    }
}

/// Clamp a computed flow value to the valid range `[0, min(MAX_SPEED, remaining)]`.
#[inline]
pub fn constrain_flow(flow: f32, remaining: f32) -> f32 {
    flow.max(0.0).min(MAX_SPEED).min(remaining)
}

// ---------------------------------------------------------------------------
// Liquid layer (per-chunk storage)
// ---------------------------------------------------------------------------

/// Per-chunk liquid storage. Row-major: local_y * chunk_size + local_x.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidLayer {
    pub cells: Vec<LiquidCell>,
}

impl LiquidLayer {
    pub fn new_empty(len: usize) -> Self {
        Self {
            cells: vec![LiquidCell::EMPTY; len],
        }
    }

    /// Serde default for backwards compatibility with old saves that lack a
    /// `liquid` field. Produces an empty layer with zero cells; the chunk
    /// loading code will resize if needed.
    pub fn serde_default() -> Self {
        Self { cells: Vec::new() }
    }

    pub fn get(&self, local_x: u32, local_y: u32, chunk_size: u32) -> LiquidCell {
        self.cells[(local_y * chunk_size + local_x) as usize]
    }

    pub fn set(&mut self, local_x: u32, local_y: u32, cell: LiquidCell, chunk_size: u32) {
        self.cells[(local_y * chunk_size + local_x) as usize] = cell;
    }

    pub fn has_liquid(&self) -> bool {
        self.cells.iter().any(|c| !c.is_empty())
    }
}
