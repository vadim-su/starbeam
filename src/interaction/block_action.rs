use bevy::prelude::*;
use bevy::sprite_render::MeshMaterial2d;
use bevy::window::PrimaryWindow;

use crate::inventory::{Hotbar, Inventory};
use crate::item::{calculate_drops, DropDef, DroppedItem, ItemRegistry, SpawnParams};
use crate::object::placement::{can_place_object, get_object_at, place_object, remove_object};
use crate::object::registry::ObjectRegistry;
use crate::object::spawn::{ObjectDisplayChunk, PlacedObjectEntity};
use crate::physics::{Bounce, Friction, Gravity, Grounded, TileCollider, Velocity};
use crate::player::Player;
use crate::registry::tile::TileId;
use crate::ui::game_ui::icon_registry::ItemIconRegistry;
use crate::world::chunk::{
    tile_to_chunk, update_bitmasks_around, world_to_tile, ChunkDirty, Layer, LoadedChunks, WorldMap,
};
use crate::world::ctx::WorldCtx;
use crate::world::lit_sprite::{
    FallbackItemImage, FallbackLightmap, LitSprite, LitSpriteMaterial, SharedLitQuad,
};

/// Dropped item display size in pixels (icons are 16×16).
const DROPPED_ITEM_SIZE: f32 = 16.0;
/// Fallback size for items without an icon.
const DROPPED_ITEM_FALLBACK_SIZE: f32 = 8.0;

/// Spawn dropped items at a tile position with random trajectories and lit-sprite materials.
fn spawn_tile_drops(
    commands: &mut Commands,
    tile_drops: &[DropDef],
    tile_center: Vec2,
    item_registry: &ItemRegistry,
    icon_registry: &ItemIconRegistry,
    quad: &SharedLitQuad,
    fallback_lm: &FallbackLightmap,
    lit_materials: &mut Assets<LitSpriteMaterial>,
    fallback_image: &Handle<Image>,
) {
    let drops = calculate_drops(tile_drops);
    for (item_id, count) in drops {
        let params = SpawnParams::random(tile_center);

        // Resolve sprite texture from icon registry
        let (sprite_image, size) = item_registry
            .by_name(&item_id)
            .and_then(|id| icon_registry.get(id).cloned())
            .map(|img| (img, DROPPED_ITEM_SIZE))
            .unwrap_or_else(|| (fallback_image.clone(), DROPPED_ITEM_FALLBACK_SIZE));

        let material = lit_materials.add(LitSpriteMaterial {
            sprite: sprite_image,
            lightmap: fallback_lm.0.clone(),
            lightmap_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
            sprite_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
        });

        let vel = params.velocity();

        commands.spawn((
            DroppedItem {
                item_id,
                count,
                lifetime: Timer::from_seconds(300.0, TimerMode::Once),
            },
            LitSprite,
            Velocity { x: vel.x, y: vel.y },
            Gravity(400.0),
            Grounded(false),
            TileCollider {
                width: 4.0,
                height: 4.0,
            },
            Friction(0.9),
            Bounce(0.3),
            Mesh2d(quad.0.clone()),
            MeshMaterial2d(material),
            Transform::from_translation(tile_center.extend(1.0))
                .with_scale(Vec3::new(size, size, 1.0)),
        ));
    }
}

const BLOCK_REACH: f32 = 5.0;

