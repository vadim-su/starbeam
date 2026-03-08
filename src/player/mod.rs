pub mod aiming;
pub mod animation;
pub mod movement;
pub mod parts;

use bevy::prelude::*;
use bevy::sprite_render::MeshMaterial2d;

use crate::cosmos::warp::NeedsRespawn;
use crate::crafting::{HandCraftState, UnlockedRecipes};
use crate::inventory::{Hotbar, Inventory};
use crate::liquid::registry::LiquidRegistry;
use crate::physics::{Gravity, Submerged, TileCollider};
use crate::registry::biome::PlanetConfig;
use crate::registry::loading::CharacterAnimConfig;
use crate::registry::player::PlayerConfig;
use crate::registry::world::ActiveWorld;
use crate::registry::AppState;
use crate::sets::GameSet;
use crate::world::lit_sprite::{FallbackLightmap, LitSprite, LitSpriteMaterial, SharedLitQuad};
use crate::world::terrain_gen;
use crate::world::terrain_gen::TerrainNoiseCache;

pub use crate::physics::{Grounded, Velocity};

use animation::{AnimationKind, AnimationState, CharacterAnimations};
use parts::{ArmAiming, CharacterPart, PartType};

#[derive(Component)]
pub struct Player;

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
            (
                movement::player_input,
                aiming::arm_aiming_system,
                animation::animate_player,
            )
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
    anim_config: Res<CharacterAnimConfig>,
    quad: Option<Res<SharedLitQuad>>,
    fallback_lm: Res<FallbackLightmap>,
    mut lit_materials: ResMut<Assets<LitSpriteMaterial>>,
    existing_player: Query<Entity, With<Player>>,
) {
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

    // Determine which parts to spawn
    let parts_to_spawn: Vec<PartType> = if anim_config.parts.is_some() {
        PartType::ALL
            .iter()
            .copied()
            .filter(|pt| animations.parts.contains_key(pt))
            .collect()
    } else {
        vec![PartType::Body]
    };

    // Spawn parent entity (physics + inventory, NO rendering components)
    let mut parent = commands.spawn((
        Player,
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
            running_backwards: false,
            facing_locked: false,
        },
        Transform::from_xyz(spawn_pixel_x, spawn_pixel_y, 1.0),
        Visibility::default(),
    ));

    // Spawn child entities for each body part
    parent.with_children(|builder| {
        for &part_type in &parts_to_spawn {
            let frames = animations.frames_for(part_type, AnimationKind::Idle);
            let sprite_handle = if !frames.is_empty() {
                frames[0].clone()
            } else {
                fallback_lm.0.clone()
            };

            let part_cfg = anim_config.parts.as_ref().and_then(|p| p.config_for(part_type));
            let (fw, fh) = part_cfg.map(|c| c.frame_size).unwrap_or(anim_config.sprite_size);
            let (ox, oy) = part_cfg.map(|c| c.offset).unwrap_or((0.0, 0.0));
            let scale = anim_config.render_scale;

            // BackArm gets a darker tint to simulate depth
            let tint = if part_type == PartType::BackArm {
                Vec4::new(0.6, 0.6, 0.6, 1.0)
            } else {
                Vec4::ONE
            };

            let material = lit_materials.add(LitSpriteMaterial {
                sprite: sprite_handle,
                lightmap: fallback_lm.0.clone(),
                lightmap_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
                sprite_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
                submerge_tint: Vec4::ZERO,
                highlight: Vec4::ZERO,
                tint,
            });

            let mut entity_cmd = builder.spawn((
                CharacterPart(part_type),
                LitSprite,
                Mesh2d(quad.0.clone()),
                MeshMaterial2d(material),
                Transform::from_xyz(ox * scale, oy * scale, part_type.z_offset())
                    .with_scale(Vec3::new(fw as f32 * scale, fh as f32 * scale, 1.0)),
            ));

            if part_type.is_arm() {
                let pivot = part_cfg
                    .and_then(|c| c.pivot)
                    .map(|(x, y)| Vec2::new(x, y) * scale)
                    .unwrap_or(Vec2::new(0.0, 5.0 * scale));
                let default_angle = part_cfg
                    .and_then(|c| c.default_angle)
                    .unwrap_or(0.0)
                    .to_radians();
                entity_cmd.insert(ArmAiming {
                    active: false,
                    pivot,
                    default_angle,
                });
            }
        }
    });
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
    player_query: Query<(&Submerged, &Children), With<Player>>,
    part_query: Query<&MeshMaterial2d<LitSpriteMaterial>, With<CharacterPart>>,
) {
    for (sub, children) in &player_query {
        let tint = if sub.ratio < 0.01 || sub.liquid_id.is_none() {
            Vec4::ZERO
        } else {
            let color = liquid_registry
                .get(sub.liquid_id)
                .map(|d| d.color)
                .unwrap_or([0.0; 4]);
            let max_c = color[0].max(color[1]).max(color[2]).max(0.01);
            let tint_r = color[0] / max_c;
            let tint_g = color[1] / max_c;
            let tint_b = color[2] / max_c;
            let strength = (sub.ratio * color[3]).min(0.5);
            Vec4::new(tint_r, tint_g, tint_b, strength)
        };

        for child in children.iter() {
            let Ok(mat_handle) = part_query.get(child) else {
                continue;
            };
            if let Some(mat) = materials.get_mut(&mat_handle.0) {
                mat.submerge_tint = tint;
            }
        }
    }
}
