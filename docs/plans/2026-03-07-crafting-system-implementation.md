# Crafting System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement station-based crafting with autonomous station entities, E-key interaction, progress bar UI, and hand crafting.

**Architecture:** Crafting stations are ECS entities with `CraftingStation` component spawned alongside `PlacedObjectEntity`. A universal interaction system (E-key) detects nearby interactables. Crafting progress ticks on the station entity independently of the player. UI shows recipe list + details + progress bar.

**Tech Stack:** Bevy 0.18, RON data files, existing `RonLoader<T>`, Bevy UI nodes (no egui).

---

### Task 1: Add CraftingStation variant to ObjectType

**Files:**
- Modify: `src/object/definition.rs:23-28` (ObjectType enum)

**Step 1: Add CraftingStation variant**

In `src/object/definition.rs`, add to the `ObjectType` enum:

```rust
#[derive(Debug, Clone, Deserialize)]
pub enum ObjectType {
    Decoration,
    Container { slots: u16 },
    LightSource,
    CraftingStation { station_id: String },
}
```

**Step 2: Verify it compiles**

Run: `cargo check`
Expected: PASS (no code uses exhaustive match on ObjectType currently)

**Step 3: Commit**

```
git add src/object/definition.rs
git commit -m "feat(crafting): add CraftingStation variant to ObjectType"
```

---

### Task 2: Create crafting station component and ActiveCraft

**Files:**
- Modify: `src/crafting/recipe.rs` (add Serialize derives, ActiveCraft struct)
- Modify: `src/crafting/mod.rs` (re-export)

**Step 1: Add ActiveCraft and CraftingStation component to recipe.rs**

Add to `src/crafting/recipe.rs` after existing imports:

```rust
use bevy::prelude::*;
use serde::Serialize;
```

Update Recipe-related derives to also derive `Serialize` where needed. Then add at the end of the file (before tests):

```rust
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

    /// Returns progress as 0.0..=1.0.
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
```

Also add `Serialize` derive to `RecipeResult`:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RecipeResult {
    pub item_id: String,
    pub count: u16,
}
```

**Step 2: Update mod.rs re-exports**

In `src/crafting/mod.rs`, the existing `pub use recipe::*;` already covers everything.

**Step 3: Write tests for ActiveCraft**

Add to the existing `#[cfg(test)] mod tests` in `src/crafting/recipe.rs`:

```rust
#[test]
fn active_craft_progress() {
    let recipe = Recipe {
        id: "torch".into(),
        result: RecipeResult { item_id: "torch".into(), count: 4 },
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
        result: RecipeResult { item_id: "item".into(), count: 1 },
        ingredients: vec![],
        craft_time: 0.0,
        station: None,
        unlocked_by: UnlockCondition::Always,
    };
    let craft = ActiveCraft::new(&recipe);
    assert!(craft.is_complete());
    assert!((craft.progress() - 1.0).abs() < f32::EPSILON);
}
```

**Step 4: Run tests**

Run: `cargo test --lib crafting::recipe`
Expected: ALL PASS

**Step 5: Commit**

```
git add src/crafting/recipe.rs src/crafting/mod.rs
git commit -m "feat(crafting): add ActiveCraft, CraftingStation, HandCraftState, UnlockedRecipes"
```

---

### Task 3: Spawn CraftingStation component on object entities

**Files:**
- Modify: `src/object/spawn.rs:68-84` (spawn_objects_for_chunk)

**Step 1: Add CraftingStation component insertion**

In `src/object/spawn.rs`, after the entity is spawned (line ~84), add logic to insert `CraftingStation` if the object type is `CraftingStation`:

Add import at top:
```rust
use crate::crafting::CraftingStation;
use super::definition::ObjectType;
```

After `Visibility::default(),` in the spawn block (around line 84), before the closing `));`, add a conditional insert after the entity_cmd is created:

```rust
// After entity_cmd is spawned (after line 84):
if let ObjectType::CraftingStation { ref station_id } = def.object_type {
    entity_cmd.insert(CraftingStation {
        station_id: station_id.clone(),
        active_craft: None,
    });
}
```

**Step 2: Verify it compiles**

Run: `cargo check`
Expected: PASS

**Step 3: Commit**

```
git add src/object/spawn.rs
git commit -m "feat(crafting): spawn CraftingStation component on crafting station objects"
```

---

### Task 4: Add HandCraftState and UnlockedRecipes to player spawn

**Files:**
- Modify: `src/player/mod.rs:96-127` (spawn_player)

