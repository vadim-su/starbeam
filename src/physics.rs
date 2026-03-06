use bevy::prelude::*;

use crate::liquid::data::{LiquidCell, LiquidId};
use crate::liquid::registry::LiquidRegistry;
use crate::math::{tile_aabb, Aabb};
use crate::object::registry::ObjectRegistry;
use crate::registry::player::PlayerConfig;
use crate::sets::GameSet;
use crate::world::chunk::{self, WorldMap};
use crate::world::ctx::WorldCtx;

/// Maximum delta time to prevent physics tunneling on lag spikes.
pub const MAX_DELTA_SECS: f32 = 1.0 / 20.0;

/// Minimum bounced velocity to actually bounce; below this the entity lands.
const BOUNCE_THRESHOLD: f32 = 5.0;

/// Horizontal velocity damping applied on each bounce.
const BOUNCE_HORIZONTAL_DAMPING: f32 = 0.9;

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// Linear velocity in pixels per second.
#[derive(Component, Default, Debug)]
pub struct Velocity {
    pub x: f32,
    pub y: f32,
}

/// Downward acceleration in pixels per second squared.
#[derive(Component, Debug)]
pub struct Gravity(pub f32);

/// Whether the entity is resting on a solid surface.
#[derive(Component, Debug)]
pub struct Grounded(pub bool);

/// Axis-aligned collider dimensions (width, height) for tile collision.
#[derive(Component, Debug)]
pub struct TileCollider {
    pub width: f32,
    pub height: f32,
}

/// Horizontal velocity damping factor applied each frame while grounded.
/// 0.0 = instant stop, 1.0 = no friction.
#[derive(Component, Debug)]
pub struct Friction(pub f32);

/// Coefficient of restitution for ground bounces.
/// 0.0 = no bounce, 1.0 = perfectly elastic.
#[derive(Component, Debug)]
pub struct Bounce(pub f32);

/// Gentle vertical oscillation while grounded (e.g. dropped items).
#[derive(Component, Debug)]
pub struct BobEffect {
    pub amplitude: f32,
    pub speed: f32,
    pub phase: f32,
    pub rest_y: f32,
}

/// Tracks how much of the entity is submerged in liquid.
/// Updated by the liquid detection system each frame.
#[derive(Component, Debug, Default)]
pub struct Submerged {
    /// 0.0 = not submerged, 1.0 = fully submerged.
    pub ratio: f32,
    /// The dominant liquid type the entity is in (by fill weight).
    pub liquid_id: LiquidId,
    /// The swim_speed_factor of the dominant liquid.
    pub swim_speed_factor: f32,
}

impl Submerged {
    /// Threshold above which the entity is considered "swimming".
    pub const SWIM_THRESHOLD: f32 = 0.3;

    pub fn is_swimming(&self) -> bool {
        self.ratio >= Self::SWIM_THRESHOLD
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                apply_gravity,
                tile_collision,
                update_submersion,
                apply_friction,
                apply_bob,
            )
                .chain()
                .in_set(GameSet::Physics),
        );
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Apply gravitational acceleration to all entities with `Velocity` + `Gravity`.
/// If the entity has a `Submerged` component and is swimming, gravity is reduced
/// by the configured `swim_gravity_factor`.
pub fn apply_gravity(
    time: Res<Time>,
    player_config: Option<Res<PlayerConfig>>,
    mut query: Query<(&mut Velocity, &Gravity, Option<&Submerged>)>,
) {
    let dt = time.delta_secs().min(MAX_DELTA_SECS);
    for (mut vel, gravity, submerged) in &mut query {
        let gravity_factor = match submerged {
            Some(sub) if sub.is_swimming() => player_config
                .as_ref()
                .map(|c| c.swim_gravity_factor)
                .unwrap_or(0.3),
            _ => 1.0,
        };
        vel.y -= gravity.0 * gravity_factor * dt;
    }
}

