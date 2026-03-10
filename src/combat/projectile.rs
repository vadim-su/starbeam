use bevy::prelude::*;

use crate::physics::{Gravity, TileCollider, Velocity};

use super::{DamageEvent, Health};

#[derive(Component, Debug)]
pub struct Projectile {
    pub damage: f32,
    pub knockback: f32,
    pub lifetime: f32,
    pub owner: Entity,
}

/// Spawn a projectile entity moving in `direction` at the given `speed`.
pub fn spawn_projectile(
    commands: &mut Commands,
    position: Vec2,
    direction: Vec2,
    speed: f32,
    damage: f32,
    knockback: f32,
    owner: Entity,
) -> Entity {
    let dir = if direction.length_squared() > f32::EPSILON {
        direction.normalize()
    } else {
        Vec2::X
    };
    let vel = dir * speed;

    commands
        .spawn((
            Projectile {
                damage,
                knockback,
                lifetime: 5.0,
                owner,
            },
            Velocity { x: vel.x, y: vel.y },
            Gravity(0.0),
            TileCollider {
                width: 8.0,
                height: 8.0,
            },
            Transform::from_xyz(position.x, position.y, 2.0),
            Visibility::default(),
        ))
        .id()
}

pub fn tick_projectiles(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Projectile)>,
) {
    let dt = time.delta_secs();
    for (entity, mut proj) in &mut query {
        proj.lifetime -= dt;
        if proj.lifetime <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

pub fn projectile_hit_detection(
    mut commands: Commands,
    mut writer: bevy::ecs::message::MessageWriter<DamageEvent>,
    projectile_query: Query<(Entity, &Transform, &Projectile)>,
    target_query: Query<(Entity, &Transform, &TileCollider), With<Health>>,
) {
    for (proj_entity, proj_tf, proj) in &projectile_query {
        let proj_pos = proj_tf.translation.truncate();
        // Simple AABB: treat projectile as a point (or small box) vs target collider
        let proj_half = Vec2::new(4.0, 4.0);

        for (target_entity, target_tf, collider) in &target_query {
            if target_entity == proj.owner {
                continue;
            }

            let target_pos = target_tf.translation.truncate();
            let target_half = Vec2::new(collider.width / 2.0, collider.height / 2.0);

            // AABB overlap test
            let overlap_x = (proj_pos.x - target_pos.x).abs() < (proj_half.x + target_half.x);
            let overlap_y = (proj_pos.y - target_pos.y).abs() < (proj_half.y + target_half.y);

            if overlap_x && overlap_y {
                let diff = target_pos - proj_pos;
                let dir = if diff.length_squared() > f32::EPSILON {
                    diff.normalize()
                } else {
                    Vec2::X
                };

                writer.write(DamageEvent {
                    target: target_entity,
                    amount: proj.damage,
                    knockback: dir * proj.knockback,
                });

                commands.entity(proj_entity).despawn();
                break;
            }
        }
    }
}
