pub mod weather_state;
pub mod wind;

use bevy::prelude::*;

use crate::registry::AppState;
use crate::sets::GameSet;

pub use weather_state::WeatherState;
pub use wind::Wind;

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Wind>()
            .init_resource::<WeatherState>()
            .add_systems(
                Update,
                (wind::update_wind, weather_state::update_weather)
                    .in_set(GameSet::WorldUpdate)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
