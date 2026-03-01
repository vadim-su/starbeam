pub mod atlas;
pub mod autotile;
pub mod biome_map;
pub mod chunk;
pub mod ctx;
pub mod day_night;
pub mod lit_sprite;
pub mod mesh_builder;
pub mod rc_lighting;
pub mod rc_pipeline;
pub mod terrain_gen;
pub mod tile_renderer;

use bevy::prelude::*;
use bevy::sprite_render::Material2dPlugin;

use crate::registry::AppState;
use crate::sets::GameSet;
use crate::world::chunk::{LoadedChunks, WorldMap};
use crate::world::lit_sprite::LitSpriteMaterial;
use crate::world::mesh_builder::MeshBuildBuffers;
use crate::world::tile_renderer::TileMaterial;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<TileMaterial>::default())
            .add_plugins(Material2dPlugin::<LitSpriteMaterial>::default())
            .add_plugins(rc_lighting::RcLightingPlugin)
            .init_resource::<WorldMap>()
            .init_resource::<LoadedChunks>()
            .init_resource::<MeshBuildBuffers>()
            .add_message::<day_night::DayPhaseChanged>()
            .add_systems(
                OnEnter(AppState::InGame),
                (
                    lit_sprite::init_lit_sprite_resources,
                    day_night::load_day_night_config,
                ),
            )
            .add_systems(
                Update,
                (chunk::chunk_loading_system, chunk::rebuild_dirty_chunks)
                    .chain()
                    .in_set(GameSet::WorldUpdate),
            )
            .add_systems(
                Update,
                day_night::tick_world_time
                    .in_set(GameSet::WorldUpdate)
                    .run_if(resource_exists::<day_night::WorldTime>),
            );
    }
}
