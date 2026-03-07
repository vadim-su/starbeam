pub mod animation;
pub mod movement;

use bevy::prelude::*;
use bevy::sprite_render::MeshMaterial2d;

use crate::cosmos::warp::NeedsRespawn;
use crate::crafting::{HandCraftState, UnlockedRecipes};
use crate::inventory::{Hotbar, Inventory};
use crate::liquid::registry::LiquidRegistry;
use crate::physics::{Gravity, Submerged, TileCollider};
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
                spawn_player.after(crate::world::lit_sprite::init_lit_sprite_resources),
                respawn_player_on_warp,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (movement::player_input, animation::animate_player)
                .chain()
                .in_set(GameSet::Physics),
        )
        .add_systems(Update, update_submerge_tint.in_set(GameSet::Physics));
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
    quad: Option<Res<SharedLitQuad>>,
    fallback_lm: Res<FallbackLightmap>,
    mut lit_materials: ResMut<Assets<LitSpriteMaterial>>,
    existing_player: Query<Entity, With<Player>>,
) {
    // Skip if player already exists (e.g. after warp — respawn_player handles repositioning)
    if existing_player.iter().next().is_some() {
        return;
    }

    let Some(quad) = quad else {
        warn!("SharedLitQuad not ready yet, deferring player spawn");
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

    let material = lit_materials.add(LitSpriteMaterial {
        sprite: animations.idle[0].clone(),
        lightmap: fallback_lm.0.clone(),
        lightmap_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
        sprite_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
        submerge_tint: Vec4::ZERO,
        highlight: Vec4::ZERO,
    });

    commands.spawn((
        Player,
        LitSprite,
        {
            let mut inv = Inventory::new();
            inv.try_add_item("torch", 10, 999, crate::inventory::BagTarget::Main);
            inv.try_add_item("workbench", 1, 10, crate::inventory::BagTarget::Main);
            inv
        },
        Hotbar::new(),
        HandCraftState::default(),
        UnlockedRecipes::default(),
        Velocity::default(),
        Gravity(player_config.gravity),
        Grounded(false),
        Submerged::default(),
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
///
/// `pub(crate)` so that other plugins can order their `OnEnter` systems
/// after this one (e.g. `snap_camera_to_player`).
pub(crate) fn respawn_player_on_warp(
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

/// Update the player sprite's submersion tint based on the `Submerged` component.
/// Applies a multiplicative hue shift simulating view through liquid.
fn update_submerge_tint(
    liquid_registry: Res<LiquidRegistry>,
    mut materials: ResMut<Assets<LitSpriteMaterial>>,
    query: Query<(&Submerged, &MeshMaterial2d<LitSpriteMaterial>), With<Player>>,
) {
    for (sub, mat_handle) in &query {
        let Some(mat) = materials.get_mut(&mat_handle.0) else {
            continue;
        };
        if sub.ratio < 0.01 || sub.liquid_id.is_none() {
            mat.submerge_tint = Vec4::ZERO;
            continue;
        }
        let color = liquid_registry
            .get(sub.liquid_id)
            .map(|d| d.color)
            .unwrap_or([0.0; 4]);
        // Normalize liquid color to unit brightness so the tint shifts hue
        // without overall darkening. E.g. water (0.2, 0.4, 0.8) → (0.25, 0.5, 1.0).
        let max_c = color[0].max(color[1]).max(color[2]).max(0.01);
        let tint_r = color[0] / max_c;
        let tint_g = color[1] / max_c;
        let tint_b = color[2] / max_c;
        // Strength scales with submersion ratio. Capped to keep the effect subtle.
        let strength = (sub.ratio * color[3]).min(0.5);
        mat.submerge_tint = Vec4::new(tint_r, tint_g, tint_b, strength);
    }
}
