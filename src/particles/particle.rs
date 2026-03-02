use bevy::prelude::*;

use crate::fluid::cell::FluidId;

#[derive(Debug, Clone)]
pub struct Particle {
    pub position: Vec2,
    pub velocity: Vec2,
    /// Fluid mass carried (for CA reabsorption). 0 for visual-only.
    pub mass: f32,
    /// Which fluid. `FluidId::NONE` for non-fluid particles.
    pub fluid_id: FluidId,
    /// Max lifetime in seconds.
    pub lifetime: f32,
    /// Current age in seconds.
    pub age: f32,
    /// Visual radius (world units).
    pub size: f32,
    /// RGBA colour.
    pub color: [f32; 4],
    pub alive: bool,
}

impl Particle {
    pub fn is_dead(&self) -> bool {
        !self.alive || self.age >= self.lifetime
    }

    /// Returns `age / lifetime`, clamped to `[0, 1]`.
    pub fn age_ratio(&self) -> f32 {
        (self.age / self.lifetime).min(1.0)
    }
}
