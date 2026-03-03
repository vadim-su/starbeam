pub mod particle;
pub mod physics;
pub mod pool;
pub mod render;

use bevy::prelude::*;

pub use particle::Particle;
pub use pool::{ParticleConfig, ParticlePool};
pub use render::ParticleMeshEntity;

use crate::registry::AppState;
use crate::sets::GameSet;

pub struct ParticlePlugin;

impl Plugin for ParticlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ParticleConfig>()
            .insert_resource(ParticlePool::new(3000))
            .add_systems(Startup, render::init_particle_render)
            .add_systems(
                Update,
                (physics::particle_physics, render::rebuild_particle_mesh)
                    .chain()
                    .in_set(GameSet::WorldUpdate)
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<render::SharedParticleMaterial>),
            );
    }
}
