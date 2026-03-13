use bevy::prelude::*;
use serde::Deserialize;

use crate::parallax::spawn::{ParallaxLayerConfig, ParallaxSkyLayer};

/// Day phase indices into the config arrays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DayPhase {
    Dawn = 0,
    Day = 1,
    Sunset = 2,
    Night = 3,
}

impl DayPhase {
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn next(self) -> Self {
        match self {
            Self::Dawn => Self::Day,
            Self::Day => Self::Sunset,
            Self::Sunset => Self::Night,
            Self::Night => Self::Dawn,
        }
    }
}

impl std::fmt::Display for DayPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dawn => write!(f, "Dawn"),
            Self::Day => write!(f, "Day"),
            Self::Sunset => write!(f, "Sunset"),
            Self::Night => write!(f, "Night"),
        }
    }
}

/// Configuration for day/night cycle, loaded from RON.
/// All arrays are ordered [dawn, day, sunset, night].
#[derive(Resource, Debug, Clone, Deserialize)]
pub struct DayNightConfig {
    pub cycle_duration_secs: f32,
    pub dawn_ratio: f32,
    pub day_ratio: f32,
    pub sunset_ratio: f32,
    pub night_ratio: f32,
    pub sun_colors: [[f32; 3]; 4],
    pub sun_intensities: [f32; 4],
    pub ambient_mins: [f32; 4],
    pub sky_colors: [[f32; 4]; 4],
    pub danger_multipliers: [f32; 4],
    pub temperature_modifiers: [f32; 4],
    #[serde(default)]
    pub temperature_celsius_offsets: [f32; 4],
}

impl DayNightConfig {
    pub fn phase_ratios(&self) -> [f32; 4] {
        [
            self.dawn_ratio,
            self.day_ratio,
            self.sunset_ratio,
            self.night_ratio,
        ]
    }
}

/// Message fired when the day phase transitions.
#[derive(Message, Debug)]
#[allow(dead_code)]
pub struct DayPhaseChanged {
    pub previous: DayPhase,
    pub current: DayPhase,
    pub time_of_day: f32,
}

/// Tracks the current world time and derived lighting/gameplay values.
#[derive(Resource, Debug)]
pub struct WorldTime {
    pub time_of_day: f32,
    pub phase: DayPhase,
    pub phase_progress: f32,
    pub sun_color: Vec3,
    pub sun_intensity: f32,
    pub ambient_min: f32,
    pub sky_color: Color,
    pub danger_multiplier: f32,
    pub temperature_modifier: f32,
    pub temperature_celsius_offset: f32,
    pub paused: bool,
}

