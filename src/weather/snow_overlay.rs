use std::collections::HashSet;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;
use rand::Rng;

use crate::object::registry::ObjectRegistry;
use crate::object::spawn::PlacedObjectEntity;
use crate::registry::biome::BiomeRegistry;
use crate::registry::tile::TileId;
use crate::registry::world::ActiveWorld;
use crate::weather::temperature::local_temperature;
use crate::weather::weather_state::WeatherState;
use crate::world::biome_map::BiomeMap;
use crate::world::chunk::{
    tile_to_chunk, ChunkCoord, ChunkDirty, Layer, LoadedChunks, WorldMap,
};
use crate::world::ctx::WorldCtx;
use crate::world::day_night::WorldTime;

/// Marker component for snow overlay entities.
#[derive(Component)]
pub struct SnowOverlay {
    pub tile_x: i32,
    pub tile_y: i32,
}

/// Resource holding the procedurally generated snow cap texture.
#[derive(Resource)]
pub struct SnowOverlayTexture {
    pub handle: Handle<Image>,
}

/// Timer resource controlling snow overlay update frequency.
#[derive(Resource)]
pub struct SnowOverlayTimer {
    pub timer: Timer,
}

impl Default for SnowOverlayTimer {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(0.5, TimerMode::Repeating),
        }
    }
}

/// Generate a 16x4px snow cap image with an irregular bottom edge.
fn generate_snow_cap_image() -> Image {
    let width = 16u32;
    let height = 4u32;
    let mut data = vec![0u8; (width * height * 4) as usize];

    // Depth pattern per column — how many pixels from the top are filled.
    let depths: [u32; 16] = [2, 3, 3, 4, 4, 3, 4, 4, 3, 3, 4, 4, 3, 4, 3, 2];

    // Slightly blue-tinted white.
    let (r, g, b, a) = (240u8, 245u8, 255u8, 230u8);

    for x in 0..width {
        let depth = depths[x as usize];
        for y in 0..depth.min(height) {
            let idx = ((y * width + x) * 4) as usize;
            data[idx] = r;
            data[idx + 1] = g;
            data[idx + 2] = b;
            data[idx + 3] = a;
        }
    }

    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = bevy::image::ImageSampler::nearest();
    image
}

/// System that creates the [`SnowOverlayTexture`] resource on entering InGame.
pub fn init_snow_overlay_texture(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let handle = images.add(generate_snow_cap_image());
    commands.insert_resource(SnowOverlayTexture { handle });
}

