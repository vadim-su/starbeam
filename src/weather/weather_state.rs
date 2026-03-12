use bevy::prelude::*;
use rand::Rng;

use crate::registry::biome::BiomeRegistry;
use crate::registry::world::ActiveWorld;
use crate::world::biome_map::BiomeMap;
use crate::world::day_night::WorldTime;

/// Speed at which snow intensity ramps up and down per second.
const RAMP_SPEED: f32 = 0.2;

/// The current weather condition.
#[derive(Debug, Clone)]
pub enum WeatherKind {
    Clear,
    Snowing {
        intensity: f32,
        target_intensity: f32,
        elapsed: f32,
        duration: f32,
    },
}

/// Resource tracking the current weather state.
#[derive(Resource)]
pub struct WeatherState {
    pub current: WeatherKind,
    /// Cooldown after a snow event ends before another can start.
    pub cooldown_timer: f32,
    /// Periodic check timer for rolling snow probability.
    pub check_timer: f32,
}

impl Default for WeatherState {
    fn default() -> Self {
        let mut rng = rand::thread_rng();
        Self {
            current: WeatherKind::Clear,
            cooldown_timer: rng.gen_range(60.0..180.0),
            check_timer: 5.0,
        }
    }
}

impl WeatherState {
    /// Returns the current snow intensity (0.0 if not snowing).
    pub fn intensity(&self) -> f32 {
        match &self.current {
            WeatherKind::Clear => 0.0,
            WeatherKind::Snowing { intensity, .. } => *intensity,
        }
    }

    /// Returns true if it is currently snowing.
    pub fn is_snowing(&self) -> bool {
        matches!(self.current, WeatherKind::Snowing { .. })
    }
}

/// System that drives weather state transitions based on biome and temperature.
pub fn update_weather(
    mut state: ResMut<WeatherState>,
    time: Res<Time>,
    camera_q: Query<&Transform, With<Camera2d>>,
    world: Res<ActiveWorld>,
    biome_map: Res<BiomeMap>,
    biome_registry: Res<BiomeRegistry>,
    world_time: Res<WorldTime>,
) {
    let dt = time.delta_secs();

    match &mut state.current {
        WeatherKind::Clear => {
            // Tick cooldown
            if state.cooldown_timer > 0.0 {
                state.cooldown_timer -= dt;
                return;
            }

            // Periodic probability check
            state.check_timer -= dt;
            if state.check_timer > 0.0 {
                return;
            }
            state.check_timer = 5.0;

            // Get biome at camera center
            let Ok(cam_tf) = camera_q.single() else {
                return;
            };
            let tile_x = (cam_tf.translation.x / world.tile_size) as i32;
            let wrapped_x = world.wrap_tile_x(tile_x).max(0) as u32;
            let biome_id = biome_map.biome_at(wrapped_x);
            let biome = biome_registry.get(biome_id);

            // Roll for snow
            let probability =
                biome.snow_base_chance * (1.0 - world_time.temperature_modifier);
            let mut rng = rand::thread_rng();
            if rng.r#gen::<f32>() < probability {
                let duration = rng.gen_range(30.0..120.0);
                let target_intensity = rng.gen_range(0.5..1.0);
                state.current = WeatherKind::Snowing {
                    intensity: 0.0,
                    target_intensity,
                    elapsed: 0.0,
                    duration,
                };
            }
        }
        WeatherKind::Snowing {
            intensity,
            target_intensity,
            elapsed,
            duration,
        } => {
            *elapsed += dt;

            // When duration is reached, start ramping down
            if *elapsed >= *duration {
                *target_intensity = 0.0;
            }

            // Lerp intensity toward target
            let diff = *target_intensity - *intensity;
            if diff.abs() < 0.001 && *target_intensity == 0.0 {
                // Fully ramped down — transition to Clear
                let mut rng = rand::thread_rng();
                state.current = WeatherKind::Clear;
                state.cooldown_timer = rng.gen_range(60.0..180.0);
                state.check_timer = 5.0;
            } else {
                *intensity += diff.signum() * RAMP_SPEED * dt;
                *intensity = intensity.clamp(0.0, 1.0);
            }
        }
    }
}
