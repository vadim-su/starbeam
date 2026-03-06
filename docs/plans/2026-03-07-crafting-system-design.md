# Crafting System Design

## Overview

Station-based crafting system inspired by Starbound. Crafting stations are separate entities with autonomous progress — player can start a craft and walk away, the station continues working. Recipes unlock via blueprints found in the world.

## Architecture: Separate Station Entities (Variant B)

Crafting stations are standalone ECS entities with a `CraftingStation` component, spawned alongside objects of type `ObjectType::CraftingStation`. Crafting progress lives on the station entity, not the player.

## Data & Components

### ObjectType — new variant

```rust
ObjectType::CraftingStation { station_id: String }
```

RON example:
```ron
object_type: CraftingStation(station_id: "workbench")
```

### CraftingStation component (on station entity)

```rust
#[derive(Component)]
pub struct CraftingStation {
    pub station_id: String,
    pub active_craft: Option<ActiveCraft>,
}

pub struct ActiveCraft {
    pub recipe_id: String,
    pub elapsed: f32,
    pub duration: f32,
    pub result: RecipeResult,
}
```

When an object with `ObjectType::CraftingStation` spawns via the object spawn system, it gets a `CraftingStation` component. On craft completion: result goes to player inventory (if nearby) or spawns as `DroppedItem` near the station.

### Recipes — no changes to existing structures

Existing `Recipe` with `station: Option<String>`, `craft_time: f32`, `unlocked_by: UnlockCondition` covers all needs. Loaded from RON files in `assets/recipes/*.ron`.

### Blueprint unlock system

```rust
#[derive(Component)]
pub struct UnlockedRecipes {
    pub blueprints: HashSet<String>,
}
```

Blueprint = item with `ItemType::Blueprint` and field `unlocks_recipe: String`. Using a blueprint adds the recipe to `UnlockedRecipes`.

### Hand crafting

```rust
#[derive(Component)]  // on player
pub struct HandCraftState {
    pub active_craft: Option<ActiveCraft>,
}
```

## Interaction System (E-key)

Universal "use object" system, not crafting-specific.

### Components

```rust
#[derive(Component)]
pub struct Interactable {
    pub interaction_type: InteractionType,
    pub range: f32,  // in tiles, default 3.0
}

pub enum InteractionType {
    CraftingStation,
    // future: Container, Door, NPC, etc.
}
```

### Logic

1. Each frame: find nearest `Interactable` within range of player → write to `NearbyInteractable(Option<Entity>)` resource
2. If nearby: show "E — Use" prompt
3. On E press + `NearbyInteractable` present → emit `InteractionEvent { entity, interaction_type }`
4. Handler for `InteractionType::CraftingStation` opens crafting UI

Distance check is wrap-aware (same as existing block_action.rs).

### Key rebind

- **E** = interaction (was: toggle inventory)
- **I** = inventory (unchanged)
- **C** = hand crafting UI

## Crafting UI

Opens on station interaction, closes on Escape or E.

### Layout

```
+-------------------------------------+
|  [Workbench]              [X]       |
|-------------------------------------|
|  Recipe list        |  Details      |
|                     |               |
|  > Wooden Sword  v  |  [icon]       |
|    Iron Pickaxe  v  |  Iron Pickaxe |
|    Gold Ring     L  |               |
|                     |  Ingredients: |
|                     |  2x Iron   v  |
|                     |  1x Wood   v  |
|                     |               |
|                     |  [===---] 1.2s|
|                     |  [Craft]      |
|-------------------------------------|
|  Filter: [All] [Available] [Locked] |
+-------------------------------------+
```

### Left panel — recipe list
- All recipes for this `station_id` (+ `station: None` for hand-craftable items)
- Unlocked + enough materials: white with checkmark
- Unlocked + insufficient materials: gray
- Locked (no blueprint): lock icon, name hidden ("???")
- Filters: All / Available / Locked

### Right panel — selected recipe details
- Result icon and name
- Ingredient list with counts (green = have, red = missing)
- Progress bar (visible during active craft)
- "Craft" button (active when ingredients available and station idle)

### Progress bar behavior
- Click Craft → ingredients consumed immediately → progress bar ticks
- Player walks away → UI closes, craft continues on station entity
- Player returns → sees current progress
- On completion: result to inventory (if player nearby) or DroppedItem at station

### Hand crafting (station = None)
- Accessed via C key
- Same UI, but progress tied to player (`HandCraftState`)
- Interrupts if player starts moving (optional for MVP)

## Systems & Data Flow

### Recipe loading

RON files in `assets/recipes/`. Loaded via `RonLoader` during `Loading` phase into `RecipeRegistry`.

```ron
// assets/recipes/workbench.ron
[
    Recipe(
        id: "wooden_sword",
        result: (item_id: "wooden_sword", count: 1),
        ingredients: [(item_id: "wood", count: 10)],
        craft_time: 2.0,
        station: Some("workbench"),
        unlocked_by: Always,
    ),
]
```

### Systems (execution order)

**Input set:**
1. `detect_nearby_interactable` — find nearest Interactable in range → `NearbyInteractable` resource
2. `show_interaction_prompt` — show "E" hint if NearbyInteractable present
3. `handle_interaction_input` — E pressed + NearbyInteractable → `InteractionEvent`
4. `open_crafting_ui` — handle InteractionEvent::CraftingStation, open UI

**WorldUpdate set:**
5. `tick_crafting_stations` — for each CraftingStation with active_craft: elapsed += dt. If done → complete
6. `complete_craft` — result to inventory (player nearby) or DroppedItem

**UI set:**
7. `update_crafting_panel` — sync UI with station state (progress bar, recipe list)
8. `handle_craft_button` — click Craft: verify ingredients → consume → set active_craft

### Key resources

```rust
#[derive(Resource)]
pub struct NearbyInteractable(pub Option<Entity>);

#[derive(Resource)]
pub struct OpenStation(pub Option<Entity>);
```

## MVP Content & Scope

### Stations
1. Hand crafting (station: None)
2. Workbench (station: "workbench")

### Recipes (~8)

**Hand craft (station: None):**
- torch_x4: 1 coal + 1 wood → 4 torch (0.5s, Always)
- workbench: 10 wood → 1 workbench (2.0s, Always)
- wooden_platform_x5: 5 wood → 5 wooden_platform (0.5s, Always)

**Workbench:**
- wooden_sword: 10 wood → 1 wooden_sword (2.0s, Always)
- wooden_pickaxe: 8 wood + 2 stone → 1 wooden_pickaxe (2.0s, Always)
- wooden_door: 6 wood → 1 wooden_door (1.5s, Always)
- chest: 8 wood + 2 iron_ore → 1 chest (3.0s, Always)
- lantern: 3 iron_ore + 1 torch → 1 lantern (2.0s, Blueprint("lantern_blueprint"))

### NOT in MVP
- Blueprint items spawning in world
- Multiple station types (furnace, anvil)
- Station animation during crafting
- Elapsed correction on chunk reload
- Crafting sounds

### In MVP
- Recipe loading from RON
- ObjectType::CraftingStation + CraftingStation component
- E-key interaction system (universal)
- Crafting UI (recipe list + details + progress bar)
- Hand crafting via C key
- HandCraftState on player
- tick_crafting_stations — autonomous station crafting
- Result to inventory or DroppedItem
- UnlockedRecipes component (preparation for blueprints)
- E = interaction, I = inventory (key split)
