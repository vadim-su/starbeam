use bevy::prelude::*;

use crate::combat::Health;
use crate::enemy::ai::AiStateMachine;
use crate::enemy::components::*;
use crate::enemy::loot::{LootDrop, LootTable};
use crate::physics::{Gravity, TileCollider, Velocity};
use crate::player::Player;

// ---------------------------------------------------------------------------
// Config resource
// ---------------------------------------------------------------------------

#[derive(Resource)]
pub struct MobSpawnConfig {
    pub max_mobs: usize,
    pub spawn_radius_min: f32,
    pub spawn_radius_max: f32,
    pub spawn_interval: f32,
    pub timer: f32,
}

impl Default for MobSpawnConfig {
    fn default() -> Self {
        Self {
            max_mobs: 15,
            spawn_radius_min: 20.0,
            spawn_radius_max: 35.0,
            spawn_interval: 5.0,
            timer: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Main spawn system
// ---------------------------------------------------------------------------

pub fn mob_spawn_system(
    time: Res<Time>,
    mut commands: Commands,
    mut config: ResMut<MobSpawnConfig>,
    enemy_query: Query<(), With<Enemy>>,
    player_query: Query<&Transform, With<Player>>,
) {
    // Count existing enemies; skip if at cap
    let enemy_count = enemy_query.iter().count();
    if enemy_count >= config.max_mobs {
        return;
    }

    // Tick timer
    config.timer += time.delta_secs();
    if config.timer < config.spawn_interval {
        return;
    }
    config.timer = 0.0;

    let Ok(player_tf) = player_query.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();

    // Pick random horizontal offset within spawn radius (in pixels, tile_size assumed 16)
    let tile_size = 16.0_f32;
    let min_px = config.spawn_radius_min * tile_size;
    let max_px = config.spawn_radius_max * tile_size;
    let range = max_px - min_px;
    let offset = min_px + rand::random::<f32>() * range;
    let sign = if rand::random::<bool>() { 1.0 } else { -1.0 };
    let spawn_x = player_pos.x + offset * sign;
    // Use player Y as approximate surface height
    let spawn_y = player_pos.y;
    let spawn_pos = Vec2::new(spawn_x, spawn_y);

    // Pick a random enemy type
    let roll: f32 = rand::random();
    if roll < 0.5 {
        spawn_slime(&mut commands, spawn_pos);
    } else if roll < 0.8 {
        spawn_shooter(&mut commands, spawn_pos);
    } else {
        spawn_flyer(&mut commands, spawn_pos);
    }
}

// ---------------------------------------------------------------------------
// Spawn helpers
// ---------------------------------------------------------------------------

pub fn spawn_slime(commands: &mut Commands, pos: Vec2) {
    commands.spawn((
        Transform::from_xyz(pos.x, pos.y, 0.0),
        Enemy,
        EnemyType::Slime,
        Health::new(30.0),
        Velocity::default(),
        Gravity(600.0),
        TileCollider {
            width: 14.0,
            height: 12.0,
        },
        DetectionRange(160.0),
        AttackRange(24.0),
        ContactDamage(8.0),
        MoveSpeed(40.0),
        PatrolAnchor(pos),
        AttackCooldown {
            duration: 1.0,
            timer: 0.0,
        },
        AiStateMachine::new(pos, 40.0),
        LootTable {
            drops: vec![LootDrop {
                item_id: "gel".into(),
                min: 1,
                max: 3,
                chance: 0.8,
            }],
        },
    ));
}

pub fn spawn_shooter(commands: &mut Commands, pos: Vec2) {
    commands.spawn((
        Transform::from_xyz(pos.x, pos.y, 0.0),
        Enemy,
        EnemyType::Shooter,
        Health::new(20.0),
        Velocity::default(),
        Gravity(600.0),
        TileCollider {
            width: 14.0,
            height: 20.0,
        },
        DetectionRange(240.0),
        AttackRange(200.0),
        ContactDamage(5.0),
        MoveSpeed(30.0),
        PatrolAnchor(pos),
        AttackCooldown {
            duration: 2.0,
            timer: 0.0,
        },
        AiStateMachine::new(pos, 30.0),
        LootTable {
            drops: vec![LootDrop {
                item_id: "lens".into(),
                min: 1,
                max: 1,
                chance: 0.4,
            }],
        },
    ));
}

pub fn spawn_flyer(commands: &mut Commands, pos: Vec2) {
    commands.spawn((
        Transform::from_xyz(pos.x, pos.y, 0.0),
        Enemy,
        EnemyType::Flyer,
        Health::new(15.0),
        Velocity::default(),
        Gravity(0.0), // Flyers ignore gravity
        TileCollider {
            width: 16.0,
            height: 16.0,
        },
        DetectionRange(200.0),
        AttackRange(32.0),
        ContactDamage(10.0),
        MoveSpeed(60.0),
        PatrolAnchor(pos),
        AttackCooldown {
            duration: 0.8,
            timer: 0.0,
        },
        AiStateMachine::new(pos, 60.0),
        LootTable {
            drops: vec![LootDrop {
                item_id: "feather".into(),
                min: 1,
                max: 2,
                chance: 0.6,
            }],
        },
    ));
}