**Step 1: Add components to player spawn**

In `src/player/mod.rs`, add import:
```rust
use crate::crafting::{HandCraftState, UnlockedRecipes};
```

In the `commands.spawn((...))` block in `spawn_player` (around line 96), add after `AnimationState { ... },`:

```rust
HandCraftState::default(),
UnlockedRecipes::default(),
```

**Step 2: Verify it compiles**

Run: `cargo check`
Expected: PASS

**Step 3: Commit**

```
git add src/player/mod.rs
git commit -m "feat(crafting): add HandCraftState and UnlockedRecipes to player"
```

---

### Task 5: Recipe loading from RON files

**Files:**
- Create: `src/registry/assets.rs` (add RecipeListAsset)
- Modify: `src/registry/mod.rs` (register RonLoader for recipes)
- Modify: `src/registry/loading.rs` (load recipes, build RecipeRegistry)
- Create: `assets/recipes/base.recipes.ron` (hand-craft recipes)
- Create: `assets/recipes/workbench.recipes.ron` (workbench recipes)

**Step 1: Add RecipeListAsset to registry/assets.rs**

In `src/registry/assets.rs`, add:

```rust
use crate::crafting::Recipe;

/// Asset loaded from *.recipes.ron — a list of crafting recipes.
#[derive(Asset, TypePath, Debug, Deserialize)]
#[serde(transparent)]
pub struct RecipeListAsset(pub Vec<Recipe>);
```

**Step 2: Register the RonLoader in registry/mod.rs**

Find where other RonLoaders are registered (in `RegistryPlugin::build`). Add:

```rust
app.init_asset::<RecipeListAsset>()
   .register_asset_loader(RonLoader::<RecipeListAsset>::new(&["recipes.ron"]));
```

**Step 3: Add recipe loading to start_loading**

In `src/registry/loading.rs`, add to `LoadingAssets`:

```rust
recipes: Vec<(String, Handle<RecipeListAsset>)>,
```

In `start_loading`, add before `commands.insert_resource(LoadingAssets { ... })`:

```rust
let recipes = vec![
    (
        "base".to_string(),
        asset_server.load::<RecipeListAsset>("recipes/base.recipes.ron"),
    ),
    (
        "workbench".to_string(),
        asset_server.load::<RecipeListAsset>("recipes/workbench.recipes.ron"),
    ),
];
```

Add `recipes` to the `LoadingAssets` struct initialization.

**Step 4: Process recipes in check_loading**

In `check_loading`, add after item loading check:

```rust
// Wait for all recipe assets
let recipe_assets = /* add Res<Assets<RecipeListAsset>> to fn params */;
let all_recipes_loaded = loading.recipes.iter().all(|(_, h)| recipe_assets.contains(h));
if !all_recipes_loaded {
    return;
}
```

After building ItemRegistry, build RecipeRegistry:

```rust
// Build RecipeRegistry from loaded recipe files
let mut recipe_registry = crate::crafting::RecipeRegistry::new();
for (_name, handle) in &loading.recipes {
    if let Some(asset) = recipe_assets.get(handle) {
        for recipe in &asset.0 {
            recipe_registry.add(recipe.clone());
        }
    }
}
info!("Recipe registry loaded: {} recipes", recipe_registry.len());
commands.insert_resource(recipe_registry);
```

Remove the empty `RecipeRegistry::new()` insertion from `CraftingPlugin::build` in `src/crafting/plugin.rs` (it will now be inserted by loading pipeline).

**Step 5: Create RON recipe files**

Create `assets/recipes/base.recipes.ron`:
```ron
[
    (
        id: "torch_x4",
        result: (item_id: "torch", count: 4),
        ingredients: [(item_id: "dirt", count: 2)],
        craft_time: 0.5,
        station: None,
        unlocked_by: Always,
    ),
]
```

Note: Using `dirt` as ingredient since that's what's available in the current ItemRegistry. Real recipes will use proper materials once more items exist.

Create `assets/recipes/workbench.recipes.ron`:
```ron
[
    (
        id: "stone_block_x4",
        result: (item_id: "stone", count: 4),
        ingredients: [(item_id: "dirt", count: 8)],
        craft_time: 2.0,
        station: Some("workbench"),
        unlocked_by: Always,
    ),
]
```

**Step 6: Verify it compiles and loads**

Run: `cargo check`
Expected: PASS

**Step 7: Commit**

```
git add src/registry/assets.rs src/registry/mod.rs src/registry/loading.rs
git add src/crafting/plugin.rs assets/recipes/
git commit -m "feat(crafting): load recipes from RON files into RecipeRegistry"
```

