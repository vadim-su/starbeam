use bevy::prelude::*;

use crate::player::Player;
use crate::registry::world::WorldConfig;
use crate::registry::BiomeParallaxConfigs;
use crate::world::biome_map::BiomeMap;
use crate::world::chunk::world_to_tile;

use super::spawn::ParallaxLayer;

/// Tracks which biome the player is currently in.
#[derive(Resource, Debug)]
pub struct CurrentBiome {
    pub biome_id: String,
}

/// Active parallax crossfade transition.
#[derive(Resource, Debug)]
pub struct ParallaxTransition {
    pub from_biome: String,
    pub to_biome: String,
    pub progress: f32,
    pub duration: f32,
}

const TRANSITION_DURATION: f32 = 1.5;

/// Detect when player enters a new biome region.
#[allow(clippy::too_many_arguments)]
pub fn track_player_biome(
    mut commands: Commands,
    player_query: Query<&Transform, With<Player>>,
    wc: Res<WorldConfig>,
    biome_map: Res<BiomeMap>,
    current_biome: Option<Res<CurrentBiome>>,
    transition: Option<Res<ParallaxTransition>>,
    asset_server: Res<AssetServer>,
    biome_parallax: Res<BiomeParallaxConfigs>,
) {
    let Ok(player_tf) = player_query.single() else {
        return;
    };
    let (tile_x, _) = world_to_tile(
        player_tf.translation.x,
        player_tf.translation.y,
        wc.tile_size,
    );
    let wrapped_x = wc.wrap_tile_x(tile_x) as u32;
    let new_biome = biome_map.biome_at(wrapped_x).to_string();

    // Initialize on first frame
    let Some(current) = current_biome else {
        info!("Initial biome: {}", new_biome);
        spawn_biome_parallax(
            &mut commands,
            &asset_server,
            &biome_parallax,
            &new_biome,
            1.0,
        );
        commands.insert_resource(CurrentBiome {
            biome_id: new_biome,
        });
        return;
    };

    if current.biome_id == new_biome {
        return; // no change
    }

    // If already transitioning and player enters yet another biome:
    // abort current transition
    if transition.is_some() {
        info!("Aborting in-progress transition, new target: {}", new_biome);
        commands.remove_resource::<ParallaxTransition>();
    }

    info!("Biome changed: {} → {}", current.biome_id, new_biome);

    // Spawn new layers for "to" biome with alpha = 0
    spawn_biome_parallax(
        &mut commands,
        &asset_server,
        &biome_parallax,
        &new_biome,
        0.0,
    );

    commands.insert_resource(ParallaxTransition {
        from_biome: current.biome_id.clone(),
        to_biome: new_biome.clone(),
        progress: 0.0,
        duration: TRANSITION_DURATION,
    });

    commands.insert_resource(CurrentBiome {
        biome_id: new_biome,
    });
}

/// Advance crossfade transition each frame.
pub fn parallax_transition_system(
    mut commands: Commands,
    time: Res<Time>,
    mut transition: Option<ResMut<ParallaxTransition>>,
    mut layer_query: Query<(&ParallaxLayer, &mut Sprite)>,
    layer_entity_query: Query<(Entity, &ParallaxLayer)>,
) {
    let Some(ref mut trans) = transition else {
        return;
    };

    trans.progress += time.delta_secs() / trans.duration;

    if trans.progress >= 1.0 {
        // Transition complete — despawn "from" layers, set "to" layers to full alpha
        for (entity, layer) in &layer_entity_query {
            if layer.biome_id == trans.from_biome {
                commands.entity(entity).despawn();
            }
        }
        for (layer, mut sprite) in &mut layer_query {
            if layer.biome_id == trans.to_biome {
                sprite.color = sprite.color.with_alpha(1.0);
            }
        }
        commands.remove_resource::<ParallaxTransition>();
        info!("Parallax transition complete → {}", trans.to_biome);
        return;
    }

    // Update alpha on all parallax layers
    for (layer, mut sprite) in &mut layer_query {
        if layer.biome_id == trans.from_biome {
            sprite.color = sprite.color.with_alpha(1.0 - trans.progress);
        } else if layer.biome_id == trans.to_biome {
            sprite.color = sprite.color.with_alpha(trans.progress);
        }
    }
}

/// Spawn parallax layers for a specific biome.
fn spawn_biome_parallax(
    commands: &mut Commands,
    asset_server: &AssetServer,
    biome_parallax: &BiomeParallaxConfigs,
    biome_id: &str,
    initial_alpha: f32,
) {
    let Some(config) = biome_parallax.configs.get(biome_id) else {
        warn!("No parallax config for biome: {}", biome_id);
        return;
    };

    for layer_def in &config.layers {
        let image_handle: Handle<Image> = asset_server.load(&layer_def.image);
        let color = Color::srgba(1.0, 1.0, 1.0, initial_alpha);

        commands.spawn((
            ParallaxLayer {
                biome_id: biome_id.to_string(),
                speed_x: layer_def.speed_x,
                speed_y: layer_def.speed_y,
                repeat_x: layer_def.repeat_x,
                repeat_y: layer_def.repeat_y,
                texture_size: Vec2::ZERO,
                initialized: false,
            },
            Sprite {
                image: image_handle,
                color,
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, layer_def.z_order),
        ));
    }

    info!(
        "Spawned {} parallax layers for biome '{}'",
        config.layers.len(),
        biome_id
    );
}
