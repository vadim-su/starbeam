use bevy::prelude::*;

#[derive(Component, Debug)]
pub struct Enemy;

#[derive(Component, Debug)]
pub struct DetectionRange(pub f32);

#[derive(Component, Debug)]
pub struct AttackRange(pub f32);

#[derive(Component, Debug)]
pub struct AttackCooldown {
    pub duration: f32,
    pub timer: f32,
}

#[derive(Component, Debug)]
pub struct ContactDamage(pub f32);

#[derive(Component, Debug)]
pub struct PatrolAnchor(pub Vec2);

#[derive(Component, Debug)]
pub struct MoveSpeed(pub f32);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyType {
    Slime,
    Shooter,
    Flyer,
}
