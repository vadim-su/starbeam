pub mod animation;
pub mod movement;
pub mod wrap;

use bevy::prelude::*;
use bevy::sprite_render::MeshMaterial2d;

use crate::cosmos::warp::NeedsRespawn;
use crate::inventory::{Hotbar, Inventory};
use crate::physics::{Gravity, TileCollider};
use crate::registry::biome::PlanetConfig;
use crate::registry::player::PlayerConfig;
use crate::registry::world::ActiveWorld;
use crate::registry::AppState;
use crate::sets::GameSet;
use crate::world::lit_sprite::{FallbackLightmap, LitSprite, LitSpriteMaterial, SharedLitQuad};
use crate::world::terrain_gen;
use crate::world::terrain_gen::TerrainNoiseCache;

pub use crate::physics::{Grounded, Velocity};

use animation::{AnimationKind, AnimationState, CharacterAnimations};

#[derive(Component)]
pub struct Player;

/// Player sprite pixel dimensions (44×44 adventurer frames).
const PLAYER_SPRITE_SIZE: f32 = 44.0;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(AppState::InGame),
            (
                animation::load_character_animations,
                spawn_player,
                respawn_player_on_warp,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                movement::player_input,
                wrap::player_wrap_system,
                animation::animate_player,
            )
                .chain()
                .in_set(GameSet::Physics),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_player(
    mut commands: Commands,
    player_config: Res<PlayerConfig>,
    world_config: Res<ActiveWorld>,
    planet_config: Res<PlanetConfig>,
    noise_cache: Res<TerrainNoiseCache>,
    animations: Res<CharacterAnimations>,
    quad: Res<SharedLitQuad>,
    fallback_lm: Res<FallbackLightmap>,
    mut lit_materials: ResMut<Assets<LitSpriteMaterial>>,
    existing_player: Query<Entity, With<Player>>,
) {
    // Skip if player already exists (e.g. after warp — respawn_player handles repositioning)
    if existing_player.iter().next().is_some() {
        return;
    }

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

    let material = lit_materials.add(LitSpriteMaterial {
        sprite: animations.idle[0].clone(),
        lightmap: fallback_lm.0.clone(),
        lightmap_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
        sprite_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
    });

    commands.spawn((
        Player,
        LitSprite,
        {
            let mut inv = Inventory::new();
            inv.try_add_item("torch", 10, 999, crate::inventory::BagTarget::Main);
            inv
        },
        Hotbar::new(),
        Velocity::default(),
        Gravity(player_config.gravity),
        Grounded(false),
        TileCollider {
            width: player_config.width,
            height: player_config.height,
        },
        AnimationState {
            kind: AnimationKind::Idle,
            frame: 0,
            timer: Timer::from_seconds(0.15, TimerMode::Repeating),
            facing_right: true,
        },
        Mesh2d(quad.0.clone()),
        MeshMaterial2d(material),
        Transform::from_xyz(spawn_pixel_x, spawn_pixel_y, 1.0).with_scale(Vec3::new(
            PLAYER_SPRITE_SIZE,
            PLAYER_SPRITE_SIZE,
            1.0,
        )),
    ));
}

/// After a warp, teleport the existing player to the new world's surface.
/// Runs on `OnEnter(InGame)` — only acts when `NeedsRespawn` marker exists.
fn respawn_player_on_warp(
    mut commands: Commands,
    needs_respawn: Option<Res<NeedsRespawn>>,
    world_config: Res<ActiveWorld>,
    planet_config: Res<PlanetConfig>,
    noise_cache: Res<TerrainNoiseCache>,
    player_config: Res<PlayerConfig>,
    mut player_query: Query<(&mut Transform, &mut Velocity), With<Player>>,
) {
    if needs_respawn.is_none() {
        return;
    }

    let Ok((mut transform, mut velocity)) = player_query.single_mut() else {
        return;
    };

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

    transform.translation.x = spawn_pixel_x;
    transform.translation.y = spawn_pixel_y;
    *velocity = Velocity::default();

    commands.remove_resource::<NeedsRespawn>();

    info!(
        "Player respawned at tile ({}, {}) → pixel ({:.0}, {:.0})",
        spawn_tile_x, surface_y, spawn_pixel_x, spawn_pixel_y
    );
}