---

### Task 6: Interaction system — detect nearby interactables

**Files:**
- Create: `src/interaction/interactable.rs`
- Modify: `src/interaction/mod.rs`

**Step 1: Create interactable.rs**

Create `src/interaction/interactable.rs`:

```rust
use bevy::prelude::*;

use crate::crafting::CraftingStation;
use crate::object::spawn::PlacedObjectEntity;
use crate::player::Player;
use crate::registry::world::ActiveWorld;

/// What kind of interaction this entity supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionType {
    CraftingStation,
}

/// Resource: the nearest interactable entity within range, if any.
#[derive(Resource, Default)]
pub struct NearbyInteractable {
    pub entity: Option<Entity>,
    pub interaction_type: Option<InteractionType>,
}

/// Resource: which crafting station UI is currently open.
#[derive(Resource, Default)]
pub struct OpenStation(pub Option<Entity>);

const INTERACTION_RANGE: f32 = 3.0; // tiles

/// Each frame, find the nearest CraftingStation within range of the player.
pub fn detect_nearby_interactable(
    mut nearby: ResMut<NearbyInteractable>,
    player_query: Query<&Transform, With<Player>>,
    station_query: Query<(Entity, &Transform), With<CraftingStation>>,
    world_config: Res<ActiveWorld>,
) {
    let Ok(player_tf) = player_query.single() else {
        nearby.entity = None;
        nearby.interaction_type = None;
        return;
    };

    let tile_size = world_config.tile_size;
    let world_width = world_config.width_tiles as f32 * tile_size;
    let range_px = INTERACTION_RANGE * tile_size;

    let mut closest: Option<(Entity, f32)> = None;

    for (entity, station_tf) in &station_query {
        let dx = (player_tf.translation.x - station_tf.translation.x).abs();
        let dx = dx.min(world_width - dx); // wrap-aware
        let dy = (player_tf.translation.y - station_tf.translation.y).abs();
        let dist = (dx * dx + dy * dy).sqrt();

        if dist <= range_px {
            if closest.is_none() || dist < closest.unwrap().1 {
                closest = Some((entity, dist));
            }
        }
    }

    if let Some((entity, _)) = closest {
        nearby.entity = Some(entity);
        nearby.interaction_type = Some(InteractionType::CraftingStation);
    } else {
        nearby.entity = None;
        nearby.interaction_type = None;
    }
}

/// Handle E key press: toggle interaction with nearby entity or close open station.
pub fn handle_interaction_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    nearby: Res<NearbyInteractable>,
    mut open_station: ResMut<OpenStation>,
) {
    if !keyboard.just_pressed(KeyCode::KeyE) {
        return;
    }

    // If a station is open, close it
    if open_station.0.is_some() {
        open_station.0 = None;
        return;
    }

    // If near a station, open it
    if let Some(entity) = nearby.entity {
        if nearby.interaction_type == Some(InteractionType::CraftingStation) {
            open_station.0 = Some(entity);
        }
    }
}
```

**Step 2: Update interaction/mod.rs**

```rust
pub mod block_action;
pub mod interactable;

use bevy::prelude::*;

use crate::registry::AppState;
use crate::sets::GameSet;
use interactable::{NearbyInteractable, OpenStation};

pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NearbyInteractable>()
            .init_resource::<OpenStation>()
            .add_systems(
                Update,
                (
                    block_action::block_interaction_system,
                    interactable::detect_nearby_interactable,
                    interactable::handle_interaction_input,
                )
                    .chain()
                    .in_set(GameSet::Input),
            );
    }
}
```

**Step 3: Remove E key from inventory toggle**

In `src/ui/game_ui/mod.rs`, function `toggle_inventory` (line 123-139), remove `KeyCode::KeyE`:

Change:
```rust
if keyboard.just_pressed(KeyCode::KeyE) || keyboard.just_pressed(KeyCode::KeyI) {
```
To:
```rust
if keyboard.just_pressed(KeyCode::KeyI) {
```

**Step 4: Verify it compiles**

Run: `cargo check`
Expected: PASS

**Step 5: Commit**

```
git add src/interaction/interactable.rs src/interaction/mod.rs src/ui/game_ui/mod.rs
git commit -m "feat(crafting): add interaction system with E-key and nearby detection"
```

---

### Task 7: Crafting tick system

**Files:**
- Modify: `src/crafting/plugin.rs`

**Step 1: Implement tick and completion systems**

