use serde::{Deserialize, Serialize};

/// Compact fluid type identifier. Index into FluidRegistry.defs.
/// 0 = no fluid (empty cell).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct FluidId(pub u8);

impl FluidId {
    pub const NONE: FluidId = FluidId(0);
}

/// A single slot holding one fluid type and its mass.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct FluidSlot {
    pub fluid_id: FluidId,
    /// Mass of fluid in this slot. 0.0 = empty, 1.0 = full, >1.0 = pressurized.
    pub mass: f32,
}

impl FluidSlot {
    pub const EMPTY: FluidSlot = FluidSlot {
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

/// A cell that can hold up to two fluids: primary (heavier, bottom) and secondary (lighter, top).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct FluidCell {
    pub primary: FluidSlot,
    pub secondary: FluidSlot,
}

impl FluidCell {
    pub const EMPTY: FluidCell = FluidCell {
        primary: FluidSlot::EMPTY,
        secondary: FluidSlot::EMPTY,
    };

    /// Create a single-fluid cell (placed in primary slot).
    pub fn new(fluid_id: FluidId, mass: f32) -> Self {
        Self {
            primary: FluidSlot::new(fluid_id, mass),
            secondary: FluidSlot::EMPTY,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.primary.is_empty() && self.secondary.is_empty()
    }

    pub fn total_mass(&self) -> f32 {
        let p = if self.primary.is_empty() { 0.0 } else { self.primary.mass };
        let s = if self.secondary.is_empty() { 0.0 } else { self.secondary.mass };
        p + s
    }

    /// Backward-compat: returns primary fluid_id.
    pub fn fluid_id(&self) -> FluidId {
        self.primary.fluid_id
    }

    /// Backward-compat: returns primary mass.
    pub fn mass(&self) -> f32 {
        self.primary.mass
    }

    /// Check if either slot contains the given fluid.
    pub fn has_fluid(&self, fid: FluidId) -> bool {
        (!self.primary.is_empty() && self.primary.fluid_id == fid)
            || (!self.secondary.is_empty() && self.secondary.fluid_id == fid)
    }

    /// Get an immutable reference to the slot containing the given fluid, if any.
    pub fn slot_for(&self, fid: FluidId) -> Option<&FluidSlot> {
        if !self.primary.is_empty() && self.primary.fluid_id == fid {
            Some(&self.primary)
        } else if !self.secondary.is_empty() && self.secondary.fluid_id == fid {
            Some(&self.secondary)
        } else {
            None
        }
    }

    /// Get a mutable reference to the slot containing the given fluid, if any.
    pub fn slot_for_mut(&mut self, fid: FluidId) -> Option<&mut FluidSlot> {
        if !self.primary.is_empty() && self.primary.fluid_id == fid {
            Some(&mut self.primary)
        } else if !self.secondary.is_empty() && self.secondary.fluid_id == fid {
            Some(&mut self.secondary)
        } else {
            None
        }
    }

    /// Clean up empty slots: clear dead slots and promote secondary to primary if needed.
    pub fn normalize(&mut self) {
        if self.primary.is_empty() && !self.secondary.is_empty() {
            self.primary = self.secondary;
            self.secondary = FluidSlot::EMPTY;
        }
        // Clear slots that have mass <= 0 to fully empty state
        if self.primary.is_empty() {
            self.primary = FluidSlot::EMPTY;
        }
        if self.secondary.is_empty() {
            self.secondary = FluidSlot::EMPTY;
        }
    }

    /// Swap primary and secondary if secondary is denser (heavier fluid should be on bottom / primary).
    /// `density_fn` maps FluidId -> density value.
    pub fn enforce_density_order(&mut self, density_fn: impl Fn(FluidId) -> f32) {
        if self.primary.is_empty() || self.secondary.is_empty() {
            return;
        }
        let dp = density_fn(self.primary.fluid_id);
        let ds = density_fn(self.secondary.fluid_id);
        if ds > dp {
            core::mem::swap(&mut self.primary, &mut self.secondary);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_cell_is_empty() {
        let cell = FluidCell::EMPTY;
        assert!(cell.is_empty());
        assert_eq!(cell.fluid_id(), FluidId::NONE);
        assert_eq!(cell.mass(), 0.0);
        assert_eq!(cell.total_mass(), 0.0);
    }

    #[test]
    fn single_fluid_cell() {
        let cell = FluidCell::new(FluidId(1), 0.5);
        assert!(!cell.is_empty());
        assert_eq!(cell.fluid_id(), FluidId(1));
        assert!((cell.mass() - 0.5).abs() < f32::EPSILON);
        assert!(cell.secondary.is_empty());
    }

    #[test]
    fn total_mass_both_slots() {
        let cell = FluidCell {
            primary: FluidSlot::new(FluidId(1), 0.6),
            secondary: FluidSlot::new(FluidId(2), 0.3),
        };
        assert!((cell.total_mass() - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn has_fluid_checks_both_slots() {
        let cell = FluidCell {
            primary: FluidSlot::new(FluidId(1), 0.5),
            secondary: FluidSlot::new(FluidId(2), 0.3),
        };
        assert!(cell.has_fluid(FluidId(1)));
        assert!(cell.has_fluid(FluidId(2)));
        assert!(!cell.has_fluid(FluidId(3)));
        assert!(!cell.has_fluid(FluidId::NONE));
    }

    #[test]
    fn normalize_moves_secondary_to_primary() {
        let mut cell = FluidCell {
            primary: FluidSlot::EMPTY,
            secondary: FluidSlot::new(FluidId(2), 0.4),
        };
        cell.normalize();
        assert_eq!(cell.primary.fluid_id, FluidId(2));
        assert!((cell.primary.mass - 0.4).abs() < f32::EPSILON);
        assert!(cell.secondary.is_empty());
    }

    #[test]
    fn enforce_density_swaps_when_needed() {
        // Primary has lighter fluid (density 1.0), secondary has heavier (density 2.0)
        let mut cell = FluidCell {
            primary: FluidSlot::new(FluidId(1), 0.5),
            secondary: FluidSlot::new(FluidId(2), 0.3),
        };
        cell.enforce_density_order(|fid| match fid.0 {
            1 => 1.0,
            2 => 2.0,
            _ => 0.0,
        });
        // After swap: heavier fluid (id=2) should be primary
        assert_eq!(cell.primary.fluid_id, FluidId(2));
        assert_eq!(cell.secondary.fluid_id, FluidId(1));
    }

    #[test]
    fn fluid_cell_serialization_roundtrip() {
        let cell = FluidCell {
            primary: FluidSlot::new(FluidId(2), 1.5),
            secondary: FluidSlot::new(FluidId(3), 0.7),
        };
        let serialized = ron::to_string(&cell).unwrap();
        let deserialized: FluidCell = ron::from_str(&serialized).unwrap();
        assert_eq!(deserialized.primary.fluid_id, FluidId(2));
        assert!((deserialized.primary.mass - 1.5).abs() < f32::EPSILON);
        assert_eq!(deserialized.secondary.fluid_id, FluidId(3));
        assert!((deserialized.secondary.mass - 0.7).abs() < f32::EPSILON);
    }
}
