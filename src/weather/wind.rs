use bevy::prelude::*;
use rand::Rng;

/// Maximum wind speed in pixels per second.
pub const MAX_WIND_SPEED: f32 = 60.0;

/// Wind resource — controls ambient wind direction and strength.
#[derive(Resource)]
pub struct Wind {
    /// Current wind direction in radians.
    pub direction: f32,
    /// Current wind strength in 0..1 range.
    pub strength: f32,
    /// Target direction the wind is lerping toward.
    pub target_direction: f32,
    /// Target strength the wind is lerping toward.
    pub target_strength: f32,
    /// Timer until the next target change.
    pub change_timer: f32,
}

impl Default for Wind {
    fn default() -> Self {
        let mut rng = rand::thread_rng();
        let dir = rng.gen_range(0.0..std::f32::consts::TAU);
        let str_ = rng.gen_range(0.2..0.8);
        Self {
            direction: dir,
            strength: str_,
            target_direction: dir,
            target_strength: str_,
            change_timer: rng.gen_range(5.0..15.0),
        }
    }
}

impl Wind {
    /// Returns the wind velocity as a Vec2.
    /// The Y component is scaled down by 0.3 so wind is mostly horizontal.
    pub fn velocity(&self) -> Vec2 {
        let speed = self.strength * MAX_WIND_SPEED;
        Vec2::new(
            self.direction.cos() * speed,
            self.direction.sin() * speed * 0.3,
        )
    }
}

/// System that smoothly updates wind direction and strength over time.
pub fn update_wind(mut wind: ResMut<Wind>, time: Res<Time>) {
    let dt = time.delta_secs();

    wind.change_timer -= dt;
    if wind.change_timer <= 0.0 {
        let mut rng = rand::thread_rng();
        // Pick a new target within ±60° of the current direction
        let offset = rng.gen_range(-std::f32::consts::FRAC_PI_3..std::f32::consts::FRAC_PI_3);
        wind.target_direction = wind.direction + offset;
        wind.target_strength = rng.gen_range(0.1..1.0);
        wind.change_timer = rng.gen_range(5.0..15.0);
    }

    // Lerp strength toward target
    let coeff = 0.02;
    wind.strength += (wind.target_strength - wind.strength) * coeff;

    // Lerp direction toward target with angle wrapping
    let mut diff = wind.target_direction - wind.direction;
    // Wrap angle difference to [-PI, PI]
    diff = (diff + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    wind.direction += diff * coeff;
}
