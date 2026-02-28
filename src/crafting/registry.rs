use std::collections::HashMap;

use bevy::prelude::*;

use super::recipe::Recipe;

#[derive(Resource, Debug)]
pub struct RecipeRegistry {
    recipes: HashMap<String, Recipe>,
}

impl RecipeRegistry {
    pub fn new() -> Self {
        Self {
            recipes: HashMap::new(),
        }
    }

    pub fn add(&mut self, recipe: Recipe) {
        self.recipes.insert(recipe.id.clone(), recipe);
    }

    pub fn get(&self, id: &str) -> Option<&Recipe> {
        self.recipes.get(id)
    }

    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }

    /// Get all recipes for a specific station (None = hand crafting).
    pub fn for_station(&self, station: Option<&str>) -> Vec<&Recipe> {
        self.recipes
            .values()
            .filter(|r| r.station.as_deref() == station)
            .collect()
    }

    /// Get all recipes that can be crafted with current inventory.
    pub fn craftable_recipes(
        &self,
        station: Option<&str>,
        inventory: &crate::inventory::Inventory,
        unlocked: &std::collections::HashSet<String>,
    ) -> Vec<&Recipe> {
        self.for_station(station)
            .into_iter()
            .filter(|r| {
                r.unlocked_by.is_unlocked(unlocked)
                    && r.ingredients
                        .iter()
                        .all(|ing| inventory.count_item(&ing.item_id) >= ing.count)
            })
            .collect()
    }
}

impl Default for RecipeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crafting::recipe::*;

    fn test_recipe() -> Recipe {
        Recipe {
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
        }
    }

    #[test]
    fn registry_stores_recipes() {
        let mut reg = RecipeRegistry::new();
        reg.add(test_recipe());

        assert_eq!(reg.len(), 1);
        assert!(reg.get("torch").is_some());
    }

    #[test]
    fn registry_filters_by_station() {
        let mut reg = RecipeRegistry::new();

        let mut hand_recipe = test_recipe();
        hand_recipe.id = "hand_item".into();
        hand_recipe.station = None;

        let mut furnace_recipe = test_recipe();
        furnace_recipe.id = "furnace_item".into();
        furnace_recipe.station = Some("furnace".into());

        reg.add(hand_recipe);
        reg.add(furnace_recipe);

        let hand_recipes = reg.for_station(None);
        assert_eq!(hand_recipes.len(), 1);

        let furnace_recipes = reg.for_station(Some("furnace"));
        assert_eq!(furnace_recipes.len(), 1);
    }
}
