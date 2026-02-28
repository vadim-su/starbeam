pub mod animation;
pub mod collision;
pub mod movement;
pub mod wrap;

use bevy::prelude::*;

use crate::inventory::{Hotbar, Inventory};
use crate::registry::biome::PlanetConfig;
use crate::registry::player::PlayerConfig;
use crate::registry::world::WorldConfig;
use crate::registry::AppState;
use crate::sets::GameSet;
use crate::world::terrain_gen;
use crate::world::terrain_gen::TerrainNoiseCache;

use animation::{AnimationKind, AnimationState, CharacterAnimations};

pub const MAX_DELTA_SECS: f32 = 1.0 / 20.0;

#[derive(Component)]
pub struct Player;

#[derive(Component, Default)]
pub struct Velocity {
    pub x: f32,
    pub y: f32,
}

#[derive(Component)]
pub struct Grounded(pub bool);

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(AppState::InGame),
            (animation::load_character_animations, spawn_player).chain(),
        )
        .add_systems(
            Update,
            (
                movement::player_input,
                movement::apply_gravity,
                collision::collision_system,
                wrap::player_wrap_system,
                animation::animate_player,
            )
                .chain()
                .in_set(GameSet::Physics),
        );
    }
}

fn spawn_player(
    mut commands: Commands,
    player_config: Res<PlayerConfig>,
    world_config: Res<WorldConfig>,
    planet_config: Res<PlanetConfig>,
    noise_cache: Res<TerrainNoiseCache>,
    animations: Res<CharacterAnimations>,
) {
    let spawn_tile_x = 0;
    let surface_y = terrain_gen::surface_height(
        &noise_cache,
        spawn_tile_x,
        &world_config,
        planet_config.layers.surface.terrain_frequency,
        planet_config.layers.surface.terrain_amplitude,
    );
    let spawn_pixel_x = spawn_tile_x as f32 * world_config.tile_size + world_config.tile_size / 2.0;
    let spawn_pixel_y =
        (surface_y + 5) as f32 * world_config.tile_size + player_config.height / 2.0;

    commands.spawn((
        Player,
        {
            let mut inv = Inventory::new();
            inv.try_add_item("torch", 10, 999);
            inv
        },
        Hotbar::new(),
        Velocity::default(),
        Grounded(false),
        AnimationState {
            kind: AnimationKind::Idle,
            frame: 0,
            timer: Timer::from_seconds(0.15, TimerMode::Repeating),
            facing_right: true,
        },
        Sprite::from_image(animations.idle[0].clone()),
        Transform::from_xyz(spawn_pixel_x, spawn_pixel_y, 1.0),
    ));
}
