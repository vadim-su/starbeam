use bevy::prelude::*;

use super::recipe::{CraftingStation, HandCraftState};
use crate::inventory::{BagTarget, Inventory};
use crate::item::{ItemRegistry, ItemType};
use crate::player::Player;
use crate::sets::GameSet;

pub struct CraftingPlugin;

impl Plugin for CraftingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (tick_crafting_stations, tick_hand_craft).in_set(GameSet::WorldUpdate),
        );
    }
}

/// Determine which bag an item should go to based on its type.
fn bag_target_for(item_id: &str, item_registry: &ItemRegistry) -> (BagTarget, u16) {
    item_registry
        .by_name(item_id)
        .map(|id| {
            let def = item_registry.get(id);
            let target = match def.item_type {
                ItemType::Block | ItemType::Material => BagTarget::Material,
                _ => BagTarget::Main,
            };
            (target, def.max_stack)
        })
        .unwrap_or((BagTarget::Main, 99))
}

/// Advance crafting progress on all stations with an active craft.
/// When complete, add result to player inventory.
fn tick_crafting_stations(
    time: Res<Time>,
    mut stations: Query<&mut CraftingStation>,
    mut player_query: Query<&mut Inventory, With<Player>>,
    item_registry: Res<ItemRegistry>,
) {
    let dt = time.delta_secs();

    for mut station in &mut stations {
        let Some(ref mut craft) = station.active_craft else {
            continue;
        };

        craft.elapsed += dt;

        if craft.is_complete() {
            let result_id = craft.result.item_id.clone();
            let result_count = craft.result.count;
            station.active_craft = None;

            // Add result to player inventory
            if let Ok(mut inventory) = player_query.single_mut() {
                let (target, max_stack) = bag_target_for(&result_id, &item_registry);
                inventory.try_add_item(&result_id, result_count, max_stack, target);
            }
            // TODO: If player not nearby, spawn DroppedItem at station position
        }
    }
}

/// Advance hand-crafting progress on the player.
fn tick_hand_craft(
    time: Res<Time>,
    mut query: Query<(&mut HandCraftState, &mut Inventory), With<Player>>,
    item_registry: Res<ItemRegistry>,
) {
    let dt = time.delta_secs();

    let Ok((mut hand_craft, mut inventory)) = query.single_mut() else {
        return;
    };

    let Some(ref mut craft) = hand_craft.active_craft else {
        return;
    };

    craft.elapsed += dt;

    if craft.is_complete() {
        let result_id = craft.result.item_id.clone();
        let result_count = craft.result.count;
        hand_craft.active_craft = None;

        let (target, max_stack) = bag_target_for(&result_id, &item_registry);
        inventory.try_add_item(&result_id, result_count, max_stack, target);
    }
}
