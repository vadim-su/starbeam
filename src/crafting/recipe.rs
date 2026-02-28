use std::collections::HashSet;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Recipe {
    pub id: String,
    pub result: RecipeResult,
    pub ingredients: Vec<Ingredient>,
    pub craft_time: f32,
    pub station: Option<String>,
    pub unlocked_by: UnlockCondition,
}

#[derive(Debug, Clone, Deserialize)]
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
}
