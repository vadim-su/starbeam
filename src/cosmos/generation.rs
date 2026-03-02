//! Core system generation — turns star/planet templates + seeds into concrete values.
//!
//! [`generate_system`] is the main entry point: given a universe seed, galaxy/system
//! coordinates, and loaded templates, it produces a [`GeneratedSystem`] containing a
//! [`GeneratedStar`] and a [`Vec<GeneratedBody>`] with fully resolved day/night configs.

use crate::cosmos::address::{CelestialAddress, CelestialSeeds};
use crate::cosmos::assets::{GenerationConfigAsset, StarTypeAsset};
use crate::registry::assets::PlanetTypeAsset;
use crate::world::day_night::DayNightConfig;

// ---------------------------------------------------------------------------
// Generated types
// ---------------------------------------------------------------------------

/// A procedurally generated star (concrete values, not ranges).
#[derive(Debug, Clone)]
pub struct GeneratedStar {
    pub type_id: String,
    pub luminosity: f32,
    pub sun_color: [f32; 3],
    pub orbit_count: u32,
}

/// A procedurally generated celestial body (planet or moon).
#[derive(Debug, Clone)]
pub struct GeneratedBody {
    pub address: CelestialAddress,
    pub planet_type_id: String,
    pub width_tiles: i32,
    pub height_tiles: i32,
    pub day_night: DayNightConfig,
}

/// A complete generated star system.
#[derive(Debug, Clone)]
pub struct GeneratedSystem {
    pub star: GeneratedStar,
    pub bodies: Vec<GeneratedBody>,
}

// ---------------------------------------------------------------------------
// Seed helpers
// ---------------------------------------------------------------------------

/// Deterministic float in [0, 1) from a seed.
fn seed_to_f32(seed: u64) -> f32 {
    (seed >> 33) as f32 / (1u64 << 31) as f32
}

/// Pick a value from a (min, max) range using a seed.
fn lerp_range(seed: u64, min: f32, max: f32) -> f32 {
    min + seed_to_f32(seed) * (max - min)
}

/// Pick an integer from a (min, max) inclusive range using a seed.
fn range_u32(seed: u64, min: u32, max: u32) -> u32 {
    min + (seed % (max - min + 1) as u64) as u32
}

/// Pick an element from a slice using a seed.
fn pick<'a, T>(seed: u64, items: &'a [T]) -> &'a T {
    &items[(seed % items.len() as u64) as usize]
}

