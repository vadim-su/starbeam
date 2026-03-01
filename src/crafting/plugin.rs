use bevy::prelude::*;

use super::registry::RecipeRegistry;

/// Crafting plugin — currently a placeholder.
/// TODO: Load recipes from RON data files (assets/recipes/*.ron).
/// TODO: Add crafting systems (craft_item, unlock_recipe, crafting_progress).
/// TODO: Add crafting UI panel integration.
pub struct CraftingPlugin;

impl Plugin for CraftingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(RecipeRegistry::new());
    }
}
