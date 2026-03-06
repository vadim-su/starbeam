use bevy::prelude::*;

/// Crafting plugin — registers crafting systems.
/// RecipeRegistry is loaded from RON files via the asset loading pipeline
/// (see src/registry/loading.rs).
pub struct CraftingPlugin;

impl Plugin for CraftingPlugin {
    fn build(&self, _app: &mut App) {
        // RecipeRegistry is inserted by the loading pipeline.
        // Crafting tick systems will be added here in a future task.
    }
}