/// Resolve tile collisions for all entities with `TileCollider`.
///
/// Axes are resolved independently (X then Y) to prevent corner sticking.
/// Optional `Grounded` is set when the entity lands on a solid tile.
/// Optional `Bounce` causes the entity to bounce off the ground.
/// Optional `BobEffect` is paused during physics and resumed after resolution.
pub fn tile_collision(
    time: Res<Time>,
    ctx: WorldCtx,
    world_map: Res<WorldMap>,
    object_registry: Option<Res<ObjectRegistry>>,
    mut query: Query<(
        &mut Transform,
        &mut Velocity,
        &TileCollider,
        Option<&mut Grounded>,
        Option<&Bounce>,
        Option<&mut BobEffect>,
    )>,
) {
    let dt = time.delta_secs().min(MAX_DELTA_SECS);
    let ts = ctx.config.tile_size;
    let ctx_ref = ctx.as_ref();

    let is_solid = |tx: i32, ty: i32| -> bool {
        match &object_registry {
            Some(reg) => world_map.is_solid_or_object(tx, ty, &ctx_ref, reg),
            None => world_map.is_solid(tx, ty, &ctx_ref),
        }
    };

    for (mut tf, mut vel, collider, mut grounded, bounce, mut bob) in &mut query {
        let pos = &mut tf.translation;
        let w = collider.width;
        let h = collider.height;

        // Remove bob offset before physics so collision uses the true rest position
        if let Some(ref bob) = bob {
            if grounded.as_ref().is_some_and(|g| g.0) {
                pos.y = bob.rest_y;
            }
        }

        // --- Resolve X axis ---
        pos.x += vel.x * dt;
        let aabb = Aabb::from_center(pos.x, pos.y, w, h);
        for (tx, ty) in aabb.overlapping_tiles(ts) {
            if is_solid(tx, ty) {
                let tile = tile_aabb(tx, ty, ts);
                let entity_aabb = Aabb::from_center(pos.x, pos.y, w, h);
                if entity_aabb.overlaps(&tile) {
                    if vel.x > 0.0 {
                        pos.x = tile.min_x - w / 2.0;
                    } else if vel.x < 0.0 {
                        pos.x = tile.max_x + w / 2.0;
                    }
                    vel.x = 0.0;
                }
            }
        }

        // --- Resolve Y axis ---
        pos.y += vel.y * dt;
        if let Some(ref mut g) = grounded {
            g.0 = false;
        }
        let aabb = Aabb::from_center(pos.x, pos.y, w, h);
        for (tx, ty) in aabb.overlapping_tiles(ts) {
            if is_solid(tx, ty) {
                let tile = tile_aabb(tx, ty, ts);
                let entity_aabb = Aabb::from_center(pos.x, pos.y, w, h);
                if entity_aabb.overlaps(&tile) {
                    if vel.y < 0.0 {
                        pos.y = tile.max_y + h / 2.0;
                        let bounce_factor = bounce.map(|b| b.0).unwrap_or(0.0);
                        let bounced = -vel.y * bounce_factor;
                        if bounced > BOUNCE_THRESHOLD {
                            vel.y = bounced;
                            vel.x *= BOUNCE_HORIZONTAL_DAMPING;
                        } else {
                            vel.y = 0.0;
                            if let Some(ref mut g) = grounded {
                                g.0 = true;
                            }
                        }
                    } else if vel.y > 0.0 {
                        pos.y = tile.min_y - h / 2.0;
                        vel.y = 0.0;
                    }
                }
            }
        }

        // Store rest_y for bob after collision resolution
        if let Some(ref mut bob) = bob {
            if grounded.as_ref().is_some_and(|g| g.0) {
                bob.rest_y = pos.y;
            }
        }
    }
}

