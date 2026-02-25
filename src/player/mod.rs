pub mod collision;
pub mod movement;
pub mod wrap;

use bevy::prelude::*;

use crate::world;
use crate::world::terrain_gen;

pub const PLAYER_SPEED: f32 = 200.0;
pub const JUMP_VELOCITY: f32 = 400.0;
pub const GRAVITY: f32 = 800.0;
pub const PLAYER_WIDTH: f32 = 64.0;
pub const PLAYER_HEIGHT: f32 = 128.0;

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
        app.add_systems(Startup, spawn_player).add_systems(
            Update,
            (
                movement::player_input,
                movement::apply_gravity,
                collision::collision_system,
                wrap::player_wrap_system,
            )
                .chain(),
        );
    }
}

fn spawn_player(mut commands: Commands) {
    let spawn_tile_x = 0;
    let surface_y = terrain_gen::surface_height(42, spawn_tile_x);
    let spawn_pixel_x = spawn_tile_x as f32 * world::TILE_SIZE + world::TILE_SIZE / 2.0;
    let spawn_pixel_y = (surface_y + 5) as f32 * world::TILE_SIZE + PLAYER_HEIGHT / 2.0;

    commands.spawn((
        Player,
        Velocity::default(),
        Grounded(false),
        Sprite::from_color(
            Color::srgb(0.2, 0.4, 0.9),
            Vec2::new(PLAYER_WIDTH, PLAYER_HEIGHT),
        ),
        Transform::from_xyz(spawn_pixel_x, spawn_pixel_y, 1.0),
    ));
}
