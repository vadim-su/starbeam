use bevy::prelude::*;

#[derive(Debug, Clone)]
pub struct Particle {
    pub position: Vec2,
    pub velocity: Vec2,
    /// Max lifetime in seconds.
    pub lifetime: f32,
    /// Current age in seconds.
    pub age: f32,
    /// Visual radius (world units).
    pub size: f32,
    /// RGBA colour.
    pub color: [f32; 4],
    pub alive: bool,
    /// Gravity multiplier: 1.0 = normal, -0.3 = bubbles float up slowly.
    pub gravity_scale: f32,
    /// Whether alpha fades to 0 as particle ages.
    pub fade_out: bool,
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
