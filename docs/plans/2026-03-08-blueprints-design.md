# Blueprint System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add consumable blueprint items that unlock crafting recipes when used.

**Architecture:** Extend existing item/crafting systems. Add `Blueprint` variant to `ItemType`, add `blueprint_recipe` field to `ItemDef`, fix `UnlockCondition::Blueprint` to check `UnlockedRecipes.blueprints`, add `use_item_system` for consuming blueprints. Two test blueprints added to starting inventory.

**Tech Stack:** Bevy 0.18, RON assets, existing crafting/item/inventory systems.

---

### Task 1: Fix UnlockCondition::Blueprint to check UnlockedRecipes

**Files:**
- Modify: `src/crafting/recipe.rs:41` (UnlockCondition::is_unlocked)

**Step 1: Update the existing test to cover Blueprint unlock**

In `src/crafting/recipe.rs`, add to the existing `recipe_can_check_if_unlocked` test:

```rust
let blueprint = UnlockCondition::Blueprint("wooden_sword".into());
let mut bp_unlocked = HashSet::new();
assert!(!blueprint.is_unlocked(&bp_unlocked));

bp_unlocked.insert("wooden_sword".into());
assert!(blueprint.is_unlocked(&bp_unlocked));
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p starbeam --lib crafting::recipe::tests::recipe_can_check_if_unlocked`
Expected: FAIL — Blueprint always returns false.

**Step 3: Fix Blueprint variant in is_unlocked**

In `src/crafting/recipe.rs`, change line 41:

```rust
// Before:
UnlockCondition::Blueprint(_) => false,

// After:
UnlockCondition::Blueprint(id) => unlocked_items.contains(id),
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p starbeam --lib crafting::recipe::tests::recipe_can_check_if_unlocked`
Expected: PASS

**Step 5: Commit**

```bash
git add src/crafting/recipe.rs
git commit -m "feat(crafting): fix Blueprint unlock condition to check UnlockedRecipes"
```

---

### Task 2: Add Blueprint ItemType and blueprint_recipe field to ItemDef

**Files:**
- Modify: `src/item/definition.rs` (ItemType enum, ItemDef struct)

**Step 1: Add Blueprint variant to ItemType enum**

In `src/item/definition.rs`, add `Blueprint` to `ItemType`:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize)]
pub enum ItemType {
    #[default]
    Block,
    Resource,
    Tool,
    Weapon,
    Armor,
    Consumable,
    Material,
    Blueprint,
}
```

**Step 2: Add blueprint_recipe field to ItemDef**

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ItemDef {
    pub id: String,
    pub display_name: String,
    pub description: String,
    #[serde(default = "default_max_stack")]
    pub max_stack: u16,
    #[serde(default)]
    pub rarity: Rarity,
    #[serde(default)]
    pub item_type: ItemType,
    pub icon: Option<String>,
    pub placeable: Option<String>,
    #[serde(default)]
    pub placeable_object: Option<String>,
    pub equipment_slot: Option<EquipmentSlot>,
    pub stats: Option<ItemStats>,
    /// If set, using this item unlocks the named recipe in UnlockedRecipes.
    #[serde(default)]
    pub blueprint_recipe: Option<String>,
}
```

**Step 3: Update existing test to include new field**

In the `item_def_has_required_fields` test, add `blueprint_recipe: None` to the ItemDef constructor.

**Step 4: Run tests**

Run: `cargo test -p starbeam --lib item::definition`
Expected: PASS

**Step 5: Commit**

```bash
git add src/item/definition.rs
git commit -m "feat(item): add Blueprint item type and blueprint_recipe field"
```

---

### Task 3: Create test blueprint items and recipes

**Files:**
- Create: `assets/content/items/blueprint_wooden_sword/blueprint_wooden_sword.item.ron`
- Create: `assets/content/items/blueprint_stone_pickaxe/blueprint_stone_pickaxe.item.ron`
- Modify: `assets/recipes/base.recipes.ron` (add 2 blueprint-locked recipes)

**Step 1: Create blueprint_wooden_sword item**

File: `assets/content/items/blueprint_wooden_sword/blueprint_wooden_sword.item.ron`

```ron
(
  id: "blueprint_wooden_sword",
  display_name: "Blueprint: Wooden Sword",
  description: "Learn to craft a wooden sword.",
  max_stack: 1,
  rarity: Uncommon,
  item_type: Blueprint,
  blueprint_recipe: Some("wooden_sword"),
)
```

**Step 2: Create blueprint_stone_pickaxe item**

File: `assets/content/items/blueprint_stone_pickaxe/blueprint_stone_pickaxe.item.ron`

```ron
(
  id: "blueprint_stone_pickaxe",
  display_name: "Blueprint: Stone Pickaxe",
  description: "Learn to craft a stone pickaxe.",
  max_stack: 1,
  rarity: Uncommon,
  item_type: Blueprint,
  blueprint_recipe: Some("stone_pickaxe"),
)
```

**Step 3: Add blueprint-locked recipes to base.recipes.ron**

Add to `assets/recipes/base.recipes.ron`:

