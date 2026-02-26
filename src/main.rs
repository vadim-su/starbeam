mod camera;
mod interaction;
mod parallax;
mod player;
mod registry;
mod ui;
mod world;

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy::sprite_render::Material2dPlugin;
use bevy_egui::EguiPlugin;

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
        .add_plugins(EguiPlugin::default())
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .add_plugins(registry::RegistryPlugin)
        .add_plugins(world::WorldPlugin)
        .add_plugins(player::PlayerPlugin)
        .add_plugins(camera::CameraPlugin)
        .add_plugins(parallax::ParallaxPlugin)
        .add_plugins(interaction::InteractionPlugin)
        .add_plugins(Material2dPlugin::<world::tile_renderer::TileMaterial>::default())
        .add_plugins(ui::UiPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: 0.7,
            ..OrthographicProjection::default_2d()
        }),
    ));
}
