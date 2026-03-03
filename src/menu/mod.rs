pub mod starfield;
pub mod ui;

use bevy::prelude::*;
use bevy::sprite_render::Material2dPlugin;

use crate::registry::AppState;
use starfield::{StarfieldMaterial, StarfieldMaterialHandle};

/// Marker component for all entities belonging to the main menu scene.
/// Used to despawn everything when leaving MainMenu state.
#[derive(Component)]
pub struct MenuEntity;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<StarfieldMaterial>::default())
            .add_systems(
                OnEnter(AppState::MainMenu),
                (spawn_menu_scene, ui::spawn_menu_ui),
            )
            .add_systems(
                Update,
                (
                    starfield::update_starfield_time,
                    ui::handle_new_game_button,
                    ui::handle_exit_button,
                )
                    .into_configs()
                    .run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(OnExit(AppState::MainMenu), despawn_menu_scene);
    }
}

/// Spawn the menu camera + fullscreen starfield quad.
fn spawn_menu_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StarfieldMaterial>>,
) {
    // Menu camera with dark clear color matching --bg-deep: #06060e
    commands.spawn((
        MenuEntity,
        Camera2d,
        Camera {
            order: 0,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.024, 0.024, 0.055)),
            ..default()
        },
    ));

    // Starfield material
    let material = materials.add(StarfieldMaterial::new());
    commands.insert_resource(StarfieldMaterialHandle(material.clone()));

    // Fullscreen quad: large enough to fill the screen at default camera zoom.
    // Using 2000x2000 to cover any reasonable window size.
    let mesh = meshes.add(Rectangle::new(2000.0, 2000.0));

    commands.spawn((
        MenuEntity,
        Mesh2d(mesh),
        MeshMaterial2d(material),
        Transform::from_xyz(0.0, 0.0, -10.0), // Behind UI
    ));
}

/// Despawn all menu entities when leaving the menu state.
fn despawn_menu_scene(mut commands: Commands, query: Query<Entity, With<MenuEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<StarfieldMaterialHandle>();
}