```ron
    (
        id: "wooden_sword",
        result: (item_id: "wood", count: 1),
        ingredients: [(item_id: "dirt", count: 5)],
        craft_time: 1.0,
        station: None,
        unlocked_by: Blueprint("wooden_sword"),
    ),
    (
        id: "stone_pickaxe",
        result: (item_id: "stone", count: 1),
        ingredients: [(item_id: "dirt", count: 10)],
        craft_time: 1.5,
        station: None,
        unlocked_by: Blueprint("stone_pickaxe"),
    ),
```

Note: recipes produce wood/stone as placeholders since we don't have sword/pickaxe items yet. The point is to test the blueprint unlock flow.

**Step 4: Verify RON syntax**

Run: `cargo build -p starbeam`
Expected: Compiles (RON loaded at runtime, not compile-time, but build confirms no code issues)

**Step 5: Commit**

```bash
git add assets/content/items/blueprint_wooden_sword/ assets/content/items/blueprint_stone_pickaxe/ assets/recipes/base.recipes.ron
git commit -m "feat(content): add test blueprint items and blueprint-locked recipes"
```

---

### Task 4: Add use_item_system for consuming blueprints

**Files:**
- Create: `src/interaction/use_item.rs`
- Modify: `src/interaction/mod.rs` (register new system)

**Step 1: Create use_item.rs**

File: `src/interaction/use_item.rs`

```rust
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::crafting::UnlockedRecipes;
use crate::inventory::{Hotbar, Inventory};
use crate::item::ItemRegistry;
use crate::item::definition::ItemType;
use crate::player::Player;

/// Consumes blueprint items from the active hotbar slot on left-click (on air/no block action).
pub fn use_item_system(
    mouse: Res<ButtonInput<MouseButton>>,
    mut player_query: Query<(&Hotbar, &mut Inventory, &mut UnlockedRecipes), With<Player>>,
    item_registry: Res<ItemRegistry>,
) {
    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }

    let Ok((hotbar, mut inventory, mut unlocked)) = player_query.single_mut() else {
        return;
    };

    // Check left hand of active slot
    let Some(item_id) = hotbar.slots[hotbar.active_slot].left_hand.as_deref() else {
        return;
    };

    if inventory.count_item(item_id) == 0 {
        return;
    }

    let Some(def_id) = item_registry.by_name(item_id) else {
        return;
    };
    let def = item_registry.get(def_id);

    if def.item_type != ItemType::Blueprint {
        return;
    }

    let Some(ref recipe_id) = def.blueprint_recipe else {
        return;
    };

    // Unlock the recipe (even if already unlocked)
    unlocked.blueprints.insert(recipe_id.clone());
    inventory.remove_item(item_id, 1);

    info!("Blueprint used: unlocked recipe '{}'", recipe_id);
}
```

Note: Uses right-click to avoid conflict with block_interaction_system's left-click. The system checks `ItemType::Blueprint` so it only fires for blueprints.

**Step 2: Register system in InteractionPlugin**

In `src/interaction/mod.rs`, add:

```rust
pub mod use_item;
```

And add the system to the plugin:

```rust
impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NearbyInteractable>()
            .init_resource::<OpenStation>()
            .init_resource::<HandCraftOpen>()
            .add_systems(
                Update,
                (
                    block_action::block_interaction_system,
                    use_item::use_item_system,
                    interactable::detect_nearby_interactable,
                    interactable::handle_interaction_input,
                    interactable::update_interactable_highlight,
                )
                    .in_set(GameSet::Input),
            );
    }
}
```

**Step 3: Build and verify**

Run: `cargo build -p starbeam`
Expected: Compiles without errors.

**Step 4: Commit**

```bash
git add src/interaction/use_item.rs src/interaction/mod.rs
git commit -m "feat(interaction): add use_item_system for consuming blueprints"
```

---

### Task 5: Add blueprints to starting inventory

**Files:**
- Modify: `src/player/mod.rs:108-113` (spawn_player inventory setup)

**Step 1: Add blueprints to starting inventory**

In `src/player/mod.rs`, update the inventory block in `spawn_player`:

```rust
{
    let mut inv = Inventory::new();
    inv.try_add_item("torch", 10, 999, crate::inventory::BagTarget::Main);
    inv.try_add_item("workbench", 1, 10, crate::inventory::BagTarget::Main);
    inv.try_add_item("blueprint_wooden_sword", 1, 1, crate::inventory::BagTarget::Main);
    inv.try_add_item("blueprint_stone_pickaxe", 1, 1, crate::inventory::BagTarget::Main);
    inv
},
```

**Step 2: Build and verify**

Run: `cargo build -p starbeam`
Expected: Compiles.

**Step 3: Commit**

```bash
git add src/player/mod.rs
git commit -m "feat(player): add test blueprints to starting inventory"
```

---

### Task 6: Manual integration test

**Step 1: Run the game**

Run: `cargo run -p starbeam`

**Step 2: Verify flow**

1. Start new game
2. Open inventory — verify 2 blueprint items present
3. Place blueprint in hotbar left-hand slot
4. Right-click to use — blueprint should disappear from inventory
5. Open hand crafting (C key or similar) — "wooden_sword" recipe should now appear
6. Verify second blueprint works the same way

**Step 3: Final commit if any fixes needed**
