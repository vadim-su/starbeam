use bevy::prelude::*;

use super::registry::RecipeRegistry;

pub struct CraftingPlugin;

impl Plugin for CraftingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(RecipeRegistry::new());
    }
}