/// System that adds and removes snow overlay sprites based on weather, biome, and tile state.
pub fn update_snow_overlays(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: ResMut<SnowOverlayTimer>,
    camera_q: Query<(&Transform, &Projection), With<Camera2d>>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    world_map: Res<WorldMap>,
    loaded_chunks: Res<LoadedChunks>,
    biome_map: Res<BiomeMap>,
    biome_registry: Res<BiomeRegistry>,
    world: Res<ActiveWorld>,
    weather: Res<WeatherState>,
    world_time: Res<WorldTime>,
    texture: Option<Res<SnowOverlayTexture>>,
    existing: Query<(Entity, &SnowOverlay)>,
    ctx: WorldCtx,
) {
    timer.timer.tick(time.delta());
    if !timer.timer.just_finished() {
        return;
    }

    let Some(texture) = texture else { return };
    let Ok((cam_tf, projection)) = camera_q.single() else {
        return;
    };
    let Projection::Orthographic(ortho) = projection else {
        return;
    };
    let Ok(window) = window_q.single() else {
        return;
    };

    let ctx_ref = ctx.as_ref();
    let tile_size = world.tile_size;
    let chunk_size = world.chunk_size;

    // Camera viewport in tile coordinates.
    let visible_w = window.width() * ortho.scale;
    let visible_h = window.height() * ortho.scale;
    let cam_x = cam_tf.translation.x;
    let cam_y = cam_tf.translation.y;

    let tile_min_x = ((cam_x - visible_w / 2.0) / tile_size).floor() as i32;
    let tile_max_x = ((cam_x + visible_w / 2.0) / tile_size).ceil() as i32;
    let tile_min_y = ((cam_y - visible_h / 2.0) / tile_size).floor() as i32;
    let tile_max_y = ((cam_y + visible_h / 2.0) / tile_size).ceil() as i32;

    let is_precipitating = weather.is_precipitating();

    // Collect existing overlay positions.
    let existing_positions: HashSet<(i32, i32)> = existing
        .iter()
        .map(|(_, o)| (o.tile_x, o.tile_y))
        .collect();

    let mut rng = rand::thread_rng();

    // --- Melting ---
    for (entity, overlay) in existing.iter() {
        let local_temp = local_temperature(overlay.tile_x, &world, &world_time, &biome_map, &biome_registry);
        if local_temp > 2.0 && !is_precipitating && rng.r#gen::<f32>() < 0.10 {
            commands.entity(entity).despawn();
        }
    }

    // --- Adding ---
    for ty in tile_min_y..=tile_max_y {
        for tx in tile_min_x..=tile_max_x {
            let wrapped_tx = world.wrap_tile_x(tx);

            if existing_positions.contains(&(wrapped_tx, ty)) {
                continue;
            }

            // Check biome wants snow.
            let biome_x = wrapped_tx.max(0) as u32;

            let local_temp = local_temperature(wrapped_tx as i32, &world, &world_time, &biome_map, &biome_registry);
            let wants_snow = local_temp < 0.0 && (is_precipitating || local_temp < -5.0);
            if !wants_snow {
                continue;
            }

            // Biome boundary falloff (4-tile zone) for deeply cold biomes
            if local_temp < -5.0 {
                let region_idx = biome_map.region_index_at(biome_x);
                let region = &biome_map.regions[region_idx];
                let dist_from_start = biome_x - region.start_x;
                let dist_from_end = (region.start_x + region.width) - biome_x;
                let min_dist = dist_from_start.min(dist_from_end);
                if min_dist < 4 {
                    let falloff = min_dist as f32 / 4.0;
                    if rng.r#gen::<f32>() > falloff {
                        continue;
                    }
                }
            }

            // Random chance for gradual appearance.
            if local_temp >= -5.0 {
                if rng.r#gen::<f32>() > 0.05 {
                    continue;
                }
            }

            // Tile must be solid and tile above must be air.
            let tile = world_map
                .get_tile(wrapped_tx, ty, Layer::Fg, &ctx_ref)
                .unwrap_or(TileId::AIR);
            if tile == TileId::AIR {
                continue;
            }
            let tile_above = world_map
                .get_tile(wrapped_tx, ty + 1, Layer::Fg, &ctx_ref)
                .unwrap_or(TileId::AIR);
            if tile_above != TileId::AIR {
                continue;
            }

            // Must have a clear path to sky: scan upward from ty+2 to the
            // world top. If any solid tile is found, this block is under a
            // roof and should not receive snow.
            let sky_limit = world.height_tiles as i32;
            let mut has_roof = false;
            for check_y in (ty + 2)..sky_limit {
                let t = world_map
                    .get_tile(wrapped_tx, check_y, Layer::Fg, &ctx_ref)
                    .unwrap_or(TileId::AIR);
                if t != TileId::AIR {
                    has_roof = true;
                    break;
                }
            }
            if has_roof {
                continue;
            }

            // Find the chunk fg entity to parent the overlay to.
            let (cx, cy) = tile_to_chunk(wrapped_tx, ty, chunk_size);
            let wrapped_cx = world.wrap_chunk_x(cx);
            let Some(chunk_entities) = loaded_chunks.map.get(&(wrapped_cx, cy)) else {
                continue;
            };

            // Spawn overlay sprite.
            // Position at top of tile: tile top = (ty + 1) * tile_size.
            // Sprite is 16x4, centered, so shift down by 2px (half height).
            let world_x = wrapped_tx as f32 * tile_size + tile_size / 2.0;
            let world_y = (ty + 1) as f32 * tile_size - 2.0;
            let overlay_entity = commands
                .spawn((
                    SnowOverlay {
                        tile_x: wrapped_tx,
                        tile_y: ty,
                    },
                    Sprite {
                        image: texture.handle.clone(),
                        ..default()
                    },
                    Transform::from_translation(Vec3::new(world_x, world_y, 0.05)),
                ))
                .id();

            commands.entity(chunk_entities.fg).add_child(overlay_entity);
        }
    }
}

/// Marker for snow cap sprites placed on tree canopies.
#[derive(Component)]
pub struct TreeSnowCap {
    pub tree_entity: Entity,
    pub tile_x: i32,
}

/// Marker inserted on tree entities that already have snow caps.
#[derive(Component)]
pub struct HasTreeSnow;

