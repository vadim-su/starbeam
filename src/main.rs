mod camera;
mod chat;
pub mod cosmos;
pub mod crafting;
mod interaction;
pub mod inventory;
pub mod item;
pub mod liquid;
pub mod math;
mod menu;
pub mod object;
pub mod particles;
mod parallax;
pub mod physics;
mod player;
mod registry;
pub mod sets;
#[cfg(test)]
mod test_helpers;
mod ui;
mod world;

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy_egui::{EguiGlobalSettings, EguiPlugin};

use sets::GameSet;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Starbeam".into(),
                        resolution: (1280, 720).into(),
                        present_mode: bevy::window::PresentMode::AutoNoVsync,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins(EguiPlugin::default())
        .insert_resource(EguiGlobalSettings {
            auto_create_primary_context: false,
            ..default()
        })
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .add_plugins(menu::MenuPlugin)
        .add_plugins(registry::RegistryPlugin)
        .add_plugins(world::WorldPlugin)
        .add_plugins(liquid::LiquidPlugin)
        .add_plugins(player::PlayerPlugin)
        .add_plugins(physics::PhysicsPlugin)
        .add_plugins(particles::ParticlePlugin)
        .add_plugins(camera::CameraPlugin)
        .add_plugins(parallax::ParallaxPlugin)
        .add_plugins(interaction::InteractionPlugin)
        .add_plugins(chat::ChatPlugin)
        .add_plugins(ui::UiPlugin)
        .add_plugins(item::ItemPlugin)
        .add_plugins(object::ObjectPlugin)
        .add_plugins(inventory::InventoryPlugin)
        .add_plugins(crafting::CraftingPlugin)
        .configure_sets(
            Update,
            (
                GameSet::Input,
                GameSet::Physics,
                GameSet::WorldUpdate,
                GameSet::Camera,
                GameSet::Parallax,
                GameSet::Ui,
            )
                .chain()
                .run_if(in_state(registry::AppState::InGame)),
        )
        .run();
}
