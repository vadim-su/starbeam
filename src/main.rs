mod camera;
pub mod crafting;
mod interaction;
pub mod inventory;
pub mod item;
pub mod math;
mod parallax;
mod player;
mod registry;
pub mod sets;
#[cfg(test)]
mod test_helpers;
mod ui;
mod world;

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy_egui::EguiPlugin;

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
        .add_plugins(ui::UiPlugin)
        .add_plugins(item::ItemPlugin)
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
