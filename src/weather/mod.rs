pub mod snow_overlay;
pub mod snow_particles;
pub mod weather_state;
pub mod wind;

use bevy::prelude::*;

use crate::registry::AppState;
use crate::sets::GameSet;

pub use weather_state::WeatherState;
pub use wind::Wind;

use snow_overlay::SnowOverlayTimer;
use snow_particles::{
    rebuild_snow_mesh, spawn_snow_particles, update_snow_particles, SharedSnowMaterial,
    SnowParticlePool,
};

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Wind>()
            .init_resource::<WeatherState>()
            .init_resource::<SnowParticlePool>()
            .init_resource::<SnowOverlayTimer>()
            .add_systems(Startup, snow_particles::init_snow_render)
            .add_systems(
                OnEnter(AppState::InGame),
                snow_overlay::init_snow_overlay_texture,
            )
            .add_systems(
                Update,
                (wind::update_wind, weather_state::update_weather)
                    .in_set(GameSet::WorldUpdate)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (spawn_snow_particles, update_snow_particles, rebuild_snow_mesh)
                    .chain()
                    .in_set(GameSet::WorldUpdate)
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<SharedSnowMaterial>),
            )
            .add_systems(
                Update,
                (
                    snow_overlay::update_snow_overlays,
                    snow_overlay::handle_dirty_chunk_overlays,
                    snow_overlay::update_tree_snow,
                    snow_overlay::cleanup_tree_snow,
                )
                    .in_set(GameSet::WorldUpdate)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
