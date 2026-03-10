use bevy::prelude::*;

#[derive(Component, Debug)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Health {
    pub fn new(max: f32) -> Self {
        Self { current: max, max }
    }

    pub fn take_damage(&mut self, amount: f32) {
        self.current = (self.current - amount).max(0.0);
    }

    pub fn heal(&mut self, amount: f32) {
        self.current = (self.current + amount).min(self.max);
    }

    pub fn is_dead(&self) -> bool {
        self.current <= 0.0
    }

    pub fn ratio(&self) -> f32 {
        if self.max == 0.0 {
            return 0.0;
        }
        self.current / self.max
    }
}

#[derive(Component, Debug)]
pub struct InvincibilityTimer {
    pub remaining: f32,
}

impl InvincibilityTimer {
    pub fn new(duration: f32) -> Self {
        Self { remaining: duration }
    }
}
