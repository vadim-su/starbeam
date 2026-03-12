use bevy::prelude::*;
use bevy::sprite_render::MeshMaterial2d;
use bevy::window::PrimaryWindow;

use crate::combat::block_damage::{BlockDamageMap, BlockDamageState};
use crate::particles::pool::ParticlePool;
use crate::cosmos::persistence::{DirtyChunks, DROPPED_ITEM_LIFETIME_SECS};
use crate::cosmos::pressurization::PressureMap;
use crate::crafting::CraftingStation;
use crate::inventory::{Hotbar, Inventory};
use crate::item::{calculate_drops, DropDef, DroppedItem, ItemRegistry, SpawnParams};
use crate::object::definition::ObjectType;
use crate::object::placement::{can_place_object, get_object_at, place_object, remove_object};
use crate::object::plugin::{ObjectAnimation, ObjectSpriteMaterials};
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
use crate::world::rc_lighting::RcGridDirty;

use super::use_item::ItemUsedThisFrame;

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
            submerge_tint: Vec4::ZERO,
            highlight: Vec4::ZERO,
            tint: Vec4::ONE,
        });

        let vel = params.velocity();

        commands.spawn((
            DroppedItem {
                item_id,
                count,
                lifetime: Timer::from_seconds(DROPPED_ITEM_LIFETIME_SECS, TimerMode::Once),
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
    mut player_query: Query<(&Transform, &mut Hotbar, &mut Inventory), With<Player>>,
    ctx: WorldCtx,
    mut world_map: ResMut<WorldMap>,
    loaded_chunks: Res<LoadedChunks>,
    item_registry: Res<ItemRegistry>,
    icon_registry: Res<ItemIconRegistry>,
    quad: Res<SharedLitQuad>,
    fallbacks: (
        Res<FallbackLightmap>,
        Res<FallbackItemImage>,
        ResMut<RcGridDirty>,
        ResMut<DirtyChunks>,
        Option<ResMut<PressureMap>>,
        Res<Time>,
        ResMut<BlockDamageMap>,
    ),
    mut lit_materials: ResMut<Assets<LitSpriteMaterial>>,
    object_registry: Option<Res<ObjectRegistry>>,
    object_sprites: Option<Res<ObjectSpriteMaterials>>,
    object_params: (
        Query<(Entity, &PlacedObjectEntity)>,
        Option<ResMut<crate::liquid::LiquidSimState>>,
        Res<ItemUsedThisFrame>,
        Res<crate::chat::ChatState>,
        ResMut<ParticlePool>,
    ),
) {
    let (object_entities, mut liquid_sim, item_used, chat_state, mut particle_pool) = object_params;

    if chat_state.is_active {
        return;
    }
    let (fallback_lm, fallback_img, mut rc_dirty, mut dirty_chunks, mut pressure_map, time, mut block_damage_map) = fallbacks;
    let left_held = mouse.pressed(MouseButton::Left);
    let right_click = mouse.just_pressed(MouseButton::Right);
    if !left_held && !right_click {
        return;
    }

    if right_click && item_used.0 {
        return;
    }

    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_gt)) = camera_query.single() else {
        return;
    };
    let Ok((player_tf, mut hotbar, mut inventory)) = player_query.single_mut() else {
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

    if left_held {
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
                dirty_chunks.0.insert((data_cx, data_cy));
                return;
            }
        }

        // Foreground layer interaction
        let Some(current) = world_map.get_tile(tile_x, tile_y, Layer::Fg, &ctx_ref) else {
            return;
        };

        if ctx_ref.tile_registry.is_solid(current) {
            // Accumulate mining damage instead of instant break
            let dt = time.delta_secs();
            let tile_def = ctx_ref.tile_registry.get(current);
            let hardness = tile_def.hardness;

            // Get mining_power from active left-hand item, default 1.0
            let mining_power = hotbar.slots[hotbar.active_slot]
                .left_hand
                .as_deref()
                .and_then(|item_id| item_registry.by_name(item_id))
                .and_then(|id| item_registry.get(id).stats.as_ref())
                .and_then(|stats| stats.mining_power)
                .unwrap_or(1.0);

            let state = block_damage_map
                .damage
                .entry((tile_x, tile_y))
                .or_insert(BlockDamageState {
                    accumulated: 0.0,
                    regen_timer: 0.0,
                    particle_timer: 0.0,
                });
            state.accumulated += mining_power * dt;
            state.regen_timer = 0.0;

            state.particle_timer += dt;
            if state.particle_timer >= 0.15 {
                state.particle_timer = 0.0;
                let tile_center = Vec2::new(
                    tile_x as f32 * ctx_ref.config.tile_size + ctx_ref.config.tile_size / 2.0,
                    tile_y as f32 * ctx_ref.config.tile_size + ctx_ref.config.tile_size / 2.0,
                );
                let albedo = ctx_ref.tile_registry.albedo(current);
                let color = [
                    albedo[0] as f32 / 255.0,
                    albedo[1] as f32 / 255.0,
                    albedo[2] as f32 / 255.0,
                    1.0,
                ];
                use rand::Rng;
                let mut rng = rand::thread_rng();
                let count = rng.gen_range(2..=4);
                for _ in 0..count {
                    let vx = rng.gen_range(-30.0..30.0);
                    let vy = rng.gen_range(20.0..60.0);
                    particle_pool.spawn(
                        tile_center,
                        Vec2::new(vx, vy),
                        0.4,   // lifetime
                        1.5,   // size
                        color,
                        1.0,   // gravity_scale
                        true,  // fade_out
                    );
                }
            }

            if state.accumulated >= hardness {
                // Block destroyed
                block_damage_map.damage.remove(&(tile_x, tile_y));

                // Decrement tool durability
                {
                    let active = hotbar.active_slot;
                    let slot = &mut hotbar.slots[active];
                    if let Some(ref mut dur) = slot.left_durability {
                        *dur = dur.saturating_sub(1);
                        if *dur == 0 {
                            slot.left_hand = None;
                            slot.left_durability = None;
                        }
                    }
                }

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
                // Wake liquid neighbors when a solid tile is removed.
                if let Some(ref mut sim) = liquid_sim {
                    sim.sleep.wake_with_neighbors(tile_x, tile_y);
                }
                let wrapped_x = ctx_ref.config.wrap_tile_x(tile_x);
                let (dirty_cx, dirty_cy) =
                    tile_to_chunk(wrapped_x, tile_y, ctx_ref.config.chunk_size);
                dirty_chunks.0.insert((dirty_cx, dirty_cy));
            } else {
                // Damage accumulated but block not yet destroyed — skip post-break logic
                return;
            }
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
                            dirty_chunks.0.insert((data_cx, data_cy));
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

                            // Spawn entity for every display chunk that maps to this data chunk
                            for (&(display_cx, display_cy), _) in &loaded_chunks.map {
                                if ctx_ref.config.wrap_chunk_x(display_cx) == data_cx
                                    && display_cy == data_cy
                                {
                                    let display_offset_x = (display_cx - data_cx) as f32
                                        * ctx_ref.config.chunk_size as f32
                                        * ctx_ref.config.tile_size;

                                    let mut entity_cmd =
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
                                                world_x + offset_x + display_offset_x,
                                                world_y + offset_y,
                                                0.5,
                                            ))
                                            .with_scale(Vec3::new(
                                                def.size.0 as f32 * ctx_ref.config.tile_size,
                                                def.size.1 as f32 * ctx_ref.config.tile_size,
                                                1.0,
                                            )),
                                            Visibility::default(),
                                        ));

                                    // Add CraftingStation component for crafting station objects
                                    if let ObjectType::CraftingStation { ref station_id } =
                                        def.object_type
                                    {
                                        entity_cmd.insert(CraftingStation {
                                            station_id: station_id.clone(),
                                            active_craft: None,
                                        });
                                    }

                                    if let Some(ref sprites) = object_sprites {
                                        if let Some(template_handle) =
                                            sprites.materials.get(&obj_id)
                                        {
                                            let mat_handle = if let Some(meta) =
                                                sprites.animation_meta.get(&obj_id)
                                            {
                                                use rand::Rng;
                                                let mut rng = rand::thread_rng();

                                                let cloned = lit_materials
                                                    .get(template_handle)
                                                    .unwrap()
                                                    .clone();
                                                let handle = lit_materials.add(cloned);

                                                let start_frame =
                                                    rng.gen_range(0..meta.total_frames);
                                                let mut timer = Timer::from_seconds(
                                                    1.0 / meta.fps,
                                                    TimerMode::Repeating,
                                                );
                                                let random_elapsed =
                                                    rng.gen_range(0.0..1.0 / meta.fps);
                                                timer.tick(std::time::Duration::from_secs_f32(
                                                    random_elapsed,
                                                ));

                                                entity_cmd.insert(ObjectAnimation {
                                                    timer,
                                                    current_frame: start_frame,
                                                    total_frames: meta.total_frames,
                                                    columns: meta.columns,
                                                    rows: meta.rows,
                                                });

                                                // Set initial UV for random start frame.
                                                let col = start_frame / meta.rows;
                                                let row = start_frame % meta.rows;
                                                let scale_x = 1.0 / meta.columns as f32;
                                                let scale_y = 1.0 / meta.rows as f32;
                                                if let Some(mat) = lit_materials.get_mut(&handle) {
                                                    mat.sprite_uv_rect = Vec4::new(
                                                        scale_x,
                                                        scale_y,
                                                        col as f32 * scale_x,
                                                        row as f32 * scale_y,
                                                    );
                                                }

                                                handle
                                            } else {
                                                template_handle.clone()
                                            };

                                            entity_cmd.insert((
                                                LitSprite,
                                                Mesh2d(quad.0.clone()),
                                                MeshMaterial2d(mat_handle),
                                            ));
                                        }
                                    }
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

            // Displace liquid when placing a solid tile.
            if let Some(ref mut sim) = liquid_sim {
                let liquid = world_map.get_liquid(tile_x, tile_y, &ctx_ref);
                if !liquid.is_empty() {
                    world_map.set_liquid(
                        tile_x,
                        tile_y,
                        crate::liquid::data::LiquidCell::EMPTY,
                        &ctx_ref,
                    );
                    sim.sleep.wake_with_neighbors(tile_x, tile_y);
                }
            }
            world_map.set_tile(tile_x, tile_y, Layer::Fg, place_id, &ctx_ref);
            let wrapped_x = ctx_ref.config.wrap_tile_x(tile_x);
            let (dirty_cx, dirty_cy) = tile_to_chunk(wrapped_x, tile_y, ctx_ref.config.chunk_size);
            dirty_chunks.0.insert((dirty_cx, dirty_cy));
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
            let wrapped_x = ctx_ref.config.wrap_tile_x(tile_x);
            let (dirty_cx, dirty_cy) = tile_to_chunk(wrapped_x, tile_y, ctx_ref.config.chunk_size);
            dirty_chunks.0.insert((dirty_cx, dirty_cy));
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
            let wrapped_x = ctx_ref.config.wrap_tile_x(tile_x);
            let (dirty_cx, dirty_cy) = tile_to_chunk(wrapped_x, tile_y, ctx_ref.config.chunk_size);
            dirty_chunks.0.insert((dirty_cx, dirty_cy));
            inventory.remove_item(item_id, 1);
        }
    } else {
        return;
    }

    // Notify RC lighting that tiles changed — density/albedo/flat grids
    // must be rebuilt on the next frame.
    rc_dirty.0 = true;

    // Mark pressure map dirty so pressurization is recalculated (ship worlds).
    if let Some(ref mut pm) = pressure_map {
        pm.dirty = true;
    }

    // Update bitmasks for the modified layer
    let modified_layer = if left_held { Layer::Fg } else { Layer::Bg };
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
