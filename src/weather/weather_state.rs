use bevy::prelude::*;
use rand::Rng;

use crate::registry::world::ActiveWorld;

const RAMP_SPEED: f32 = 0.2;

#[derive(Debug, Clone, PartialEq)]
pub enum WeatherPhase {
    Clear,
    Precipitation,
}

#[derive(Resource)]
pub struct WeatherState {
    pub phase: WeatherPhase,
    pub intensity: f32,
    pub target_intensity: f32,
    pub duration: f32,
    pub elapsed: f32,
    pub cooldown: f32,
    pub check_timer: f32,
    pub precipitation_seed: u32,
}

impl Default for WeatherState {
    fn default() -> Self {
        let mut rng = rand::thread_rng();
        Self {
            phase: WeatherPhase::Clear,
            intensity: 0.0,
            target_intensity: 0.0,
            duration: 0.0,
            elapsed: 0.0,
            cooldown: rng.gen_range(60.0..180.0),
            check_timer: 5.0,
            precipitation_seed: 0,
        }
    }
}

impl WeatherState {
    pub fn intensity(&self) -> f32 {
        self.intensity
    }

    pub fn is_precipitating(&self) -> bool {
        self.phase == WeatherPhase::Precipitation
    }
}

pub fn update_weather(
    mut state: ResMut<WeatherState>,
    time: Res<Time>,
    world: Res<ActiveWorld>,
) {
    let dt = time.delta_secs();

    let Some(config) = &world.weather_config else {
        return;
    };

    match state.phase {
        WeatherPhase::Clear => {
            if state.cooldown > 0.0 {
                state.cooldown -= dt;
                return;
            }

            state.check_timer -= dt;
            if state.check_timer > 0.0 {
                return;
            }
            state.check_timer = 5.0;

            let mut rng = rand::thread_rng();
            if rng.r#gen::<f32>() < config.precipitation_chance {
                let duration = rng.gen_range(
                    config.precipitation_duration.0..config.precipitation_duration.1,
                );
                let target_intensity = rng.gen_range(0.5..1.0);
                state.phase = WeatherPhase::Precipitation;
                state.intensity = 0.0;
                state.target_intensity = target_intensity;
                state.elapsed = 0.0;
                state.duration = duration;
                state.precipitation_seed = rng.r#gen::<u32>();
            }
        }
        WeatherPhase::Precipitation => {
            state.elapsed += dt;

            if state.elapsed >= state.duration {
                state.target_intensity = 0.0;
            }

            let diff = state.target_intensity - state.intensity;
            if diff.abs() < 0.001 && state.target_intensity == 0.0 {
                let mut rng = rand::thread_rng();
                state.phase = WeatherPhase::Clear;
                state.intensity = 0.0;
                state.cooldown = rng.gen_range(config.cooldown.0..config.cooldown.1);
                state.check_timer = 5.0;
            } else {
                state.intensity += diff.signum() * RAMP_SPEED * dt;
                state.intensity = state.intensity.clamp(0.0, 1.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_starts_clear() {
        let state = WeatherState::default();
        assert_eq!(state.phase, WeatherPhase::Clear);
        assert_eq!(state.intensity(), 0.0);
        assert!(!state.is_precipitating());
    }

    #[test]
    fn is_precipitating_when_precipitation_phase() {
        let mut state = WeatherState::default();
        state.phase = WeatherPhase::Precipitation;
        state.intensity = 0.5;
        assert!(state.is_precipitating());
    }
}
