use bevy::prelude::*;

use crate::math::{tile_aabb, Aabb};
use crate::sets::GameSet;
use crate::world::chunk::WorldMap;
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

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (apply_gravity, tile_collision, apply_friction, apply_bob)
                .chain()
                .in_set(GameSet::Physics),
        );
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Apply gravitational acceleration to all entities with `Velocity` + `Gravity`.
pub fn apply_gravity(time: Res<Time>, mut query: Query<(&mut Velocity, &Gravity)>) {
    let dt = time.delta_secs().min(MAX_DELTA_SECS);
    for (mut vel, gravity) in &mut query {
        vel.y -= gravity.0 * dt;
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
            if world_map.is_solid(tx, ty, &ctx_ref) {
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
            if world_map.is_solid(tx, ty, &ctx_ref) {
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
