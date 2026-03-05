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

/// Minimum level below which a cell is considered empty.
pub const MIN_LEVEL: f32 = 0.001;
/// Maximum level a cell can hold.
pub const MAX_LEVEL: f32 = 1.0;
/// Maximum flow per face per step.
pub const MAX_FLOW: f32 = 0.5;

/// Flow state for a single cell — not persisted, recomputed each frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct FlowCell {
    pub flow: [f32; 4], // [right, up, left, down]
}

/// Face indices.
pub const FACE_RIGHT: usize = 0;
pub const FACE_UP: usize = 1;
pub const FACE_LEFT: usize = 2;
pub const FACE_DOWN: usize = 3;

/// Opposite face lookup.
pub const OPPOSITE_FACE: [usize; 4] = [FACE_LEFT, FACE_DOWN, FACE_RIGHT, FACE_UP];

/// Tile offsets for each face direction.
pub const FACE_OFFSET: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

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
