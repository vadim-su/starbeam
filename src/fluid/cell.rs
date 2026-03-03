use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Compact fluid type identifier. Index into FluidRegistry.defs.
/// 0 = no fluid (empty cell).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct FluidId(pub u8);

impl FluidId {
    pub const NONE: FluidId = FluidId(0);
}

/// A single cell of fluid/gas data.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct FluidCell {
    pub fluid_id: FluidId,
    /// Mass of fluid in this cell. 0.0 = empty, 1.0 = full, >1.0 = pressurized.
    pub mass: f32,
}

impl FluidCell {
    pub const EMPTY: FluidCell = FluidCell {
        fluid_id: FluidId::NONE,
        mass: 0.0,
    };

    pub fn new(fluid_id: FluidId, mass: f32) -> Self {
        Self { fluid_id, mass }
    }

    pub fn is_empty(&self) -> bool {
        self.fluid_id == FluidId::NONE || self.mass <= 0.0
    }
}

/// Tracks whether an entity was in fluid last frame.
#[derive(Component, Default)]
pub struct FluidContactState {
    pub last_fluid: FluidId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_cell_is_empty() {
        let cell = FluidCell::EMPTY;
        assert!(cell.is_empty());
        assert_eq!(cell.fluid_id, FluidId::NONE);
        assert_eq!(cell.mass, 0.0);
    }

    #[test]
    fn cell_with_fluid_is_not_empty() {
        let cell = FluidCell::new(FluidId(1), 0.5);
        assert!(!cell.is_empty());
    }

    #[test]
    fn cell_with_zero_mass_is_empty() {
        let cell = FluidCell::new(FluidId(1), 0.0);
        assert!(cell.is_empty());
    }

    #[test]
    fn fluid_id_none_is_zero() {
        assert_eq!(FluidId::NONE, FluidId(0));
    }

    #[test]
    fn fluid_cell_serialization_roundtrip() {
        let cell = FluidCell::new(FluidId(2), 1.5);
        let serialized = ron::to_string(&cell).unwrap();
        let deserialized: FluidCell = ron::from_str(&serialized).unwrap();
        assert_eq!(deserialized.fluid_id, FluidId(2));
        assert!((deserialized.mass - 1.5).abs() < f32::EPSILON);
    }
}