/// Detect liquid submersion for all entities with `TileCollider` + `Submerged`.
///
/// This is a pure detection system — it writes the `Submerged` component
/// but applies no forces. Forces are handled by the movement system
/// (player_input) and gravity, which read `Submerged` to adjust behavior.
pub fn update_submersion(
    ctx: WorldCtx,
    world_map: Res<WorldMap>,
    liquid_registry: Res<LiquidRegistry>,
    mut query: Query<(&Transform, &TileCollider, &mut Submerged)>,
) {
    if liquid_registry.defs.is_empty() {
        return;
    }
    let ts = ctx.config.tile_size;
    let ctx_ref = ctx.as_ref();

    for (tf, collider, mut sub) in &mut query {
        let pos = tf.translation;
        let w = collider.width;
        let h = collider.height;
        let aabb = Aabb::from_center(pos.x, pos.y, w, h);

        let mut total_fill: f32 = 0.0;
        let mut best_fill: f32 = 0.0;
        let mut best_liquid = LiquidId::NONE;
        let mut best_swim_factor: f32 = 1.0;
        let mut max_damage: f32 = 0.0;

        for (tx, ty) in aabb.overlapping_tiles(ts) {
            let cell = {
                let wtx = ctx_ref.config.wrap_tile_x(tx);
                if ty < 0 || ty >= ctx_ref.config.height_tiles {
                    LiquidCell::EMPTY
                } else {
                    let (cx, cy) = chunk::tile_to_chunk(wtx, ty, ctx_ref.config.chunk_size);
                    let (lx, ly) = chunk::tile_to_local(wtx, ty, ctx_ref.config.chunk_size);
                    match world_map.chunk(cx, cy) {
                        Some(chunk) => chunk.liquid.get(lx, ly, ctx_ref.config.chunk_size),
                        None => LiquidCell::EMPTY,
                    }
                }
            };
            if cell.is_empty() {
                continue;
            }

            if let Some(def) = liquid_registry.get(cell.liquid_type) {
                let fill = cell.level.clamp(0.0, 1.0);
                total_fill += fill;
                if fill > best_fill {
                    best_fill = fill;
                    best_liquid = cell.liquid_type;
                    best_swim_factor = def.swim_speed_factor;
                }
                max_damage = max_damage.max(def.damage_on_contact);
            }
        }

        let entity_tile_height = (h / ts).max(1.0);
        sub.ratio = (total_fill / entity_tile_height).clamp(0.0, 1.0);
        sub.liquid_id = best_liquid;
        sub.swim_speed_factor = if best_liquid.is_none() {
            1.0
        } else {
            best_swim_factor
        };

        // TODO: Apply damage_on_contact to player health when health system exists.
        let _ = max_damage;
    }
}

/// Damp horizontal velocity while grounded.
pub fn apply_friction(mut query: Query<(&mut Velocity, &Grounded, &Friction)>) {
    for (mut vel, grounded, friction) in &mut query {
        if grounded.0 {
            vel.x *= friction.0;
        }
    }
}