/// System that adds snow cap sprites on top of tree canopies in snowy biomes.
pub fn update_tree_snow(
    mut commands: Commands,
    world: Res<ActiveWorld>,
    biome_map: Res<BiomeMap>,
    biome_registry: Res<BiomeRegistry>,
    weather: Res<WeatherState>,
    world_time: Res<WorldTime>,
    object_registry: Res<ObjectRegistry>,
    texture: Option<Res<SnowOverlayTexture>>,
    trees_without_snow: Query<(Entity, &PlacedObjectEntity, &Transform), Without<HasTreeSnow>>,
    tree_snow_caps: Query<(Entity, &TreeSnowCap)>,
    trees_with_snow: Query<Entity, With<HasTreeSnow>>,
    mut rng_state: Local<Option<u32>>,
) {
    let Some(texture) = texture else { return };
    let tile_size = world.tile_size;
    let is_precipitating = weather.is_precipitating();

    let tree_id = object_registry.by_name("tree_object");

    // --- Melting: remove tree snow when warm and not precipitating ---
    if !is_precipitating {
        let tick = rng_state.get_or_insert(0);
        *tick = tick.wrapping_add(1);
        // 10% chance per tick (matching ground snow melt rate)
        for (cap_entity, cap) in tree_snow_caps.iter() {
            let local_temp = local_temperature(cap.tile_x, &world, &world_time, &biome_map, &biome_registry);
            if local_temp > 2.0 {
                let hash = cap.tree_entity.to_bits().wrapping_mul(2654435761) ^ (*tick as u64);
                if hash % 10 == 0 {
                    commands.entity(cap_entity).despawn();
                    if let Ok(tree_e) = trees_with_snow.get(cap.tree_entity) {
                        commands.entity(tree_e).remove::<HasTreeSnow>();
                    }
                }
            }
        }
    }

    // --- Adding snow to trees ---
    if let Some(tree_obj_id) = tree_id {
        let tree_def = object_registry.get(tree_obj_id);
        let tree_h = tree_def.size.1 as f32;

        for (entity, placed, transform) in trees_without_snow.iter() {
            if placed.object_id != tree_obj_id {
                continue;
            }

            // Check biome wants snow at this tree's X position.
            let tree_tile_x = (transform.translation.x / tile_size).floor() as i32;

            let local_temp = local_temperature(tree_tile_x, &world, &world_time, &biome_map, &biome_registry);
            let wants_snow = local_temp < 0.0 && (is_precipitating || local_temp < -5.0);
            if !wants_snow {
                continue;
            }

            // Tree center is at transform.translation. Top of tree is
            // center_y + tree_h * tile_size / 2.
            let cx = transform.translation.x;
            let top_y = transform.translation.y + tree_h * tile_size / 2.0;

            // Place snow caps on the crown: top center and two side positions.
            // Snow cap is 16x4, so custom_size matches tile width.
            let cap_positions = [
                // Top center
                Vec3::new(cx, top_y - 2.0, 0.05),
                // Left crown (1 tile lower, 1.5 tiles left)
                Vec3::new(cx - 1.5 * tile_size, top_y - tile_size - 2.0, 0.05),
                // Right crown (1 tile lower, 1.5 tiles right)
                Vec3::new(cx + 1.5 * tile_size, top_y - tile_size - 2.0, 0.05),
                // Inner left (2 tiles lower, 0.5 tile left)
                Vec3::new(cx - 0.5 * tile_size, top_y - 0.5 * tile_size - 2.0, 0.05),
                // Inner right (2 tiles lower, 0.5 tile right)
                Vec3::new(cx + 0.5 * tile_size, top_y - 0.5 * tile_size - 2.0, 0.05),
            ];

            for pos in &cap_positions {
                commands.spawn((
                    TreeSnowCap {
                        tree_entity: entity,
                        tile_x: tree_tile_x,
                    },
                    Sprite {
                        image: texture.handle.clone(),
                        ..default()
                    },
                    Transform::from_translation(*pos),
                ));
            }

            commands.entity(entity).insert(HasTreeSnow);
        }
    }
}

/// Cleanup tree snow caps when their parent tree entity is despawned.
pub fn cleanup_tree_snow(
    mut commands: Commands,
    caps: Query<(Entity, &TreeSnowCap)>,
    trees: Query<Entity, With<PlacedObjectEntity>>,
) {
    for (cap_entity, cap) in caps.iter() {
        if trees.get(cap.tree_entity).is_err() {
            commands.entity(cap_entity).despawn();
        }
    }
}

/// System that removes snow overlays from dirty chunks when the underlying tile is no longer valid.
pub fn handle_dirty_chunk_overlays(
    mut commands: Commands,
    dirty_q: Query<&ChunkCoord, With<ChunkDirty>>,
    world_map: Res<WorldMap>,
    world: Res<ActiveWorld>,
    ctx: WorldCtx,
    overlays: Query<(Entity, &SnowOverlay)>,
) {
    let ctx_ref = ctx.as_ref();
    let chunk_size = world.chunk_size as i32;

    // Collect dirty chunk coordinates.
    let dirty_chunks: HashSet<(i32, i32)> = dirty_q.iter().map(|c| (c.x, c.y)).collect();
    if dirty_chunks.is_empty() {
        return;
    }

    for (entity, overlay) in overlays.iter() {
        let (cx, cy) = tile_to_chunk(overlay.tile_x, overlay.tile_y, chunk_size as u32);
        let wrapped_cx = world.wrap_chunk_x(cx);

        if !dirty_chunks.contains(&(wrapped_cx, cy)) {
            continue;
        }

        // Verify underlying tile is still solid and tile above is still air.
        let tile = world_map
            .get_tile(overlay.tile_x, overlay.tile_y, Layer::Fg, &ctx_ref)
            .unwrap_or(TileId::AIR);
        let tile_above = world_map
            .get_tile(overlay.tile_x, overlay.tile_y + 1, Layer::Fg, &ctx_ref)
            .unwrap_or(TileId::AIR);

        if tile == TileId::AIR || tile_above != TileId::AIR {
            commands.entity(entity).despawn();
        }
    }
}
