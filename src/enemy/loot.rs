use bevy::prelude::*;
use serde::Deserialize;

use super::Enemy;
use crate::combat::Health;

#[derive(Component, Debug, Clone, Deserialize)]
pub struct LootTable {
    pub drops: Vec<LootDrop>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LootDrop {
    pub item_id: String,
    pub min: u16,
    pub max: u16,
    pub chance: f32,
}

pub fn enemy_death_system(
    mut commands: Commands,
    query: Query<(Entity, &Transform, &Health, Option<&LootTable>), With<Enemy>>,
) {
    for (entity, transform, health, loot) in &query {
        if !health.is_dead() {
            continue;
        }
        if let Some(loot_table) = loot {
            let _pos = transform.translation.truncate();
            for drop in &loot_table.drops {
                if rand::random::<f32>() <= drop.chance {
                    let _count = if drop.min == drop.max {
                        drop.min
                    } else {
                        rand::random::<u16>() % (drop.max - drop.min + 1) + drop.min
                    };
                    // TODO: spawn_dropped_item when item drop system is wired up
                }
            }
        }
        commands.entity(entity).despawn();
    }
}