/// Oscillate grounded entities vertically for a gentle bob animation.
pub fn apply_bob(time: Res<Time>, mut query: Query<(&mut Transform, &mut BobEffect, &Grounded)>) {
    let dt = time.delta_secs();
    for (mut tf, mut bob, grounded) in &mut query {
        if grounded.0 {
            bob.phase += bob.speed * dt;
            tf.translation.y = bob.rest_y + bob.amplitude * bob.phase.sin();
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::fixtures;
    use crate::world::chunk::WorldMap;
    use crate::world::terrain_gen;

    // -----------------------------------------------------------------------
    // Gravity tests
    // -----------------------------------------------------------------------

    #[test]
    fn gravity_system_pulls_entity_down() {
        let mut app = fixtures::test_app();
        app.add_systems(Update, apply_gravity);

        app.world_mut()
            .spawn((Velocity { x: 0.0, y: 0.0 }, Gravity(980.0)));

        // First update initialises Time (dt=0); sleep then second update gives real dt.
        app.update();
        std::thread::sleep(std::time::Duration::from_millis(50));
        app.update();

        let mut query = app.world_mut().query::<&Velocity>();
        let vel = query.iter(app.world()).next().unwrap();
        assert!(
            vel.y < 0.0,
            "gravity should pull velocity downward, got {}",
            vel.y
        );
    }

    #[test]
    fn gravity_does_not_affect_entity_without_gravity_component() {
        let mut app = fixtures::test_app();
        app.add_systems(Update, apply_gravity);

        app.world_mut().spawn(Velocity { x: 10.0, y: 5.0 });

        app.update();
        std::thread::sleep(std::time::Duration::from_millis(50));
        app.update();

        let mut query = app.world_mut().query::<&Velocity>();
        let vel = query.iter(app.world()).next().unwrap();
        assert_eq!(vel.x, 10.0, "x should be unchanged");
        assert_eq!(vel.y, 5.0, "y should be unchanged without Gravity");
    }

    // -----------------------------------------------------------------------
    // Tile collision tests
    // -----------------------------------------------------------------------

    #[test]
    fn collision_does_not_crash_on_empty_world() {
        let mut app = fixtures::test_app();
        app.add_systems(Update, tile_collision);

        app.world_mut().spawn((
            Transform::from_xyz(500.0, 30000.0, 0.0),
            Velocity { x: 0.0, y: -100.0 },
            TileCollider {
                width: 24.0,
                height: 40.0,
            },
            Grounded(false),
        ));

        // Should not panic — WorldMap is empty, is_solid returns false
        app.update();

        let mut query = app.world_mut().query::<&Grounded>();
        let grounded = query.iter(app.world()).next().unwrap();
        assert!(!grounded.0, "should not be grounded in empty world");
    }

    #[test]
    fn collision_grounds_entity_on_solid_surface() {
        let mut app = fixtures::test_app();
        app.add_systems(Update, tile_collision);

        // Pre-generate chunks around surface using separate resource copies
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let surface_y = terrain_gen::surface_height(
            &nc,
            0,
            &wc,
            pc.layers.surface.terrain_frequency,
            pc.layers.surface.terrain_amplitude,
        );
        let chunk_size = wc.chunk_size as i32;
        let surface_chunk_y = surface_y.div_euclid(chunk_size);

        let mut world_map = WorldMap::default();
        // Generate a few chunks around the surface
        for cy in (surface_chunk_y - 1)..=(surface_chunk_y + 1) {
            world_map.get_or_generate_chunk(0, cy, &ctx);
        }

        // Insert pre-generated WorldMap
        *app.world_mut().resource_mut::<WorldMap>() = world_map;

        // Position entity so bottom edge is slightly INSIDE the surface tile.
        // Surface tile spans [surface_y * ts .. (surface_y+1) * ts].
        // Entity bottom = pos.y - h/2; we want it 2px inside the tile top.
        let tile_size = wc.tile_size;
        let entity_height = 40.0;
        let spawn_y = (surface_y + 1) as f32 * tile_size + entity_height / 2.0 - 2.0;

        app.world_mut().spawn((
            Transform::from_xyz(tile_size / 2.0, spawn_y, 0.0),
            Velocity { x: 0.0, y: -200.0 },
            TileCollider {
                width: 24.0,
                height: entity_height,
            },
            Grounded(false),
        ));

        // Even with dt=0, collision resolves because entity already overlaps
        // the surface tile and vel.y < 0.
        app.update();

        let mut query = app.world_mut().query::<&Grounded>();
        let grounded = query.iter(app.world()).next().unwrap();
        assert!(
            grounded.0,
            "entity should be grounded after landing on solid surface"
        );
    }

    #[test]
    fn collision_works_without_grounded_component() {
        let mut app = fixtures::test_app();
        app.add_systems(Update, tile_collision);

        // Entity with TileCollider but NO Grounded — should not panic
        app.world_mut().spawn((
            Transform::from_xyz(500.0, 30000.0, 0.0),
            Velocity { x: 0.0, y: -100.0 },
            TileCollider {
                width: 16.0,
                height: 16.0,
            },
        ));

        // Should not panic
        app.update();
    }

    // -----------------------------------------------------------------------
    // Friction tests
    // -----------------------------------------------------------------------

    #[test]
    fn friction_slows_grounded_entity() {
        let mut app = fixtures::test_app();
        app.add_systems(Update, apply_friction);

        app.world_mut()
            .spawn((Velocity { x: 100.0, y: 0.0 }, Grounded(true), Friction(0.9)));

        app.update();

        let mut query = app.world_mut().query::<&Velocity>();
        let vel = query.iter(app.world()).next().unwrap();
        assert!(
            (vel.x - 90.0).abs() < 0.01,
            "friction should reduce vel.x from 100 to 90, got {}",
            vel.x
        );
    }

    #[test]
    fn friction_does_not_affect_airborne_entity() {
        let mut app = fixtures::test_app();
        app.add_systems(Update, apply_friction);

        app.world_mut().spawn((
            Velocity { x: 100.0, y: 0.0 },
            Grounded(false),
            Friction(0.9),
        ));

        app.update();

        let mut query = app.world_mut().query::<&Velocity>();
        let vel = query.iter(app.world()).next().unwrap();
        assert_eq!(vel.x, 100.0, "airborne entity should keep full velocity");
    }

    // -----------------------------------------------------------------------
    // Bob tests
    // -----------------------------------------------------------------------

    #[test]
    fn bob_oscillates_grounded_entity() {
        let mut app = fixtures::test_app();
        app.add_systems(Update, apply_bob);

        let rest_y = 100.0;
        app.world_mut().spawn((
            Transform::from_xyz(0.0, rest_y, 0.0),
            BobEffect {
                amplitude: 3.0,
                speed: 5.0,
                phase: 0.0,
                rest_y,
            },
            Grounded(true),
        ));

        // First update initialises Time (dt=0); sleep then second update gives real dt.
        app.update();
        std::thread::sleep(std::time::Duration::from_millis(50));
        app.update();

        let mut q_bob = app.world_mut().query::<&BobEffect>();
        let bob = q_bob.iter(app.world()).next().unwrap();
        assert!(bob.phase > 0.0, "phase should advance");

        let mut q_tf = app.world_mut().query::<&Transform>();
        let tf = q_tf.iter(app.world()).next().unwrap();
        // Y should differ from rest_y because sin(phase) != 0
        assert!(
            (tf.translation.y - rest_y).abs() > 0.001,
            "bob should offset Y from rest, got {}",
            tf.translation.y
        );
    }

    // -----------------------------------------------------------------------
    // Bounce tests
    // -----------------------------------------------------------------------

    #[test]
    fn bounce_reverses_velocity_on_ground_hit() {
        // Pure logic: falling velocity with bounce factor should reverse
        let fall_speed = 200.0_f32;
        let bounce_factor = 0.5;
        let bounced = fall_speed * bounce_factor;
        assert_eq!(bounced, 100.0);
        assert!(bounced > BOUNCE_THRESHOLD, "should bounce, not land");
    }

    #[test]
    fn bounce_below_threshold_grounds_entity() {
        // Pure logic: very slow fall with bounce should land instead of bounce
        let fall_speed = 8.0_f32;
        let bounce_factor = 0.3;
        let bounced = fall_speed * bounce_factor;
        assert!(bounced < BOUNCE_THRESHOLD, "should land, not bounce");
    }

    #[test]
    fn bounce_damps_horizontal_velocity() {
        let vel_x = 100.0_f32;
        let damped = vel_x * BOUNCE_HORIZONTAL_DAMPING;
        assert!((damped - 90.0).abs() < 0.01);
    }

    // -----------------------------------------------------------------------
    // Velocity movement tests (no collision)
    // -----------------------------------------------------------------------

    #[test]
    fn velocity_moves_entity_in_empty_world() {
        let mut app = fixtures::test_app();
        app.add_systems(Update, tile_collision);

        app.world_mut().spawn((
            Transform::from_xyz(100.0, 100.0, 0.0),
            Velocity { x: 500.0, y: 0.0 },
            TileCollider {
                width: 4.0,
                height: 4.0,
            },
        ));

        app.update();
        std::thread::sleep(std::time::Duration::from_millis(50));
        app.update();

        let mut query = app.world_mut().query::<&Transform>();
        let tf = query.iter(app.world()).next().unwrap();
        assert!(
            tf.translation.x > 100.0,
            "entity should move right, got {}",
            tf.translation.x
        );
    }

    #[test]
    fn zero_velocity_entity_stays_in_place() {
        let mut app = fixtures::test_app();
        app.add_systems(Update, tile_collision);

        app.world_mut().spawn((
            Transform::from_xyz(200.0, 200.0, 0.0),
            Velocity { x: 0.0, y: 0.0 },
            TileCollider {
                width: 4.0,
                height: 4.0,
            },
        ));

        app.update();

        let mut query = app.world_mut().query::<&Transform>();
        let tf = query.iter(app.world()).next().unwrap();
        assert_eq!(tf.translation.x, 200.0);
        assert_eq!(tf.translation.y, 200.0);
    }

    // -----------------------------------------------------------------------
    // Collision edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn collision_stops_horizontal_velocity_on_wall() {
        use crate::registry::tile::TileId;
        use crate::world::chunk::Layer;

        let mut app = fixtures::test_app();
        app.add_systems(Update, tile_collision);

        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);

        let surface_y = terrain_gen::surface_height(
            &nc,
            0,
            &wc,
            pc.layers.surface.terrain_frequency,
            pc.layers.surface.terrain_amplitude,
        );

        let ts = wc.tile_size;
        // Work above ground where everything is air
        let air_ty = surface_y + 5;
        let wall_tx = 2; // wall tile at x=2

        let mut world_map = WorldMap::default();
        // Manually place a single solid wall tile
        world_map.set_tile(wall_tx, air_ty, Layer::Fg, TileId(1), &ctx);
        *app.world_mut().resource_mut::<WorldMap>() = world_map;

        // Position entity so its right edge overlaps the wall tile by 2px.
        // Wall tile spans x: [wall_tx*ts .. (wall_tx+1)*ts].
        // Entity width=8, so right edge = center_x + 4.
        // We want right_edge = wall_tx*ts + 2  =>  center_x = wall_tx*ts - 2.
        let entity_x = wall_tx as f32 * ts - 2.0;
        let entity_y = air_ty as f32 * ts + ts / 2.0;

        app.world_mut().spawn((
            Transform::from_xyz(entity_x, entity_y, 0.0),
            Velocity { x: 1000.0, y: 0.0 },
            TileCollider {
                width: 8.0,
                height: 8.0,
            },
            Grounded(false),
        ));

        app.update();

        let mut query = app.world_mut().query::<&Velocity>();
        let vel = query.iter(app.world()).next().unwrap();
        assert_eq!(
            vel.x, 0.0,
            "horizontal velocity should be zeroed on wall collision"
        );
    }

    #[test]
    fn collision_stops_upward_velocity_on_ceiling() {
        use crate::registry::tile::TileId;
        use crate::world::chunk::Layer;

        let mut app = fixtures::test_app();
        app.add_systems(Update, tile_collision);

        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);

        let surface_y = terrain_gen::surface_height(
            &nc,
            0,
            &wc,
            pc.layers.surface.terrain_frequency,
            pc.layers.surface.terrain_amplitude,
        );

        let ts = wc.tile_size;
        // Work above ground where everything is air
        let entity_ty = surface_y + 5;
        let ceiling_ty = entity_ty + 1; // solid tile directly above

        let mut world_map = WorldMap::default();
        // Manually place a single solid ceiling tile
        world_map.set_tile(0, ceiling_ty, Layer::Fg, TileId(1), &ctx);
        *app.world_mut().resource_mut::<WorldMap>() = world_map;

        // Position entity so its top edge overlaps the ceiling tile by 2px.
        // Ceiling tile spans y: [ceiling_ty*ts .. (ceiling_ty+1)*ts].
        // Entity height=8, so top edge = center_y + 4.
        // We want top_edge = ceiling_ty*ts + 2  =>  center_y = ceiling_ty*ts - 2.
        let entity_x = ts / 2.0;
        let entity_y = ceiling_ty as f32 * ts - 2.0;

        app.world_mut().spawn((
            Transform::from_xyz(entity_x, entity_y, 0.0),
            Velocity { x: 0.0, y: 500.0 },
            TileCollider {
                width: 8.0,
                height: 8.0,
            },
            Grounded(false),
        ));

        app.update();

        let mut query = app.world_mut().query::<&Velocity>();
        let vel = query.iter(app.world()).next().unwrap();
        assert_eq!(
            vel.y, 0.0,
            "upward velocity should be zeroed on ceiling collision"
        );
    }

    #[test]
    fn multiple_entities_collide_independently() {
        let mut app = fixtures::test_app();
        app.add_systems(Update, (apply_gravity, tile_collision).chain());

        // Two entities with different gravity
        app.world_mut().spawn((
            Transform::from_xyz(100.0, 100.0, 0.0),
            Velocity { x: 0.0, y: 0.0 },
            Gravity(400.0),
            TileCollider {
                width: 4.0,
                height: 4.0,
            },
        ));
        app.world_mut().spawn((
            Transform::from_xyz(200.0, 200.0, 0.0),
            Velocity { x: 0.0, y: 0.0 },
            Gravity(100.0),
            TileCollider {
                width: 4.0,
                height: 4.0,
            },
        ));

        app.update();
        std::thread::sleep(std::time::Duration::from_millis(50));
        app.update();

        let mut query = app.world_mut().query::<(&Transform, &Gravity)>();
        let entities: Vec<_> = query.iter(app.world()).collect();
        // Entity with stronger gravity should have fallen further
        let heavy = entities.iter().find(|(_, g)| g.0 == 400.0).unwrap().0;
        let light = entities.iter().find(|(_, g)| g.0 == 100.0).unwrap().0;
        assert!(
            heavy.translation.y < light.translation.y,
            "heavier entity ({}) should be lower than lighter ({})",
            heavy.translation.y,
            light.translation.y
        );
    }

    #[test]
    fn gravity_capped_by_max_delta() {
        // Verify MAX_DELTA_SECS prevents huge velocity spikes
        let dt_huge = 1.0_f32;
        let capped = dt_huge.min(MAX_DELTA_SECS);
        assert_eq!(capped, MAX_DELTA_SECS);
        assert!(capped < 0.1, "capped dt should be small");
    }

    #[test]
    fn friction_converges_toward_zero() {
        let mut app = fixtures::test_app();
        app.add_systems(Update, apply_friction);

        app.world_mut()
            .spawn((Velocity { x: 100.0, y: 0.0 }, Grounded(true), Friction(0.5)));

        // Apply multiple frames
        for _ in 0..10 {
            app.update();
        }

        let mut query = app.world_mut().query::<&Velocity>();
        let vel = query.iter(app.world()).next().unwrap();
        assert!(
            vel.x < 1.0,
            "velocity should converge to ~0 after many friction frames, got {}",
            vel.x
        );
    }

    #[test]
    fn friction_does_not_affect_y_velocity() {
        let mut app = fixtures::test_app();
        app.add_systems(Update, apply_friction);

        app.world_mut().spawn((
            Velocity { x: 100.0, y: -50.0 },
            Grounded(true),
            Friction(0.5),
        ));

        app.update();

        let mut query = app.world_mut().query::<&Velocity>();
        let vel = query.iter(app.world()).next().unwrap();
        assert_eq!(vel.y, -50.0, "friction should only affect x, not y");
    }

    // -----------------------------------------------------------------------
    // Bob tests (continued)
    // -----------------------------------------------------------------------

    #[test]
    fn bob_does_not_affect_airborne_entity() {
        let mut app = fixtures::test_app();
        app.add_systems(Update, apply_bob);

        let rest_y = 100.0;
        app.world_mut().spawn((
            Transform::from_xyz(0.0, rest_y, 0.0),
            BobEffect {
                amplitude: 3.0,
                speed: 5.0,
                phase: 0.0,
                rest_y,
            },
            Grounded(false),
        ));

        app.update();
        std::thread::sleep(std::time::Duration::from_millis(50));
        app.update();

        let mut q_bob = app.world_mut().query::<&BobEffect>();
        let bob = q_bob.iter(app.world()).next().unwrap();
        assert_eq!(bob.phase, 0.0, "phase should not advance while airborne");

        let mut q_tf = app.world_mut().query::<&Transform>();
        let tf = q_tf.iter(app.world()).next().unwrap();
        assert_eq!(
            tf.translation.y, rest_y,
            "Y should remain at rest while airborne"
        );
    }
}
