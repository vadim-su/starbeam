mod camera;
mod interaction;
mod player;
mod world;

use bevy::prelude::*;
use bevy_ecs_tilemap::prelude::*;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Starbeam".into(),
                        resolution: (1280, 720).into(),
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins(TilemapPlugin)
        .add_plugins(world::WorldPlugin)
        .add_plugins(player::PlayerPlugin)
        .add_plugins(camera::CameraPlugin)
        .add_plugins(interaction::InteractionPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}