impl Default for WorldTime {
    fn default() -> Self {
        Self {
            time_of_day: 0.25,
            phase: DayPhase::Dawn,
            phase_progress: 0.0,
            sun_color: Vec3::new(1.0, 0.98, 0.9),
            sun_intensity: 1.0,
            ambient_min: 0.0,
            sky_color: Color::WHITE,
            danger_multiplier: 0.0,
            temperature_modifier: 0.0,
            temperature_celsius_offset: 0.0,
            paused: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn compute_phase_and_progress(time_of_day: f32, config: &DayNightConfig) -> (DayPhase, f32) {
    let ratios = config.phase_ratios();
    let phases = [
        DayPhase::Dawn,
        DayPhase::Day,
        DayPhase::Sunset,
        DayPhase::Night,
    ];
    // Dawn starts at 0.25 (6 AM equivalent).
    let t = (time_of_day - 0.25).rem_euclid(1.0);
    let mut accumulated = 0.0;
    for (i, phase) in phases.iter().enumerate() {
        let ratio = ratios[i];
        if t < accumulated + ratio {
            let progress = (t - accumulated) / ratio;
            return (*phase, progress.clamp(0.0, 1.0));
        }
        accumulated += ratio;
    }
    (DayPhase::Night, 1.0)
}

fn lerp_phase_value(values: &[f32; 4], phase: DayPhase, progress: f32) -> f32 {
    let a = values[phase.index()];
    let b = values[phase.next().index()];
    a + (b - a) * progress
}

fn lerp_phase_color(colors: &[[f32; 3]; 4], phase: DayPhase, progress: f32) -> Vec3 {
    let a = Vec3::from_array(colors[phase.index()]);
    let b = Vec3::from_array(colors[phase.next().index()]);
    a + (b - a) * progress
}

fn lerp_phase_color4(colors: &[[f32; 4]; 4], phase: DayPhase, progress: f32) -> Color {
    let a = colors[phase.index()];
    let b = colors[phase.next().index()];
    Color::srgba(
        a[0] + (b[0] - a[0]) * progress,
        a[1] + (b[1] - a[1]) * progress,
        a[2] + (b[2] - a[2]) * progress,
        a[3] + (b[3] - a[3]) * progress,
    )
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Advances world time and recomputes all derived values each frame.
pub fn tick_world_time(
    time: Res<Time>,
    config: Res<DayNightConfig>,
    mut world_time: ResMut<WorldTime>,
    mut phase_events: MessageWriter<DayPhaseChanged>,
) {
    if !world_time.paused {
        let dt = time.delta_secs();
        world_time.time_of_day += dt / config.cycle_duration_secs;
        world_time.time_of_day = world_time.time_of_day.rem_euclid(1.0);
    }

    let (phase, progress) = compute_phase_and_progress(world_time.time_of_day, &config);

    if phase != world_time.phase {
        phase_events.write(DayPhaseChanged {
            previous: world_time.phase,
            current: phase,
            time_of_day: world_time.time_of_day,
        });
        info!("Day phase: {} → {}", world_time.phase, phase);
    }

    world_time.phase = phase;
    world_time.phase_progress = progress;
    world_time.sun_color = lerp_phase_color(&config.sun_colors, phase, progress);
    world_time.sun_intensity = lerp_phase_value(&config.sun_intensities, phase, progress);
    world_time.ambient_min = lerp_phase_value(&config.ambient_mins, phase, progress);
    world_time.sky_color = lerp_phase_color4(&config.sky_colors, phase, progress);
    world_time.danger_multiplier = lerp_phase_value(&config.danger_multipliers, phase, progress);
    world_time.temperature_modifier =
        lerp_phase_value(&config.temperature_modifiers, phase, progress);
    world_time.temperature_celsius_offset =
        lerp_phase_value(&config.temperature_celsius_offsets, phase, progress);
}

/// Tint parallax layers based on time of day.
/// Sky layers get full tint; background layers get 50% blend.
pub fn tint_parallax_layers(
    world_time: Res<WorldTime>,
    mut sky_query: Query<&mut Sprite, With<ParallaxSkyLayer>>,
    mut layer_query: Query<&mut Sprite, (With<ParallaxLayerConfig>, Without<ParallaxSkyLayer>)>,
) {
    let sky_tint = world_time.sky_color;

    // Sky: full RGB tint, preserve alpha (biome transition controls alpha)
    for mut sprite in &mut sky_query {
        let alpha = sprite.color.alpha();
        sprite.color = sky_tint.with_alpha(alpha);
    }

    // Background hills/trees: 50% blend toward sky tint, preserve alpha
    for mut sprite in &mut layer_query {
        let alpha = sprite.color.alpha();
        let blended = Color::WHITE.mix(&sky_tint, 0.5).with_alpha(alpha);
        sprite.color = blended;
    }
}

impl WorldTime {
    /// Create a `WorldTime` initialized from a `DayNightConfig`, computing all
    /// derived values to avoid a first-frame flash.
    pub fn from_config(config: &DayNightConfig) -> Self {
        let mut wt = Self::default();
        let (phase, progress) = compute_phase_and_progress(wt.time_of_day, config);
        wt.phase = phase;
        wt.phase_progress = progress;
        wt.sun_color = lerp_phase_color(&config.sun_colors, phase, progress);
        wt.sun_intensity = lerp_phase_value(&config.sun_intensities, phase, progress);
        wt.ambient_min = lerp_phase_value(&config.ambient_mins, phase, progress);
        wt.sky_color = lerp_phase_color4(&config.sky_colors, phase, progress);
        wt.danger_multiplier = lerp_phase_value(&config.danger_multipliers, phase, progress);
        wt.temperature_modifier = lerp_phase_value(&config.temperature_modifiers, phase, progress);
        wt.temperature_celsius_offset =
            lerp_phase_value(&config.temperature_celsius_offsets, phase, progress);
        wt
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> DayNightConfig {
        DayNightConfig {
            cycle_duration_secs: 100.0,
            dawn_ratio: 0.10,
            day_ratio: 0.40,
            sunset_ratio: 0.10,
            night_ratio: 0.40,
            sun_colors: [
                [1.0, 0.65, 0.35],
                [1.0, 0.98, 0.90],
                [1.0, 0.50, 0.25],
                [0.15, 0.15, 0.35],
            ],
            sun_intensities: [0.6, 1.0, 0.5, 0.0],
            ambient_mins: [0.08, 0.0, 0.06, 0.04],
            sky_colors: [
                [0.95, 0.55, 0.35, 1.0],
                [1.0, 1.0, 1.0, 1.0],
                [0.90, 0.40, 0.30, 1.0],
                [0.08, 0.08, 0.18, 1.0],
            ],
            danger_multipliers: [0.5, 0.0, 0.5, 1.0],
            temperature_modifiers: [-0.1, 0.0, -0.05, -0.2],
            temperature_celsius_offsets: [-3.0, 0.0, -3.0, -8.0],
        }
    }

    #[test]
    fn phase_at_dawn_start() {
        let config = test_config();
        let (phase, progress) = compute_phase_and_progress(0.25, &config);
        assert_eq!(phase, DayPhase::Dawn);
        assert!(progress.abs() < 0.01);
    }

    #[test]
    fn phase_at_noon() {
        let config = test_config();
        // dawn 0.25..0.35, day 0.35..0.75. mid-day = 0.55
        let (phase, progress) = compute_phase_and_progress(0.55, &config);
        assert_eq!(phase, DayPhase::Day);
        assert!((progress - 0.5).abs() < 0.01);
    }

    #[test]
    fn phase_at_midnight() {
        let config = test_config();
        let (phase, _) = compute_phase_and_progress(0.0, &config);
        assert_eq!(phase, DayPhase::Night);
    }

    #[test]
    fn phase_at_sunset() {
        let config = test_config();
        // dawn=0.10, day=0.40 → sunset starts at 0.25+0.10+0.40=0.75
        let (phase, progress) = compute_phase_and_progress(0.75, &config);
        assert_eq!(phase, DayPhase::Sunset);
        assert!(progress.abs() < 0.01);
    }

    #[test]
    fn phase_ratios_sum_to_one() {
        let config = test_config();
        let ratios = config.phase_ratios();
        let sum: f32 = ratios.iter().sum();
        assert!((sum - 1.0).abs() < 0.001);
    }

    #[test]
    fn lerp_phase_value_at_boundaries() {
        let vals = [0.6, 1.0, 0.5, 0.0];
        assert!((lerp_phase_value(&vals, DayPhase::Dawn, 0.0) - 0.6).abs() < 0.001);
        assert!((lerp_phase_value(&vals, DayPhase::Dawn, 1.0) - 1.0).abs() < 0.001);
        assert!((lerp_phase_value(&vals, DayPhase::Dawn, 0.5) - 0.8).abs() < 0.001);
    }

    #[test]
    fn day_phase_next_cycles() {
        assert_eq!(DayPhase::Dawn.next(), DayPhase::Day);
        assert_eq!(DayPhase::Day.next(), DayPhase::Sunset);
        assert_eq!(DayPhase::Sunset.next(), DayPhase::Night);
        assert_eq!(DayPhase::Night.next(), DayPhase::Dawn);
    }

    #[test]
    fn world_time_from_config() {
        let config = test_config();
        let wt = WorldTime::from_config(&config);
        // Should start at dawn (time_of_day defaults to 0.25)
        assert_eq!(wt.phase, DayPhase::Dawn);
        assert!(wt.phase_progress.abs() < 0.01);
        // Sun intensity should be interpolated from config
        assert!(wt.sun_intensity > 0.0);
    }
}