Replace `src/crafting/plugin.rs` entirely:

```rust
use bevy::prelude::*;

use super::recipe::{ActiveCraft, CraftingStation, HandCraftState};
use super::registry::RecipeRegistry;
use crate::interaction::interactable::OpenStation;
use crate::inventory::{BagTarget, Inventory};
use crate::item::ItemRegistry;
use crate::player::Player;
use crate::sets::GameSet;

pub struct CraftingPlugin;

impl Plugin for CraftingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                tick_crafting_stations,
                tick_hand_craft,
            )
                .in_set(GameSet::WorldUpdate),
        );
    }
}

/// Advance crafting progress on all stations with an active craft.
fn tick_crafting_stations(
    time: Res<Time>,
    mut stations: Query<&mut CraftingStation>,
    mut player_query: Query<&mut Inventory, With<Player>>,
    player_tf_query: Query<&Transform, With<Player>>,
    station_tf_query: Query<&Transform, With<CraftingStation>>,
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

            // Try to add result to player inventory
            if let Ok(mut inventory) = player_query.single_mut() {
                let max_stack = item_registry
                    .by_name(&result_id)
                    .map(|id| item_registry.get(id).max_stack)
                    .unwrap_or(99);
                let target = item_registry
                    .by_name(&result_id)
                    .map(|id| {
                        let def = item_registry.get(id);
                        match def.item_type {
                            crate::item::ItemType::Block | crate::item::ItemType::Material => {
                                BagTarget::Material
                            }
                            _ => BagTarget::Main,
                        }
                    })
                    .unwrap_or(BagTarget::Main);
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

        let max_stack = item_registry
            .by_name(&result_id)
            .map(|id| item_registry.get(id).max_stack)
            .unwrap_or(99);
        let target = item_registry
            .by_name(&result_id)
            .map(|id| {
                let def = item_registry.get(id);
                match def.item_type {
                    crate::item::ItemType::Block | crate::item::ItemType::Material => {
                        BagTarget::Material
                    }
                    _ => BagTarget::Main,
                }
            })
            .unwrap_or(BagTarget::Main);
        inventory.try_add_item(&result_id, result_count, max_stack, target);
    }
}
```

**Step 2: Verify it compiles**

Run: `cargo check`
Expected: PASS

**Step 3: Commit**

```
git add src/crafting/plugin.rs
git commit -m "feat(crafting): add crafting tick systems for stations and hand-craft"
```

---

### Task 8: Crafting UI panel

**Files:**
- Create: `src/ui/crafting_panel.rs`
- Modify: `src/ui/mod.rs`

This is the largest task. The crafting panel shows:
- Left: recipe list for the current station
- Right: selected recipe details + ingredients + progress bar + craft button

**Step 1: Create crafting_panel.rs**

Create `src/ui/crafting_panel.rs` with the crafting UI. This will include:

- `CraftingPanel` marker component
- `CraftingRecipeList` marker
- `CraftingDetailPanel` marker  
- `CraftingProgressBar` marker
- `CraftButton` marker
- `SelectedRecipe` resource
- `spawn_crafting_panel` system (OnEnter or on OpenStation change)
- `despawn_crafting_panel` system (on close)
- `update_recipe_list` system
- `update_recipe_details` system
- `update_progress_bar` system
- `handle_craft_click` system
- `handle_recipe_select` system

The UI uses Bevy UI nodes (same style as inventory panel), not egui.

Key resources to track UI state:

```rust
#[derive(Resource, Default)]
pub struct CraftingUiState {
    pub selected_recipe_id: Option<String>,
    pub filter: RecipeFilter,
}

#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub enum RecipeFilter {
    #[default]
    All,
    Available,
    Locked,
}
```

The implementation should:
1. Watch `OpenStation` resource — when it changes to `Some(entity)`, spawn the panel
2. When it changes to `None`, despawn the panel
3. Recipe list reads from `RecipeRegistry::for_station(station_id)`
4. Clicking a recipe sets `selected_recipe_id`
5. Detail panel shows ingredients with green/red based on `Inventory::count_item`
6. Craft button calls `start_craft` which checks ingredients, consumes them, sets `active_craft` on the station
7. Progress bar reads `station.active_craft.progress()`

Due to the size of this task, implement it incrementally:
- First: spawn/despawn panel with station name header
- Second: recipe list (text only)
- Third: recipe selection + detail panel
- Fourth: craft button + ingredient consumption
- Fifth: progress bar

Each sub-step should compile and be committed separately.

