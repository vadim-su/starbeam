/// Compact fluid type identifier. Index into FluidRegistry.defs.
/// 0 = no fluid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FluidId(pub u8);

impl FluidId {
    pub const NONE: FluidId = FluidId(0);
}

/// Per-tile fluid state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FluidCell {
    /// Which fluid occupies this tile (NONE = empty).
    pub fluid_id: FluidId,
    /// Fluid amount: 0 = empty, 255 = full tile.
    pub level: u8,
}

impl FluidCell {
    pub fn is_empty(&self) -> bool {
        self.fluid_id == FluidId::NONE || self.level == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty() {
        let cell = FluidCell::default();
        assert!(cell.is_empty());
        assert_eq!(cell.fluid_id, FluidId::NONE);
        assert_eq!(cell.level, 0);
    }

    #[test]
    fn non_empty_cell() {
        let cell = FluidCell {
            fluid_id: FluidId(1),
            level: 128,
        };
        assert!(!cell.is_empty());
    }

    #[test]
    fn zero_level_is_empty() {
        let cell = FluidCell {
            fluid_id: FluidId(1),
            level: 0,
        };
        assert!(cell.is_empty());
    }
}