/// Sub-seed helper: derives a new seed for a specific index from a parent.
/// Uses the same SplitMix64 mixing step as `address.rs`.
fn sub_seed(parent: u64, index: u64) -> u64 {
    let mut z = parent.wrapping_add(index).wrapping_add(0x9e3779b97f4a7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

// ---------------------------------------------------------------------------
// System generation
// ---------------------------------------------------------------------------

/// Generate a complete star system from templates and a system address.
pub fn generate_system(
    universe_seed: u64,
    galaxy: bevy::math::IVec2,
    system: bevy::math::IVec2,
    star_templates: &[&StarTypeAsset],
    planet_templates: &std::collections::HashMap<String, &PlanetTypeAsset>,
    gen_config: &GenerationConfigAsset,
) -> GeneratedSystem {
    let base_addr = CelestialAddress {
        galaxy,
        system,
        orbit: 0,
        satellite: None,
    };
    let seeds = CelestialSeeds::derive(universe_seed, &base_addr);

    // Pick star type
    let star_template = pick(seeds.star_seed, star_templates);
    let luminosity = lerp_range(
        sub_seed(seeds.star_seed, 1),
        star_template.luminosity.0,
        star_template.luminosity.1,
    );
    let orbit_count = range_u32(
        sub_seed(seeds.star_seed, 2),
        star_template.orbit_count.0,
        star_template.orbit_count.1,
    );

    let star = GeneratedStar {
        type_id: star_template.id.clone(),
        luminosity,
        sun_color: star_template.sun_color,
        orbit_count,
    };

    // Generate bodies for each orbit
    let mut bodies = Vec::with_capacity(orbit_count as usize);
    for orbit in 0..orbit_count {
        let addr = CelestialAddress {
            galaxy,
            system,
            orbit,
            satellite: None,
        };
        let body_seeds = CelestialSeeds::derive(universe_seed, &addr);

        // Determine planet type from star's temperature zones
        let planet_type_id = determine_planet_type(orbit, star_template, body_seeds.body_seed);

        // Look up template (fallback to first available if type not found)
        let planet_template = planet_templates
            .get(&planet_type_id)
            .or_else(|| planet_templates.values().next())
            .expect("at least one planet template must exist");

        // Generate size
        let (width, height) = planet_template.size.unwrap_or((
            gen_config.default_planet_size.width,
            gen_config.default_planet_size.height,
        ));

        // Generate day/night
        let day_night = generate_day_night(
            &star,
            planet_template,
            orbit,
            orbit_count,
            body_seeds.daynight_seed,
        );

        bodies.push(GeneratedBody {
            address: addr,
            planet_type_id,
            width_tiles: width,
            height_tiles: height,
            day_night,
        });
    }

    GeneratedSystem { star, bodies }
}

// ---------------------------------------------------------------------------
// Planet type determination
// ---------------------------------------------------------------------------

/// Determine planet type from orbit index and star's temperature zones.
fn determine_planet_type(orbit: u32, star: &StarTypeAsset, seed: u64) -> String {
    for zone in &star.zones {
        if orbit >= zone.orbits.0 && orbit <= zone.orbits.1 && !zone.types.is_empty() {
            return pick(seed, &zone.types).clone();
        }
    }
    // Fallback: first type of last zone
    star.zones
        .last()
        .and_then(|z| z.types.first())
        .cloned()
        .unwrap_or_else(|| "barren".to_string())
}

// ---------------------------------------------------------------------------
// Day/night generation
// ---------------------------------------------------------------------------

/// Generate concrete [`DayNightConfig`] from star + planet template + orbit.
pub fn generate_day_night(
    star: &GeneratedStar,
    planet: &PlanetTypeAsset,
    orbit: u32,
    total_orbits: u32,
    seed: u64,
) -> DayNightConfig {
    // Orbit factor: 0.0 = closest, 1.0 = farthest
    let orbit_factor = if total_orbits > 1 {
        orbit as f32 / (total_orbits - 1) as f32
    } else {
        0.5
    };

    // Cycle duration: farther = longer days (600..1800 secs range)
    let cycle_duration = planet
        .cycle_duration_range
        .map(|(min, max)| lerp_range(sub_seed(seed, 10), min, max))
        .unwrap_or(600.0 + orbit_factor * 1200.0);

    // Day/night ratios
    let day_ratio = planet
        .day_ratio
        .map(|(min, max)| lerp_range(sub_seed(seed, 20), min, max))
        .unwrap_or(0.35 + orbit_factor * 0.1);
    let night_ratio = planet
        .night_ratio
        .map(|(min, max)| lerp_range(sub_seed(seed, 21), min, max))
        .unwrap_or(0.35 + orbit_factor * 0.1);
    // Dawn/sunset fill the remainder
    let transition_total = (1.0 - day_ratio - night_ratio).max(0.04);
    let dawn_ratio = planet
        .dawn_ratio
        .map(|(min, max)| lerp_range(sub_seed(seed, 22), min, max))
        .unwrap_or(transition_total * 0.5);
    let sunset_ratio = planet
        .sunset_ratio
        .map(|(min, max)| lerp_range(sub_seed(seed, 23), min, max))
        .unwrap_or(transition_total - dawn_ratio);

    // Normalize ratios to sum to 1.0
    let sum = day_ratio + night_ratio + dawn_ratio + sunset_ratio;
    let day_ratio = day_ratio / sum;
    let night_ratio = night_ratio / sum;
    let dawn_ratio = dawn_ratio / sum;
    let sunset_ratio = sunset_ratio / sum;

    // Sun intensity: luminosity diminished by distance
    let base_intensity = star.luminosity * (1.0 - orbit_factor * 0.6);
    let intensity_mod = planet
        .sun_intensity_modifier
        .map(|(min, max)| lerp_range(sub_seed(seed, 30), min, max))
        .unwrap_or(1.0);
    let peak_intensity = base_intensity * intensity_mod;

    let sun_intensities = [
        peak_intensity * 0.6,
        peak_intensity,
        peak_intensity * 0.5,
        0.0,
    ];

    // Sun colors from star
    let sun_colors = [
        [
            star.sun_color[0],
            star.sun_color[1] * 0.66,
            star.sun_color[2] * 0.39,
        ],
        star.sun_color,
        [
            star.sun_color[0],
            star.sun_color[1] * 0.51,
            star.sun_color[2] * 0.28,
        ],
        [
            star.sun_color[0] * 0.15,
            star.sun_color[1] * 0.15,
            star.sun_color[2] * 0.39,
        ],
    ];

    // Sky colors from planet palette or defaults
    let sky_colors = if let Some(palette) = &planet.sky_color_palette {
        let mut colors = [[0.0f32; 4]; 4];
        for (i, pair) in palette.iter().enumerate() {
            let s = sub_seed(seed, 40 + i as u64);
            let t = seed_to_f32(s);
            for c in 0..4 {
                colors[i][c] = pair[0][c] + t * (pair[1][c] - pair[0][c]);
            }
        }
        colors
    } else {
        // Default dim atmosphere
        [
            [0.5, 0.3, 0.2, 1.0],
            [0.6, 0.6, 0.7, 1.0],
            [0.5, 0.25, 0.15, 1.0],
            [0.05, 0.05, 0.1, 1.0],
        ]
    };

    let danger_multipliers = planet.danger_multipliers.unwrap_or([0.5, 0.0, 0.5, 1.0]);
    let temperature_modifiers = planet.temperature_modifiers.unwrap_or_else(|| {
        let base = -0.1 * (1.0 - orbit_factor);
        [base, 0.0, base * 0.5, base * 2.0]
    });

    // Ambient mins
    let ambient_mins = [
        0.08 * (1.0 - orbit_factor * 0.5),
        0.0,
        0.06 * (1.0 - orbit_factor * 0.5),
        0.04,
    ];

    DayNightConfig {
        cycle_duration_secs: cycle_duration,
        dawn_ratio,
        day_ratio,
        sunset_ratio,
        night_ratio,
        sun_colors,
        sun_intensities,
        ambient_mins,
        sky_colors,
        danger_multipliers,
        temperature_modifiers,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cosmos::assets::{
        GenerationConfigAsset, PlanetSizeConfig, StarTypeAsset, TemperatureZone,
    };
    use bevy::math::IVec2;
    use std::collections::HashMap;

    fn test_star() -> StarTypeAsset {
        StarTypeAsset {
            id: "yellow_dwarf".into(),
            orbit_count: (3, 6),
            luminosity: (0.9, 1.1),
            sun_color: [1.0, 0.98, 0.90],
            zones: vec![
                TemperatureZone {
                    orbits: (0, 1),
                    temperature: "hot".into(),
                    types: vec!["barren".into()],
                },
                TemperatureZone {
                    orbits: (2, 4),
                    temperature: "warm".into(),
                    types: vec!["garden".into()],
                },
                TemperatureZone {
                    orbits: (5, 9),
                    temperature: "cold".into(),
                    types: vec!["barren".into()],
                },
            ],
        }
    }

    fn test_planet_template() -> PlanetTypeAsset {
        use crate::registry::assets::{LayerConfigAsset, LayersAsset};

        PlanetTypeAsset {
            id: "garden".into(),
            primary_biome: "meadow".into(),
            secondary_biomes: vec!["forest".into()],
            layers: LayersAsset {
                surface: LayerConfigAsset {
                    primary_biome: None,
                    terrain_frequency: 0.02,
                    terrain_amplitude: 40.0,
                    depth_ratio: 0.30,
                },
                underground: LayerConfigAsset {
                    primary_biome: Some("underground_dirt".into()),
                    terrain_frequency: 0.07,
                    terrain_amplitude: 1.0,
                    depth_ratio: 0.25,
                },
                deep_underground: LayerConfigAsset {
                    primary_biome: Some("underground_rock".into()),
                    terrain_frequency: 0.05,
                    terrain_amplitude: 1.0,
                    depth_ratio: 0.33,
                },
                core: LayerConfigAsset {
                    primary_biome: Some("core_magma".into()),
                    terrain_frequency: 0.04,
                    terrain_amplitude: 1.0,
                    depth_ratio: 0.12,
                },
            },
            region_width_min: 300,
            region_width_max: 600,
            primary_region_ratio: 0.6,
            size: None,
            cycle_duration_range: None,
            day_ratio: Some((0.35, 0.50)),
            night_ratio: Some((0.30, 0.45)),
            dawn_ratio: None,
            sunset_ratio: None,
            sky_color_palette: Some([
                [[0.90, 0.50, 0.30, 1.0], [1.0, 0.60, 0.40, 1.0]],
                [[0.85, 0.90, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]],
                [[0.85, 0.35, 0.25, 1.0], [0.95, 0.45, 0.35, 1.0]],
                [[0.05, 0.05, 0.15, 1.0], [0.12, 0.12, 0.22, 1.0]],
            ]),
            sun_intensity_modifier: None,
            danger_multipliers: Some([0.5, 0.0, 0.5, 1.0]),
            temperature_modifiers: None,
        }
    }

    fn test_gen_config() -> GenerationConfigAsset {
        GenerationConfigAsset {
            default_planet_size: PlanetSizeConfig {
                width: 2048,
                height: 1024,
            },
            chunk_size: 32,
            tile_size: 8.0,
            chunk_load_radius: 3,
            orbit_temperature_falloff: 0.15,
        }
    }

    #[test]
    fn generate_system_deterministic() {
        let star = test_star();
        let planet = test_planet_template();
        let gen_cfg = test_gen_config();
        let mut templates = HashMap::new();
        templates.insert("garden".to_string(), &planet);
        templates.insert("barren".to_string(), &planet); // reuse for test

        let sys1 = generate_system(42, IVec2::ZERO, IVec2::ZERO, &[&star], &templates, &gen_cfg);
        let sys2 = generate_system(42, IVec2::ZERO, IVec2::ZERO, &[&star], &templates, &gen_cfg);

        assert_eq!(sys1.star.type_id, sys2.star.type_id);
        assert_eq!(sys1.star.orbit_count, sys2.star.orbit_count);
        assert_eq!(sys1.bodies.len(), sys2.bodies.len());
        for (a, b) in sys1.bodies.iter().zip(sys2.bodies.iter()) {
            assert_eq!(a.planet_type_id, b.planet_type_id);
            assert_eq!(
                a.day_night.cycle_duration_secs,
                b.day_night.cycle_duration_secs
            );
        }
    }

    #[test]
    fn different_seed_different_system() {
        let star = test_star();
        let planet = test_planet_template();
        let gen_cfg = test_gen_config();
        let mut templates = HashMap::new();
        templates.insert("garden".to_string(), &planet);
        templates.insert("barren".to_string(), &planet);

        let sys1 = generate_system(42, IVec2::ZERO, IVec2::ZERO, &[&star], &templates, &gen_cfg);
        let sys2 = generate_system(
            999,
            IVec2::ZERO,
            IVec2::ZERO,
            &[&star],
            &templates,
            &gen_cfg,
        );

        // At minimum, orbit count or luminosity should differ
        let differs = sys1.star.luminosity != sys2.star.luminosity
            || sys1.star.orbit_count != sys2.star.orbit_count;
        assert!(
            differs,
            "different universe seeds should produce different systems"
        );
    }

    #[test]
    fn orbit_determines_planet_type() {
        let star = test_star();
        // Orbit 0 should be "hot" zone → barren
        let t = determine_planet_type(0, &star, 42);
        assert_eq!(t, "barren");
        // Orbit 3 should be "warm" zone → garden
        let t = determine_planet_type(3, &star, 42);
        assert_eq!(t, "garden");
    }

    #[test]
    fn day_night_ratios_sum_to_one() {
        let star = GeneratedStar {
            type_id: "yellow_dwarf".into(),
            luminosity: 1.0,
            sun_color: [1.0, 0.98, 0.90],
            orbit_count: 5,
        };
        let planet = test_planet_template();
        let dn = generate_day_night(&star, &planet, 2, 5, 12345);
        let sum = dn.dawn_ratio + dn.day_ratio + dn.sunset_ratio + dn.night_ratio;
        assert!(
            (sum - 1.0).abs() < 0.001,
            "ratios must sum to 1.0, got {sum}"
        );
    }

    #[test]
    fn day_night_sun_color_from_star() {
        let star = GeneratedStar {
            type_id: "test".into(),
            luminosity: 1.0,
            sun_color: [0.8, 0.3, 0.1],
            orbit_count: 3,
        };
        let planet = test_planet_template();
        let dn = generate_day_night(&star, &planet, 1, 3, 42);
        // Day sun_color should be the star's color
        assert_eq!(dn.sun_colors[1], [0.8, 0.3, 0.1]);
    }

    #[test]
    fn farther_orbit_longer_cycle() {
        let star = GeneratedStar {
            type_id: "test".into(),
            luminosity: 1.0,
            sun_color: [1.0, 1.0, 1.0],
            orbit_count: 6,
        };
        // Use planet without explicit cycle_duration_range → derive from orbit
        let mut planet = test_planet_template();
        planet.cycle_duration_range = None;
        let dn_close = generate_day_night(&star, &planet, 0, 6, 42);
        let dn_far = generate_day_night(&star, &planet, 5, 6, 42);
        assert!(
            dn_far.cycle_duration_secs > dn_close.cycle_duration_secs,
            "farther orbit should have longer cycle: {} vs {}",
            dn_far.cycle_duration_secs,
            dn_close.cycle_duration_secs
        );
    }
}
