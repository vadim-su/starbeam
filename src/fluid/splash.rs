use bevy::prelude::*;

/// Configuration for splash transitions (kept for potential future use).
///
/// Controls visual properties of spawned splash particles.
#[derive(Resource, Debug, Clone)]
pub struct SplashConfig {
    /// Fraction of cell mass displaced on a Splash impact.
    pub splash_displacement: f32,
    /// Particles spawned per unit of displaced mass (visual density).
    pub particles_per_mass: f32,
    /// Max lifetime of splash particles in seconds.
    pub particle_lifetime: f32,
    /// Visual radius of each particle in world units.
    pub particle_size: f32,
    /// Minimum velocity magnitude to trigger splash particles.
    pub min_splash_velocity: f32,
}

impl Default for SplashConfig {
    fn default() -> Self {
        Self {
            splash_displacement: 0.25,
            particles_per_mass: 40.0,
            particle_lifetime: 1.2,
            particle_size: 5.0,
            min_splash_velocity: 5.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splash_config_defaults() {
        let cfg = SplashConfig::default();
        assert!(cfg.splash_displacement > 0.0);
        assert!(cfg.particle_lifetime > 0.0);
        assert!(cfg.particles_per_mass > 0.0);
    }
}
