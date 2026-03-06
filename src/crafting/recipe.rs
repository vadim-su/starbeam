use std::collections::HashSet;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct Recipe {
    pub id: String,
    pub result: RecipeResult,
    pub ingredients: Vec<Ingredient>,
    pub craft_time: f32,
    pub station: Option<String>,
    pub unlocked_by: UnlockCondition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeResult {
    pub item_id: String,
    pub count: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Ingredient {
    pub item_id: String,
    pub count: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub enum UnlockCondition {
    Always,
    PickupItem(String),
    Blueprint(String),
    Station(String),
}

impl UnlockCondition {
    pub fn is_unlocked(&self, unlocked_items: &HashSet<String>) -> bool {
        match self {
            UnlockCondition::Always => true,
            UnlockCondition::PickupItem(item) => unlocked_items.contains(item),
            UnlockCondition::Blueprint(_) => false,
            UnlockCondition::Station(_) => false,
        }
    }
}

/// Progress state for an active craft on a station or player hand-craft.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveCraft {
    pub recipe_id: String,
    pub elapsed: f32,
    pub duration: f32,
    pub result: RecipeResult,
}

impl ActiveCraft {
    pub fn new(recipe: &Recipe) -> Self {
        Self {
            recipe_id: recipe.id.clone(),
            elapsed: 0.0,
            duration: recipe.craft_time,
            result: recipe.result.clone(),
        }
    }

    pub fn progress(&self) -> f32 {
        if self.duration <= 0.0 {
            1.0
        } else {
            (self.elapsed / self.duration).min(1.0)
        }
    }

    pub fn is_complete(&self) -> bool {
        self.elapsed >= self.duration
    }
}

/// Marker + state for a placed crafting station in the world.
#[derive(Component, Debug)]
pub struct CraftingStation {
    pub station_id: String,
    pub active_craft: Option<ActiveCraft>,
}

/// Hand-crafting state on the player entity.
#[derive(Component, Debug, Default)]
pub struct HandCraftState {
    pub active_craft: Option<ActiveCraft>,
}

/// Tracks which recipes the player has unlocked via blueprints.
#[derive(Component, Debug, Default)]
pub struct UnlockedRecipes {
    pub blueprints: std::collections::HashSet<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_has_result_and_ingredients() {
        let recipe = Recipe {
            id: "torch".into(),
            result: RecipeResult {
                item_id: "torch".into(),
                count: 4,
            },
            ingredients: vec![
                Ingredient {
                    item_id: "coal".into(),
                    count: 1,
                },
                Ingredient {
                    item_id: "wood".into(),
                    count: 1,
                },
            ],
            craft_time: 0.5,
            station: None,
            unlocked_by: UnlockCondition::Always,
        };

        assert_eq!(recipe.id, "torch");
        assert_eq!(recipe.result.count, 4);
        assert_eq!(recipe.ingredients.len(), 2);
    }

    #[test]
    fn recipe_can_check_if_unlocked() {
        let always = UnlockCondition::Always;
        assert!(always.is_unlocked(&HashSet::new()));

        let pickup = UnlockCondition::PickupItem("stone".into());
        let mut unlocked = HashSet::new();
        assert!(!pickup.is_unlocked(&unlocked));

        unlocked.insert("stone".into());
        assert!(pickup.is_unlocked(&unlocked));
    }

    #[test]
    fn active_craft_progress() {
        let recipe = Recipe {
            id: "torch".into(),
            result: RecipeResult {
                item_id: "torch".into(),
                count: 4,
            },
            ingredients: vec![],
            craft_time: 2.0,
            station: None,
            unlocked_by: UnlockCondition::Always,
        };
        let mut craft = ActiveCraft::new(&recipe);
        assert!((craft.progress() - 0.0).abs() < f32::EPSILON);
        assert!(!craft.is_complete());
        craft.elapsed = 1.0;
        assert!((craft.progress() - 0.5).abs() < f32::EPSILON);
        craft.elapsed = 2.0;
        assert!(craft.is_complete());
        assert!((craft.progress() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn active_craft_instant() {
        let recipe = Recipe {
            id: "instant".into(),
            result: RecipeResult {
                item_id: "item".into(),
                count: 1,
            },
            ingredients: vec![],
            craft_time: 0.0,
            station: None,
            unlocked_by: UnlockCondition::Always,
        };
        let craft = ActiveCraft::new(&recipe);
        assert!(craft.is_complete());
        assert!((craft.progress() - 1.0).abs() < f32::EPSILON);
    }
}