#[allow(clippy::too_many_arguments)]
pub fn block_interaction_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut player_query: Query<(&Transform, &Hotbar, &mut Inventory), With<Player>>,
    ctx: WorldCtx,
    mut world_map: ResMut<WorldMap>,
    loaded_chunks: Res<LoadedChunks>,
    item_registry: Res<ItemRegistry>,
    icon_registry: Res<ItemIconRegistry>,
    quad: Res<SharedLitQuad>,
    fallback_lm: Res<FallbackLightmap>,
    fallback_img: Res<FallbackItemImage>,
    mut lit_materials: ResMut<Assets<LitSpriteMaterial>>,
    object_registry: Option<Res<ObjectRegistry>>,
    object_entities: Query<(Entity, &PlacedObjectEntity)>,
) {
    let left_click = mouse.just_pressed(MouseButton::Left);
    let right_click = mouse.just_pressed(MouseButton::Right);
    if !left_click && !right_click {
        return;
    }

    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_gt)) = camera_query.single() else {
        return;
    };
    let Ok((player_tf, hotbar, mut inventory)) = player_query.single_mut() else {
        return;
    };

    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok(world_pos) = camera.viewport_to_world_2d(camera_gt, cursor_pos) else {
        return;
    };

    let ctx_ref = ctx.as_ref();
    let (tile_x, tile_y) = world_to_tile(world_pos.x, world_pos.y, ctx_ref.config.tile_size);

    // Range check (wrap-aware on X axis)
    let player_tile_x = (player_tf.translation.x / ctx_ref.config.tile_size).floor();
    let player_tile_y = (player_tf.translation.y / ctx_ref.config.tile_size).floor();
    let raw_dx = (tile_x as f32 - player_tile_x).abs();
    let dx = raw_dx.min(ctx_ref.config.width_tiles as f32 - raw_dx);
    let dy = (tile_y as f32 - player_tile_y).abs();
    if dx > BLOCK_REACH || dy > BLOCK_REACH {
        return;
    }

    if left_click {
        // Check for object first
        if let Some(ref obj_reg) = object_registry {
            if let Some((anchor_x, anchor_y, obj_idx, obj_id)) =
                get_object_at(&world_map, tile_x, tile_y, &ctx_ref)
            {
                // Break object
                let def = obj_reg.get(obj_id);
                let tile_center = Vec2::new(
                    tile_x as f32 * ctx_ref.config.tile_size + ctx_ref.config.tile_size / 2.0,
                    tile_y as f32 * ctx_ref.config.tile_size + ctx_ref.config.tile_size / 2.0,
                );
                spawn_tile_drops(
                    &mut commands,
                    &def.drops,
                    tile_center,
                    &item_registry,
                    &icon_registry,
                    &quad,
                    &fallback_lm,
                    &mut lit_materials,
                    &fallback_img.0,
                );

                // Despawn the object entity
                let wrapped_ax = ctx_ref.config.wrap_tile_x(anchor_x);
                let (data_cx, data_cy) =
                    tile_to_chunk(wrapped_ax, anchor_y, ctx_ref.config.chunk_size);
                for (entity, placed) in object_entities.iter() {
                    if placed.data_chunk == (data_cx, data_cy) && placed.object_index == obj_idx {
                        commands.entity(entity).despawn();
                    }
                }

                remove_object(
                    &mut world_map,
                    obj_reg,
                    anchor_x,
                    anchor_y,
                    obj_idx,
                    &ctx_ref,
                );
                return;
            }
        }

        // Foreground layer interaction
        let Some(current) = world_map.get_tile(tile_x, tile_y, Layer::Fg, &ctx_ref) else {
            return;
        };

        if ctx_ref.tile_registry.is_solid(current) {
            // Break fg tile
            let tile_def = ctx_ref.tile_registry.get(current);
            let tile_center = Vec2::new(
                tile_x as f32 * ctx_ref.config.tile_size + ctx_ref.config.tile_size / 2.0,
                tile_y as f32 * ctx_ref.config.tile_size + ctx_ref.config.tile_size / 2.0,
            );
            spawn_tile_drops(
                &mut commands,
                &tile_def.drops,
                tile_center,
                &item_registry,
                &icon_registry,
                &quad,
                &fallback_lm,
                &mut lit_materials,
                &fallback_img.0,
            );
            world_map.set_tile(tile_x, tile_y, Layer::Fg, TileId::AIR, &ctx_ref);
        } else {
            // Left-click on air = place from left hand (objects then tiles).
            // This is intentional: left-hand items use left-click, right-hand items use right-click.
            let Some(item_id) = hotbar.slots[hotbar.active_slot].left_hand.as_deref() else {
                return;
            };
            if inventory.count_item(item_id) == 0 {
                return;
            }

            // Check if item places an object
            if let Some(ref obj_reg) = object_registry {
                if let Some(obj_name) = resolve_placeable_object(item_id, &item_registry) {
                    if let Some(obj_id) = obj_reg.by_name(&obj_name) {
                        if can_place_object(&world_map, obj_reg, obj_id, tile_x, tile_y, &ctx_ref) {
                            place_object(&mut world_map, obj_reg, obj_id, tile_x, tile_y, &ctx_ref);
                            inventory.remove_item(item_id, 1);

                            // Spawn entity for the new object
                            let def = obj_reg.get(obj_id);
                            let wrapped_x = ctx_ref.config.wrap_tile_x(tile_x);
                            let (data_cx, data_cy) =
                                tile_to_chunk(wrapped_x, tile_y, ctx_ref.config.chunk_size);
                            let chunk = world_map.chunk(data_cx, data_cy).unwrap();
                            let new_idx = (chunk.objects.len() - 1) as u16;

                            let world_x = tile_x as f32 * ctx_ref.config.tile_size
                                + ctx_ref.config.tile_size / 2.0;
                            let world_y = tile_y as f32 * ctx_ref.config.tile_size
                                + ctx_ref.config.tile_size / 2.0;
                            let offset_x =
                                (def.size.0 as f32 - 1.0) * ctx_ref.config.tile_size / 2.0;
                            let offset_y =
                                (def.size.1 as f32 - 1.0) * ctx_ref.config.tile_size / 2.0;

                            // Find display chunk for this data chunk
                            for (&(display_cx, display_cy), _) in &loaded_chunks.map {
                                if ctx_ref.config.wrap_chunk_x(display_cx) == data_cx
                                    && display_cy == data_cy
                                {
                                    commands.spawn((
                                        PlacedObjectEntity {
                                            data_chunk: (data_cx, data_cy),
                                            object_index: new_idx,
                                            object_id: obj_id,
                                        },
                                        ObjectDisplayChunk {
                                            display_chunk: (display_cx, data_cy),
                                        },
                                        Transform::from_translation(Vec3::new(
                                            world_x + offset_x,
                                            world_y + offset_y,
                                            0.5,
                                        )),
                                        Visibility::default(),
                                    ));
                                    break;
                                }
                            }
                            return;
                        }
                    }
                }
            }

            // Fall back to tile placement
            let has_neighbor = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| {
                let nx = tile_x + dx;
                let ny = tile_y + dy;
                world_map
                    .get_tile(nx, ny, Layer::Fg, &ctx_ref)
                    .is_some_and(|t| ctx_ref.tile_registry.is_solid(t))
                    || world_map
                        .get_tile(nx, ny, Layer::Bg, &ctx_ref)
                        .is_some_and(|t| t != TileId::AIR)
            });
            if !has_neighbor {
                return;
            }

            let Some(place_id) = resolve_placeable(item_id, &item_registry, &ctx_ref) else {
                return;
            };

            world_map.set_tile(tile_x, tile_y, Layer::Fg, place_id, &ctx_ref);
            inventory.remove_item(item_id, 1);
        }
    } else if right_click {
        // Background layer interaction
        let Some(current_bg) = world_map.get_tile(tile_x, tile_y, Layer::Bg, &ctx_ref) else {
            return;
        };

        if current_bg != TileId::AIR {
            // Break bg tile
            let tile_def = ctx_ref.tile_registry.get(current_bg);
            let tile_center = Vec2::new(
                tile_x as f32 * ctx_ref.config.tile_size + ctx_ref.config.tile_size / 2.0,
                tile_y as f32 * ctx_ref.config.tile_size + ctx_ref.config.tile_size / 2.0,
            );
            spawn_tile_drops(
                &mut commands,
                &tile_def.drops,
                tile_center,
                &item_registry,
                &icon_registry,
                &quad,
                &fallback_lm,
                &mut lit_materials,
                &fallback_img.0,
            );
            world_map.set_tile(tile_x, tile_y, Layer::Bg, TileId::AIR, &ctx_ref);
        } else {
            // Place bg tile from right hand of active hotbar slot
            let has_neighbor = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| {
                let nx = tile_x + dx;
                let ny = tile_y + dy;
                world_map
                    .get_tile(nx, ny, Layer::Fg, &ctx_ref)
                    .is_some_and(|t| t != TileId::AIR)
                    || world_map
                        .get_tile(nx, ny, Layer::Bg, &ctx_ref)
                        .is_some_and(|t| t != TileId::AIR)
            });
            if !has_neighbor {
                return;
            }

            let Some(item_id) = hotbar.slots[hotbar.active_slot].right_hand.as_deref() else {
                return;
            };
            let Some(place_id) = resolve_placeable(item_id, &item_registry, &ctx_ref) else {
                return;
            };
            if inventory.count_item(item_id) == 0 {
                return;
            }

            world_map.set_tile(tile_x, tile_y, Layer::Bg, place_id, &ctx_ref);
            inventory.remove_item(item_id, 1);
        }
    } else {
        return;
    }

    // Update bitmasks for the modified layer
    let modified_layer = if left_click { Layer::Fg } else { Layer::Bg };
    let bitmask_dirty =
        update_bitmasks_around(&mut world_map, tile_x, tile_y, modified_layer, &ctx_ref);

    let all_dirty = bitmask_dirty;

    for (cx, cy) in all_dirty {
        for (&(display_cx, display_cy), entities) in &loaded_chunks.map {
            if ctx_ref.config.wrap_chunk_x(display_cx) == cx && display_cy == cy {
                commands.entity(entities.fg).insert(ChunkDirty);
                commands.entity(entities.bg).insert(ChunkDirty);
            }
        }
    }
}

/// Look up item_id → placeable_object name. Returns None if not an object placer.
fn resolve_placeable_object(item_id: &str, item_registry: &ItemRegistry) -> Option<String> {
    let item_def_id = item_registry.by_name(item_id)?;
    let item_def = item_registry.get(item_def_id);
    item_def.placeable_object.clone()
}

/// Look up item_id → placeable tile name → TileId. Returns None if not placeable.
fn resolve_placeable(
    item_id: &str,
    item_registry: &ItemRegistry,
    ctx: &crate::world::ctx::WorldCtxRef<'_>,
) -> Option<TileId> {
    let item_def_id = item_registry.by_name(item_id)?;
    let item_def = item_registry.get(item_def_id);
    let tile_name = item_def.placeable.as_deref()?;
    Some(ctx.tile_registry.by_name(tile_name))
}
