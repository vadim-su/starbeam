use bevy::prelude::*;

use crate::registry::assets::WeatherConfig;
use crate::registry::biome::BiomeRegistry;
use crate::registry::world::ActiveWorld;
use crate::weather::temperature;
use crate::weather::weather_state::WeatherState;
use crate::world::biome_map::BiomeMap;
use crate::world::day_night::WorldTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrecipitationType {
    Snow,
    Rain,
    Fog,
    Sandstorm,
}

impl PrecipitationType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "snow" => Some(Self::Snow),
            "rain" => Some(Self::Rain),
            "fog" => Some(Self::Fog),
            "sandstorm" => Some(Self::Sandstorm),
            _ => None,
        }
    }
}

pub fn resolve_precipitation_type(
    local_temp: f32,
    config: &WeatherConfig,
    seed: u32,
) -> Option<PrecipitationType> {
    let matching: Vec<PrecipitationType> = config
        .types
        .iter()
        .filter(|entry| local_temp >= entry.temp_min && local_temp < entry.temp_max)
        .filter_map(|entry| PrecipitationType::from_str(&entry.kind))
        .collect();

    if matching.is_empty() {
        return None;
    }

    let index = (seed as usize) % matching.len();
    Some(matching[index])
}

#[derive(Resource)]
pub struct ResolvedWeatherType(pub Option<PrecipitationType>);

pub fn resolve_weather_type_system(
    mut commands: Commands,
    weather: Res<WeatherState>,
    world: Res<ActiveWorld>,
    world_time: Res<WorldTime>,
    biome_map: Res<BiomeMap>,
    biome_registry: Res<BiomeRegistry>,
    camera_q: Query<&Transform, With<Camera2d>>,
) {
    if !weather.is_precipitating() {
        commands.insert_resource(ResolvedWeatherType(None));
        return;
    }

    let Ok(cam_tf) = camera_q.single() else {
        commands.insert_resource(ResolvedWeatherType(None));
        return;
    };

    let Some(config) = &world.weather_config else {
        commands.insert_resource(ResolvedWeatherType(None));
        return;
    };

    let tile_x = (cam_tf.translation.x / world.tile_size) as i32;
    let local_temp = temperature::local_temperature(
        tile_x, &world, &world_time, &biome_map, &biome_registry,
    );

    let resolved = resolve_precipitation_type(local_temp, config, weather.precipitation_seed);
    commands.insert_resource(ResolvedWeatherType(resolved));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::assets::{WeatherConfig, WeatherTypeEntry};

    fn test_config() -> WeatherConfig {
        WeatherConfig {
            precipitation_chance: 0.3,
            precipitation_duration: (60.0, 180.0),
            cooldown: (60.0, 300.0),
            types: vec![
                WeatherTypeEntry { kind: "snow".into(), temp_min: f32::NEG_INFINITY, temp_max: 0.0 },
                WeatherTypeEntry { kind: "rain".into(), temp_min: 0.0, temp_max: 35.0 },
                WeatherTypeEntry { kind: "fog".into(), temp_min: 5.0, temp_max: 20.0 },
                WeatherTypeEntry { kind: "sandstorm".into(), temp_min: 30.0, temp_max: f32::INFINITY },
            ],
        }
    }

    #[test]
    fn resolve_snow_below_zero() {
        let config = test_config();
        assert_eq!(
            resolve_precipitation_type(-10.0, &config, 0),
            Some(PrecipitationType::Snow),
        );
    }

    #[test]
    fn resolve_rain_at_warm_temp() {
        let config = test_config();
        assert_eq!(
            resolve_precipitation_type(25.0, &config, 0),
            Some(PrecipitationType::Rain),
        );
    }

    #[test]
    fn resolve_overlap_selects_deterministically() {
        let config = test_config();
        let r0 = resolve_precipitation_type(10.0, &config, 0);
        let r1 = resolve_precipitation_type(10.0, &config, 1);
        assert!(r0 == Some(PrecipitationType::Rain) || r0 == Some(PrecipitationType::Fog));
        assert!(r1 == Some(PrecipitationType::Rain) || r1 == Some(PrecipitationType::Fog));
        assert_eq!(r0, resolve_precipitation_type(10.0, &config, 0));
    }

    #[test]
    fn resolve_none_when_no_match() {
        let config = WeatherConfig {
            precipitation_chance: 0.3,
            precipitation_duration: (60.0, 180.0),
            cooldown: (60.0, 300.0),
            types: vec![
                WeatherTypeEntry { kind: "snow".into(), temp_min: f32::NEG_INFINITY, temp_max: 0.0 },
            ],
        };
        assert_eq!(resolve_precipitation_type(20.0, &config, 0), None);
    }
}
