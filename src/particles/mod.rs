pub mod particle;
pub mod pool;

use bevy::prelude::*;

pub use particle::Particle;
pub use pool::{ParticleConfig, ParticlePool};

pub struct ParticlePlugin;

impl Plugin for ParticlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ParticleConfig>()
            .insert_resource(ParticlePool::new(3000));
        // NOTE: Physics system will be added in a later task.
        // For now, just register resources.
    }
}