**Step 2: Register in ui/mod.rs**

Add `pub mod crafting_panel;` and register systems in `UiPlugin`.

**Step 3: Verify and commit after each sub-step**

Run: `cargo check` after each sub-step
Commit after each working increment.

---

### Task 9: Hand-craft UI (C key)

**Files:**
- Modify: `src/ui/crafting_panel.rs` (reuse panel for hand-crafting)
- Modify: `src/interaction/interactable.rs` (handle C key)

**Step 1: Add C key handler**

In `handle_interaction_input`, add C key support that opens crafting panel with `station = None`:

```rust
// Add a separate resource for hand-craft UI
#[derive(Resource, Default)]
pub struct HandCraftOpen(pub bool);
```

When C is pressed, toggle `HandCraftOpen`. The crafting panel checks both `OpenStation` and `HandCraftOpen` to determine what to show.

**Step 2: Reuse crafting panel for hand-craft**

The same UI panel works for both modes:
- Station mode: reads `CraftingStation.station_id`, shows station recipes
- Hand mode: uses `station = None`, shows hand-craftable recipes, progress on `HandCraftState`

**Step 3: Verify and commit**

```
git add src/ui/crafting_panel.rs src/interaction/interactable.rs
git commit -m "feat(crafting): add hand-craft UI via C key"
```

---

### Task 10: Create workbench object asset

**Files:**
- Create: `assets/content/objects/workbench/workbench.object.ron`
- Create: `assets/content/objects/workbench/workbench.item.ron`
- Create or use placeholder: `assets/content/objects/workbench/workbench.png`
- Create or use placeholder: `assets/content/objects/workbench/item.png`
- Modify: `src/registry/loading.rs` (register workbench object and item)

**Step 1: Create workbench.object.ron**

```ron
(
  id: "workbench_object",
  display_name: "Workbench",
  size: (2, 1),
  sprite: "workbench.png",
  solid_mask: [true, true],
  placement: Floor,
  light_emission: (0, 0, 0),
  object_type: CraftingStation(station_id: "workbench"),
  drops: [( item_id: "workbench", min: 1, max: 1, chance: 1.0 )],
  sprite_columns: 1,
  sprite_rows: 1,
  sprite_fps: 0.0,
  flicker_speed: 0.0,
  flicker_strength: 0.0,
  flicker_min: 1.0,
)
```

**Step 2: Create workbench.item.ron**

```ron
(
  id: "workbench",
  display_name: "Workbench",
  description: "A basic crafting station",
  max_stack: 10,
  rarity: Common,
  item_type: Block,
  icon: "item.png",
  placeable_object: Some("workbench_object"),
)
```

**Step 3: Create placeholder sprites**

Create a 64x32 placeholder PNG for `workbench.png` (2x1 tiles at 32px) and a 16x16 `item.png`. These can be simple colored rectangles for now.

**Step 4: Register in loading pipeline**

In `src/registry/loading.rs`, add to `start_loading`'s objects list:

```rust
(
    "content/objects/workbench/".to_string(),
    asset_server.load::<ObjectDefAsset>("content/objects/workbench/workbench.object.ron"),
),
```

And to items list:

```rust
(
    "content/objects/workbench/".to_string(),
    asset_server.load::<ItemDefAsset>("content/objects/workbench/workbench.item.ron"),
),
```

**Step 5: Give player a workbench on spawn (for testing)**

Temporarily in `src/player/mod.rs` spawn_player, add:

```rust
inv.try_add_item("workbench", 1, 10, crate::inventory::BagTarget::Main);
```

**Step 6: Verify and commit**

Run: `cargo check`

```
git add assets/content/objects/workbench/ assets/recipes/ src/registry/loading.rs src/player/mod.rs
git commit -m "feat(crafting): add workbench object, item, and test recipes"
```

---

### Task 11: Integration testing

**Step 1: Manual testing checklist**

Run `cargo run` and verify:
1. Player spawns with workbench in inventory
2. Can place workbench on ground (left-click)
3. Walking near workbench shows interaction prompt possibility (E key works)
4. Pressing E opens crafting UI with workbench recipes
5. Can select a recipe and see ingredients
6. Can click Craft, ingredients are consumed, progress bar fills
7. On completion, result appears in inventory
8. Pressing E again or Escape closes the panel
9. C key opens hand-craft panel with base recipes
10. Breaking workbench drops it back as item

**Step 2: Fix any issues found**

**Step 3: Final commit**

```
git add -A
git commit -m "feat(crafting): complete MVP crafting system"
```
