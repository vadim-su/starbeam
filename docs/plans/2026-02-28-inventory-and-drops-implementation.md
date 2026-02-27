# Inventory and Drops Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement Starbound-style inventory system with block drops, item magnetism, and auto-pickup.

**Architecture:** Modular plugin-based design following existing patterns. Item registry mirrors tile registry. Dropped items are entities with physics. Inventory is a component on player.

**Tech Stack:** Bevy 0.18, Rust 2024, serde/ron for data files, bevy_egui for UI

---

## Phase 1: Item Registry

### Task 1.1: Create Item Definition Types

**Files:**
- Create: `src/item/mod.rs`
- Create: `src/item/definition.rs`

**Step 1: Write the failing test**

```rust
// src/item/definition.rs (at bottom, #[cfg(test)] mod tests)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_def_has_required_fields() {
        let item = ItemDef {
            id: "dirt".into(),
            display_name: "Dirt Block".into(),
            description: "A block of dirt".into(),
            max_stack: 999,
            rarity: Rarity::Common,
            item_type: ItemType::Block,
            icon: "items/dirt.png".into(),
            placeable: Some("dirt".into()),
            equipment_slot: None,
            stats: None,
        };
        
        assert_eq!(item.id, "dirt");
        assert_eq!(item.max_stack, 999);
        assert!(item.placeable.is_some());
    }

    #[test]
    fn drop_def_calculates_count() {
        let drop = DropDef {
            item_id: "dirt".into(),
            min: 1,
            max: 3,
            chance: 1.0,
        };
        
        assert!(drop.min <= drop.max);
        assert!(drop.chance <= 1.0);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test item_def_has_required_fields 2>&1`
Expected: Compilation error "cannot find type ItemDef"

**Step 3: Write minimal implementation**

```rust
// src/item/definition.rs

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Legendary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum ItemType {
    Block,
    Resource,
    Tool,
    Weapon,
    Armor,
    Consumable,
    Material,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum EquipmentSlot {
    Head,
    Chest,
    Legs,
    Back,
    Accessory1,
    Accessory2,
    Accessory3,
    Accessory4,
    Weapon1,
    Weapon2,
    Pet,
    CosmeticHead,
    CosmeticChest,
    CosmeticLegs,
    CosmeticBack,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemStats {
    pub damage: Option<f32>,
    pub defense: Option<f32>,
    pub speed_bonus: Option<f32>,
    pub health_bonus: Option<i32>,
}

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
    pub icon: String,
    pub placeable: Option<String>,
    pub equipment_slot: Option<EquipmentSlot>,
    pub stats: Option<ItemStats>,
}

fn default_max_stack() -> u16 { 99 }

#[derive(Debug, Clone, Deserialize)]
pub struct DropDef {
    pub item_id: String,
    #[serde(default = "default_drop_min")]
    pub min: u16,
    #[serde(default = "default_drop_max")]
    pub max: u16,
    #[serde(default = "default_drop_chance")]
    pub chance: f32,
}

fn default_drop_min() -> u16 { 1 }
fn default_drop_max() -> u16 { 1 }
fn default_drop_chance() -> f32 { 1.0 }
```

```rust
// src/item/mod.rs

pub mod definition;

pub use definition::*;
```

**Step 4: Run test to verify it passes**

Run: `cargo test item_def_has_required_fields 2>&1`
Expected: PASS

**Step 5: Commit**

```bash
git add src/item/mod.rs src/item/definition.rs
git commit -m "feat(item): add ItemDef and DropDef types"
```

---

### Task 1.2: Create Item Registry

**Files:**
- Create: `src/item/registry.rs`
- Modify: `src/item/mod.rs`

**Step 1: Write the failing test**

```rust
// src/item/registry.rs (at bottom)

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry() -> ItemRegistry {
        ItemRegistry::from_defs(vec![
            ItemDef {
                id: "dirt".into(),
                display_name: "Dirt Block".into(),
                description: "A block of dirt".into(),
                max_stack: 999,
                rarity: Rarity::Common,
                item_type: ItemType::Block,
                icon: "items/dirt.png".into(),
                placeable: Some("dirt".into()),
                equipment_slot: None,
                stats: None,
            },
            ItemDef {
                id: "stone".into(),
                display_name: "Stone".into(),
                description: "A block of stone".into(),
                max_stack: 999,
                rarity: Rarity::Common,
                item_type: ItemType::Block,
                icon: "items/stone.png".into(),
                placeable: Some("stone".into()),
                equipment_slot: None,
                stats: None,
            },
        ])
    }

    #[test]
    fn registry_lookup_by_name() {
        let reg = test_registry();
        let id = reg.by_name("dirt");
        assert_eq!(id, ItemId(0));
        assert_eq!(reg.by_name("stone"), ItemId(1));
    }

    #[test]
    fn registry_get_returns_def() {
        let reg = test_registry();
        let dirt = reg.get(ItemId(0));
        assert_eq!(dirt.id, "dirt");
        assert_eq!(dirt.max_stack, 999);
    }

    #[test]
    fn registry_max_stack() {
        let reg = test_registry();
        assert_eq!(reg.max_stack(ItemId(0)), 999);
        assert_eq!(reg.max_stack(ItemId(1)), 999);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test registry_lookup_by_name 2>&1`
Expected: Compilation error "cannot find type ItemRegistry"

**Step 3: Write minimal implementation**

```rust
// src/item/registry.rs

use std::collections::HashMap;

use bevy::prelude::*;

use super::definition::ItemDef;

/// Compact item identifier. Index into ItemRegistry.defs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ItemId(pub u16);

impl ItemId {
    pub const AIR: ItemId = ItemId(0);
}

/// Registry of all item definitions. Inserted as a Resource after asset loading.
#[derive(Resource, Debug)]
pub struct ItemRegistry {
    defs: Vec<ItemDef>,
    name_to_id: HashMap<String, ItemId>,
}

impl ItemRegistry {
    /// Build registry from a list of ItemDefs. Order = ItemId index.
    pub fn from_defs(defs: Vec<ItemDef>) -> Self {
        let name_to_id = defs
            .iter()
            .enumerate()
            .map(|(i, d)| (d.id.clone(), ItemId(i as u16)))
            .collect();
        Self { defs, name_to_id }
    }

    pub fn get(&self, id: ItemId) -> &ItemDef {
        &self.defs[id.0 as usize]
    }

    pub fn max_stack(&self, id: ItemId) -> u16 {
        self.defs[id.0 as usize].max_stack
    }

    pub fn by_name(&self, name: &str) -> ItemId {
        *self
            .name_to_id
            .get(name)
            .unwrap_or_else(|| panic!("Unknown item: {name}"))
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}
```

```rust
// src/item/mod.rs (update)

pub mod definition;
pub mod registry;

pub use definition::*;
pub use registry::*;
```

**Step 4: Run test to verify it passes**

Run: `cargo test registry_ 2>&1`
Expected: All PASS

**Step 5: Commit**

```bash
git add src/item/registry.rs src/item/mod.rs
git commit -m "feat(item): add ItemRegistry with lookup methods"
```

---

### Task 1.3: Extend TileDef with Drops

**Files:**
- Modify: `src/registry/tile.rs`
- Modify: `src/test_helpers.rs`

**Step 1: Write the failing test**

```rust
// src/registry/tile.rs (add to existing tests module)

#[test]
fn tile_def_has_drops() {
    let reg = test_registry();
    let dirt = reg.get(TileId(2)); // dirt is index 2
    // Initially empty drops
    assert!(dirt.drops.is_empty());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test tile_def_has_drops 2>&1`
Expected: Compilation error "no field `drops` on type `TileDef`"

**Step 3: Write minimal implementation**

```rust
// src/registry/tile.rs (add import at top)

use crate::item::DropDef;

// Modify TileDef struct (add field)

#[derive(Debug, Clone, Deserialize)]
pub struct TileDef {
    // ... existing fields ...
    #[serde(default)]
    pub drops: Vec<DropDef>,
}
```

**Step 4: Update test_helpers.rs**

```rust
// src/test_helpers.rs (update test_tile_registry)

// Add to each TileDef:
drops: vec![],
```

**Step 5: Run test to verify it passes**

Run: `cargo test tile_def_has_drops 2>&1`
Expected: PASS

**Step 6: Run all tests**

Run: `cargo test 2>&1`
Expected: All PASS

**Step 7: Commit**

```bash
git add src/registry/tile.rs src/test_helpers.rs
git commit -m "feat(tile): add drops field to TileDef"
```

---

### Task 1.4: Register Item Plugin in Main

**Files:**
- Modify: `src/main.rs`
- Modify: `src/lib.rs` (if exists) or create `src/item/plugin.rs`

**Step 1: Create ItemPlugin**

```rust
// src/item/plugin.rs

use bevy::prelude::*;

use super::registry::ItemRegistry;

pub struct ItemPlugin;

impl Plugin for ItemPlugin {
    fn build(&self, app: &mut App) {
        // For now, insert empty registry (will be loaded from assets later)
        app.insert_resource(ItemRegistry::from_defs(vec![]));
    }
}
```

```rust
// src/item/mod.rs (update)

pub mod definition;
pub mod registry;
pub mod plugin;

pub use definition::*;
pub use registry::*;
pub use plugin::ItemPlugin;
```

**Step 2: Add to main.rs**

```rust
// src/main.rs (add import and plugin)

mod item;

// In App::new().add_plugins(...)
.add_plugins(item::ItemPlugin)
```

**Step 3: Run to verify it compiles**

Run: `cargo build 2>&1`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add src/item/plugin.rs src/item/mod.rs src/main.rs
git commit -m "feat(item): add ItemPlugin and register in App"
```

---

## Phase 2: Dropped Item Entity

### Task 2.1: Create DroppedItem Components

**Files:**
- Create: `src/item/dropped_item.rs`

**Step 1: Write the failing test**

```rust
// src/item/dropped_item.rs (at bottom)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_item_has_required_fields() {
        let item = DroppedItem {
            item_id: "dirt".into(),
            count: 5,
            velocity: Vec2::ZERO,
            lifetime: Timer::from_seconds(300.0, TimerMode::Once),
            magnetized: false,
        };
        
        assert_eq!(item.item_id, "dirt");
        assert_eq!(item.count, 5);
        assert!(!item.magnetized);
    }

    #[test]
    fn dropped_item_physics_defaults() {
        let physics = DroppedItemPhysics::default();
        assert_eq!(physics.gravity, 400.0);
        assert_eq!(physics.friction, 0.9);
        assert_eq!(physics.bounce, 0.3);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test dropped_item_has_required_fields 2>&1`
Expected: Compilation error

**Step 3: Write minimal implementation**

```rust
// src/item/dropped_item.rs

use bevy::prelude::*;

/// A dropped item entity in the world.
#[derive(Component, Debug)]
pub struct DroppedItem {
    pub item_id: String,
    pub count: u16,
    pub velocity: Vec2,
    pub lifetime: Timer,
    pub magnetized: bool,
}

/// Physics parameters for dropped items.
#[derive(Component, Debug, Clone, Copy)]
pub struct DroppedItemPhysics {
    pub gravity: f32,
    pub friction: f32,
    pub bounce: f32,
}

impl Default for DroppedItemPhysics {
    fn default() -> Self {
        Self {
            gravity: 400.0,
            friction: 0.9,
            bounce: 0.3,
        }
    }
}

/// Configuration for item pickup behavior.
#[derive(Resource, Debug, Clone)]
pub struct PickupConfig {
    pub magnet_radius: f32,
    pub magnet_strength: f32,
    pub pickup_radius: f32,
}

impl Default for PickupConfig {
    fn default() -> Self {
        Self {
            magnet_radius: 48.0,    // 3 tiles
            magnet_strength: 200.0,
            pickup_radius: 16.0,   // 1 tile
        }
    }
}
```

```rust
// src/item/mod.rs (update)

pub mod definition;
pub mod registry;
pub mod plugin;
pub mod dropped_item;

pub use definition::*;
pub use registry::*;
pub use plugin::ItemPlugin;
pub use dropped_item::*;
```

**Step 4: Run test to verify it passes**

Run: `cargo test dropped_item 2>&1`
Expected: All PASS

**Step 5: Commit**

```bash
git add src/item/dropped_item.rs src/item/mod.rs
git commit -m "feat(item): add DroppedItem components and PickupConfig"
```

---

### Task 2.2: Create Spawn Function for Dropped Items

**Files:**
- Modify: `src/item/dropped_item.rs`

**Step 1: Write the failing test**

```rust
// src/item/dropped_item.rs (add to tests)

#[test]
fn spawn_params_calculates_velocity() {
    let params = SpawnParams {
        position: Vec2::new(100.0, 200.0),
        angle: std::f32::consts::FRAC_PI_2, // 90 degrees (straight up)
        speed: 100.0,
    };
    
    // At 90 degrees: cos = 0, sin = 1
    assert!(params.velocity().x.abs() < 0.1);
    assert!((params.velocity().y - 100.0).abs() < 0.1);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test spawn_params 2>&1`
Expected: Compilation error

**Step 3: Write minimal implementation**

```rust
// src/item/dropped_item.rs (add after PickupConfig)

/// Parameters for spawning a dropped item.
pub struct SpawnParams {
    pub position: Vec2,
    pub angle: f32,
    pub speed: f32,
}

impl SpawnParams {
    /// Create spawn params with random angle (60°-150°) and speed (80-150).
    pub fn random(position: Vec2) -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let angle = rng.gen_range(0.6..2.5); // ~60°-150° in radians
        let speed = rng.gen_range(80.0..150.0);
        Self { position, angle, speed }
    }
    
    /// Calculate initial velocity from angle and speed.
    pub fn velocity(&self) -> Vec2 {
        Vec2::new(self.angle.cos(), self.angle.sin()) * self.speed
    }
}

/// Calculate drops from a tile definition.
pub fn calculate_drops(
    tile_drops: &[crate::item::DropDef],
) -> Vec<(String, u16)> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    
    tile_drops
        .iter()
        .filter_map(|drop| {
            if rng.gen::<f32>() < drop.chance {
                let count = rng.gen_range(drop.min..=drop.max);
                Some((drop.item_id.clone(), count))
            } else {
                None
            }
        })
        .collect()
}
```

**Step 4: Add rand dependency**

```toml
# Cargo.toml (add to dependencies)
rand = "0.8"
```

**Step 5: Run test to verify it passes**

Run: `cargo test spawn_params 2>&1`
Expected: PASS

**Step 6: Commit**

```bash
git add src/item/dropped_item.rs Cargo.toml
git commit -m "feat(item): add SpawnParams and calculate_drops functions"
```

---

### Task 2.3: Create Dropped Item Physics System

**Files:**
- Modify: `src/item/dropped_item.rs`
- Modify: `src/item/plugin.rs`

**Step 1: Write the failing test**

```rust
// src/item/dropped_item.rs (add to tests)

#[test]
fn physics_system_applies_gravity() {
    // This tests the pure calculation logic
    let velocity = Vec2::new(50.0, 100.0);
    let gravity = 400.0;
    let delta = 0.016; // ~60fps
    
    let new_velocity = apply_gravity(velocity, gravity, delta);
    
    assert_eq!(new_velocity.x, 50.0);
    assert!((new_velocity.y - (100.0 - gravity * delta)).abs() < 0.1);
}

#[test]
fn physics_system_applies_friction_when_grounded() {
    let velocity = Vec2::new(100.0, 0.0);
    let friction = 0.9;
    
    let new_velocity = apply_friction(velocity, friction);
    
    assert!((new_velocity.x - 90.0).abs() < 0.1);
    assert_eq!(new_velocity.y, 0.0);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test physics_system 2>&1`
Expected: Compilation error

**Step 3: Write minimal implementation**

```rust
// src/item/dropped_item.rs (add pure physics functions)

/// Apply gravity to velocity (pure function for testing).
pub fn apply_gravity(velocity: Vec2, gravity: f32, delta: f32) -> Vec2 {
    Vec2::new(velocity.x, velocity.y - gravity * delta)
}

/// Apply friction to velocity (pure function for testing).
pub fn apply_friction(velocity: Vec2, friction: f32) -> Vec2 {
    Vec2::new(velocity.x * friction, velocity.y * friction)
}

/// Apply bounce on collision (pure function for testing).
pub fn apply_bounce(velocity: Vec2, bounce: f32, hit_ground: bool) -> Vec2 {
    if hit_ground && velocity.y < 0.0 {
        Vec2::new(velocity.x * 0.9, -velocity.y * bounce)
    } else {
        velocity
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test physics_system 2>&1`
Expected: All PASS

**Step 5: Create the Bevy system**

```rust
// src/item/dropped_item.rs (add system)

/// System that updates dropped item physics.
pub fn dropped_item_physics_system(
    time: Res<Time>,
    mut query: Query<(&mut DroppedItem, &DroppedItemPhysics, &mut Transform)>,
) {
    let delta = time.delta_secs();
    
    for (mut item, physics, mut transform) in &mut query {
        // Apply gravity
        item.velocity = apply_gravity(item.velocity, physics.gravity, delta);
        
        // Update position
        transform.translation.x += item.velocity.x * delta;
        transform.translation.y += item.velocity.y * delta;
        
        // Apply friction when moving slowly
        if item.velocity.length() < 10.0 {
            item.velocity = apply_friction(item.velocity, physics.friction);
        }
        
        // Update lifetime
        item.lifetime.tick(time.delta());
    }
}
```

**Step 6: Register system in plugin**

```rust
// src/item/plugin.rs (update)

use bevy::prelude::*;

use super::registry::ItemRegistry;
use super::dropped_item::{dropped_item_physics_system, PickupConfig};

pub struct ItemPlugin;

impl Plugin for ItemPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ItemRegistry::from_defs(vec![]))
            .insert_resource(PickupConfig::default())
            .add_systems(Update, dropped_item_physics_system);
    }
}
```

**Step 7: Run all tests**

Run: `cargo test 2>&1`
Expected: All PASS

**Step 8: Commit**

```bash
git add src/item/dropped_item.rs src/item/plugin.rs
git commit -m "feat(item): add dropped item physics system"
```

---

## Phase 3: Inventory Core

### Task 3.1: Create Inventory Components

**Files:**
- Create: `src/inventory/mod.rs`
- Create: `src/inventory/components.rs`

**Step 1: Write the failing test**

```rust
// src/inventory/components.rs (at bottom)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_slot_tracks_item_and_count() {
        let slot = InventorySlot {
            item_id: "dirt".into(),
            count: 50,
        };
        
        assert_eq!(slot.item_id, "dirt");
        assert_eq!(slot.count, 50);
    }

    #[test]
    fn inventory_starts_empty() {
        let inv = Inventory::new();
        
        assert_eq!(inv.main_bag.len(), 40);
        assert_eq!(inv.material_bag.len(), 40);
        assert!(inv.main_bag.iter().all(|s| s.is_none()));
    }

    #[test]
    fn inventory_total_slots_includes_bonus() {
        let mut inv = Inventory::new();
        assert_eq!(inv.total_slots(), 40);
        
        inv.max_slots_bonus = 10;
        assert_eq!(inv.total_slots(), 50);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test inventory_slot 2>&1`
Expected: Compilation error

**Step 3: Write minimal implementation**

```rust
// src/inventory/components.rs

use bevy::prelude::*;

/// A single slot in the inventory.
#[derive(Clone, Debug, PartialEq)]
pub struct InventorySlot {
    pub item_id: String,
    pub count: u16,
}

/// Player inventory component.
#[derive(Component, Debug)]
pub struct Inventory {
    pub main_bag: Vec<Option<InventorySlot>>,
    pub material_bag: Vec<Option<InventorySlot>>,
    pub max_slots_base: usize,
    pub max_slots_bonus: usize,
}

impl Inventory {
    pub fn new() -> Self {
        Self {
            main_bag: vec![None; 40],
            material_bag: vec![None; 40],
            max_slots_base: 40,
            max_slots_bonus: 0,
        }
    }
    
    pub fn total_slots(&self) -> usize {
        self.max_slots_base + self.max_slots_bonus
    }
}

impl Default for Inventory {
    fn default() -> Self {
        Self::new()
    }
}
```

```rust
// src/inventory/mod.rs

pub mod components;

pub use components::*;
```

**Step 4: Run test to verify it passes**

Run: `cargo test inventory 2>&1`
Expected: All PASS

**Step 5: Commit**

```bash
git add src/inventory/mod.rs src/inventory/components.rs
git commit -m "feat(inventory): add Inventory and InventorySlot components"
```

---

### Task 3.2: Implement Item Stacking Logic

**Files:**
- Modify: `src/inventory/components.rs`

**Step 1: Write the failing test**

```rust
// src/inventory/components.rs (add to tests)

#[test]
fn try_add_item_to_empty_slot() {
    let mut inv = Inventory::new();
    let remaining = inv.try_add_item("dirt", 10, 999);
    
    assert_eq!(remaining, 0);
    assert!(inv.main_bag[0].is_some());
    assert_eq!(inv.main_bag[0].as_ref().unwrap().count, 10);
}

#[test]
fn try_add_item_stacks_into_existing() {
    let mut inv = Inventory::new();
    inv.main_bag[0] = Some(InventorySlot { item_id: "dirt".into(), count: 50 });
    
    let remaining = inv.try_add_item("dirt", 30, 999);
    
    assert_eq!(remaining, 0);
    assert_eq!(inv.main_bag[0].as_ref().unwrap().count, 80);
}

#[test]
fn try_add_item_respects_max_stack() {
    let mut inv = Inventory::new();
    inv.main_bag[0] = Some(InventorySlot { item_id: "dirt".into(), count: 990 });
    
    let remaining = inv.try_add_item("dirt", 20, 999);
    
    assert_eq!(remaining, 11); // 990 + 9 = 999, 11 left over
    assert_eq!(inv.main_bag[0].as_ref().unwrap().count, 999);
}

#[test]
fn try_add_item_creates_new_slot_when_full() {
    let mut inv = Inventory::new();
    inv.main_bag[0] = Some(InventorySlot { item_id: "dirt".into(), count: 999 });
    
    let remaining = inv.try_add_item("dirt", 10, 999);
    
    assert_eq!(remaining, 0);
    assert_eq!(inv.main_bag[1].as_ref().unwrap().count, 10);
}

#[test]
fn try_add_item_returns_remainder_when_inventory_full() {
    let mut inv = Inventory::new();
    // Fill all slots
    for slot in inv.main_bag.iter_mut() {
        *slot = Some(InventorySlot { item_id: "stone".into(), count: 999 });
    }
    
    let remaining = inv.try_add_item("dirt", 10, 999);
    
    assert_eq!(remaining, 10);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test try_add_item 2>&1`
Expected: Compilation error "no method `try_add_item`"

**Step 3: Write minimal implementation**

```rust
// src/inventory/components.rs (add to Inventory impl)

impl Inventory {
    // ... existing methods ...
    
    /// Try to add an item to the inventory.
    /// Returns the count that couldn't fit.
    pub fn try_add_item(&mut self, item_id: &str, count: u16, max_stack: u16) -> u16 {
        let mut remaining = count;
        
        // First, try to stack into existing slots
        for slot in self.main_bag.iter_mut() {
            if remaining == 0 { break; }
            
            if let Some(s) = slot {
                if s.item_id == item_id && s.count < max_stack {
                    let can_add = max_stack - s.count;
                    let to_add = remaining.min(can_add);
                    s.count += to_add;
                    remaining -= to_add;
                }
            }
        }
        
        // Then, try to create new slots
        if remaining > 0 {
            for slot in self.main_bag.iter_mut() {
                if remaining == 0 { break; }
                
                if slot.is_none() {
                    let to_add = remaining.min(max_stack);
                    *slot = Some(InventorySlot {
                        item_id: item_id.to_string(),
                        count: to_add,
                    });
                    remaining -= to_add;
                }
            }
        }
        
        remaining
    }
    
    /// Count total items of a specific type.
    pub fn count_item(&self, item_id: &str) -> u16 {
        self.main_bag
            .iter()
            .chain(self.material_bag.iter())
            .filter_map(|s| s.as_ref())
            .filter(|s| s.item_id == item_id)
            .map(|s| s.count)
            .sum()
    }
    
    /// Remove items from inventory. Returns true if successful.
    pub fn remove_item(&mut self, item_id: &str, count: u16) -> bool {
        let total = self.count_item(item_id);
        if total < count {
            return false;
        }
        
        let mut remaining = count;
        
        for slot in self.main_bag.iter_mut().chain(self.material_bag.iter_mut()) {
            if remaining == 0 { break; }
            
            if let Some(s) = slot {
                if s.item_id == item_id {
                    let to_remove = remaining.min(s.count);
                    s.count -= to_remove;
                    remaining -= to_remove;
                    
                    if s.count == 0 {
                        *slot = None;
                    }
                }
            }
        }
        
        true
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test try_add_item 2>&1`
Expected: All PASS

**Step 5: Commit**

```bash
git add src/inventory/components.rs
git commit -m "feat(inventory): implement item stacking with try_add_item"
```

---

### Task 3.3: Create Inventory Plugin

**Files:**
- Create: `src/inventory/plugin.rs`
- Modify: `src/inventory/mod.rs`
- Modify: `src/main.rs`

**Step 1: Create plugin**

```rust
// src/inventory/plugin.rs

use bevy::prelude::*;

use super::components::Inventory;

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        // Inventory will be added to player entity
    }
}
```

```rust
// src/inventory/mod.rs (update)

pub mod components;
pub mod plugin;

pub use components::*;
pub use plugin::InventoryPlugin;
```

**Step 2: Register in main.rs**

```rust
// src/main.rs (add)

mod inventory;

// In add_plugins:
.add_plugins(inventory::InventoryPlugin)
```

**Step 3: Run to verify it compiles**

Run: `cargo build 2>&1`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add src/inventory/plugin.rs src/inventory/mod.rs src/main.rs
git commit -m "feat(inventory): add InventoryPlugin"
```

---

## Phase 4: Item Magnetism and Pickup

### Task 4.1: Create Magnetism System

**Files:**
- Create: `src/inventory/systems.rs`
- Modify: `src/inventory/mod.rs`
- Modify: `src/inventory/plugin.rs`

**Step 1: Write the failing test**

```rust
// src/inventory/systems.rs (at bottom)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculate_magnet_strength_increases_near_player() {
        let config = PickupConfig::default();
        
        // At edge of magnet radius
        let strength = calculate_magnet_strength(48.0, &config);
        assert!(strength > 0.0);
        
        // Very close to player
        let strength_close = calculate_magnet_strength(10.0, &config);
        assert!(strength_close > strength);
    }

    #[test]
    fn calculate_magnet_strength_zero_outside_radius() {
        let config = PickupConfig::default();
        
        let strength = calculate_magnet_strength(100.0, &config);
        assert_eq!(strength, 0.0);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test calculate_magnet_strength 2>&1`
Expected: Compilation error

**Step 3: Write minimal implementation**

```rust
// src/inventory/systems.rs

use bevy::prelude::*;

use crate::item::{DroppedItem, PickupConfig};
use crate::player::Player;

/// Calculate magnet strength based on distance (pure function for testing).
pub fn calculate_magnet_strength(distance: f32, config: &PickupConfig) -> f32 {
    if distance >= config.magnet_radius {
        return 0.0;
    }
    
    // Strength increases as distance decreases
    config.magnet_strength * (1.0 - distance / config.magnet_radius)
}

/// System that pulls dropped items toward the player.
pub fn item_magnetism_system(
    config: Res<PickupConfig>,
    time: Res<Time>,
    player_query: Query<&Transform, With<Player>>,
    mut item_query: Query<(&Transform, &mut DroppedItem)>,
) {
    let Ok(player_tf) = player_query.single() else { return };
    let player_pos = player_tf.translation.truncate();
    let delta = time.delta_secs();
    
    for (item_tf, mut item) in &mut item_query {
        let item_pos = item_tf.translation.truncate();
        let distance = player_pos.distance(item_pos);
        
        // Activate magnet when in range
        if distance < config.magnet_radius {
            item.magnetized = true;
        }
        
        // Apply magnetism
        if item.magnetized && distance > 0.0 {
            let direction = (player_pos - item_pos).normalize();
            let strength = calculate_magnet_strength(distance, &config);
            
            item.velocity.x += direction.x * strength * delta;
            item.velocity.y += direction.y * strength * delta;
        }
    }
}
```

```rust
// src/inventory/mod.rs (update)

pub mod components;
pub mod plugin;
pub mod systems;

pub use components::*;
pub use plugin::InventoryPlugin;
pub use systems::*;
```

**Step 4: Run test to verify it passes**

Run: `cargo test calculate_magnet_strength 2>&1`
Expected: All PASS

**Step 5: Register system in plugin**

```rust
// src/inventory/plugin.rs (update)

use bevy::prelude::*;

use super::components::Inventory;
use super::systems::item_magnetism_system;
use crate::item::PickupConfig;

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PickupConfig::default())
            .add_systems(Update, item_magnetism_system);
    }
}
```

**Step 6: Run all tests**

Run: `cargo test 2>&1`
Expected: All PASS

**Step 7: Commit**

```bash
git add src/inventory/systems.rs src/inventory/mod.rs src/inventory/plugin.rs
git commit -m "feat(inventory): add item magnetism system"
```

---

### Task 4.2: Create Pickup System

**Files:**
- Modify: `src/inventory/systems.rs`
- Modify: `src/inventory/plugin.rs`

**Step 1: Write the failing test**

```rust
// src/inventory/systems.rs (add to tests)

#[test]
fn should_pickup_within_radius() {
    let config = PickupConfig::default();
    
    assert!(should_pickup(10.0, &config));
    assert!(should_pickup(16.0, &config));
    assert!(!should_pickup(20.0, &config));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test should_pickup 2>&1`
Expected: Compilation error

**Step 3: Write minimal implementation**

```rust
// src/inventory/systems.rs (add)

use crate::item::ItemRegistry;

/// Check if item should be picked up (pure function for testing).
pub fn should_pickup(distance: f32, config: &PickupConfig) -> bool {
    distance < config.pickup_radius
}

/// Event fired when an item is picked up.
#[derive(Event, Debug)]
pub struct ItemPickupEvent {
    pub item_id: String,
    pub count: u16,
}

/// System that detects and triggers item pickup.
#[allow(clippy::too_many_arguments)]
pub fn item_pickup_system(
    config: Res<PickupConfig>,
    player_query: Query<(Entity, &Transform, &mut Inventory), With<Player>>,
    item_registry: Res<ItemRegistry>,
    mut item_query: Query<(Entity, &Transform, &DroppedItem)>,
    mut commands: Commands,
    mut pickup_events: EventWriter<ItemPickupEvent>,
) {
    let Ok((player_entity, player_tf, mut inventory)) = player_query.single() else { return };
    let player_pos = player_tf.translation.truncate();
    
    for (item_entity, item_tf, item) in &mut item_query {
        let item_pos = item_tf.translation.truncate();
        let distance = player_pos.distance(item_pos);
        
        if should_pickup(distance, &config) {
            // Try to add to inventory
            let max_stack = item_registry.max_stack(
                item_registry.by_name(&item.item_id)
            );
            let remaining = inventory.try_add_item(&item.item_id, item.count, max_stack);
            
            if remaining == 0 {
                // Successfully picked up
                commands.entity(item_entity).despawn();
                pickup_events.send(ItemPickupEvent {
                    item_id: item.item_id.clone(),
                    count: item.count,
                });
            }
        }
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test should_pickup 2>&1`
Expected: PASS

**Step 5: Register system and event in plugin**

```rust
// src/inventory/plugin.rs (update)

use bevy::prelude::*;

use super::components::Inventory;
use super::systems::{item_magnetism_system, item_pickup_system, ItemPickupEvent};
use crate::item::PickupConfig;

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PickupConfig::default())
            .add_event::<ItemPickupEvent>()
            .add_systems(Update, (
                item_magnetism_system,
                item_pickup_system,
            ).chain());
    }
}
```

**Step 6: Run all tests**

Run: `cargo test 2>&1`
Expected: All PASS

**Step 7: Commit**

```bash
git add src/inventory/systems.rs src/inventory/plugin.rs
git commit -m "feat(inventory): add item pickup system with events"
```

---

### Task 4.3: Add Inventory to Player

**Files:**
- Modify: `src/player/mod.rs`

**Step 1: Find player spawn location**

Run: `grep -n "spawn_player" src/player/`
Expected: Find the function that spawns the player

**Step 2: Add Inventory component to player**

```rust
// src/player/mod.rs (or wherever spawn_player is)

// Add import
use crate::inventory::Inventory;

// In spawn_player function, add to player entity bundle:
.insert(Inventory::new())
```

**Step 3: Run to verify it compiles**

Run: `cargo build 2>&1`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add src/player/mod.rs
git commit -m "feat(player): add Inventory component to player"
```

---

## Phase 5: Block Drop Integration

### Task 5.1: Spawn Drops on Block Break

**Files:**
- Modify: `src/interaction/block_action.rs`

**Step 1: Add imports and modify block breaking**

```rust
// src/interaction/block_action.rs (add imports)

use crate::item::{calculate_drops, DroppedItem, DroppedItemPhysics, SpawnParams};
use crate::inventory::Inventory;
use rand::Rng;

// In block_interaction_system, after breaking a tile:

// BEFORE:
world_map.set_tile(tile_x, tile_y, Layer::Fg, TileId::AIR, &ctx_ref);

// AFTER:
let old_tile = world_map.get_tile(tile_x, tile_y, Layer::Fg, &ctx_ref);
world_map.set_tile(tile_x, tile_y, Layer::Fg, TileId::AIR, &ctx_ref);

// Spawn drops
if let Some(tile_id) = old_tile {
    let tile_def = ctx_ref.tile_registry.get(tile_id);
    let drops = calculate_drops(&tile_def.drops);
    
    let tile_world_x = tile_x as f32 * ctx_ref.config.tile_size;
    let tile_world_y = tile_y as f32 * ctx_ref.config.tile_size;
    let tile_center = Vec2::new(
        tile_world_x + ctx_ref.config.tile_size / 2.0,
        tile_world_y + ctx_ref.config.tile_size / 2.0,
    );
    
    for (item_id, count) in drops {
        let params = SpawnParams::random(tile_center);
        
        commands.spawn((
            DroppedItem {
                item_id,
                count,
                velocity: params.velocity(),
                lifetime: Timer::from_seconds(300.0, TimerMode::Once),
                magnetized: false,
            },
            DroppedItemPhysics::default(),
            Transform::from_translation(tile_center.extend(1.0)),
            // Add sprite when assets are ready
        ));
    }
}
```

**Step 2: Run to verify it compiles**

Run: `cargo build 2>&1`
Expected: SUCCESS (may have warnings about unused)

**Step 3: Commit**

```bash
git add src/interaction/block_action.rs
git commit -m "feat(interaction): spawn dropped items when breaking blocks"
```

---

## Phase 6: Hotbar System

### Task 6.1: Create Hotbar Component

**Files:**
- Create: `src/inventory/hotbar.rs`
- Modify: `src/inventory/mod.rs`

**Step 1: Write the failing test**

```rust
// src/inventory/hotbar.rs (at bottom)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotbar_has_6_slots() {
        let hotbar = Hotbar::new();
        assert_eq!(hotbar.slots.len(), 6);
    }

    #[test]
    fn hotbar_active_slot_defaults_to_zero() {
        let hotbar = Hotbar::new();
        assert_eq!(hotbar.active_slot, 0);
    }

    #[test]
    fn hotbar_select_slot_wraps() {
        let mut hotbar = Hotbar::new();
        hotbar.select_slot(5);
        assert_eq!(hotbar.active_slot, 5);
        
        hotbar.select_slot(6); // Should wrap or clamp
        assert_eq!(hotbar.active_slot, 0);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test hotbar_has_6_slots 2>&1`
Expected: Compilation error

**Step 3: Write minimal implementation**

```rust
// src/inventory/hotbar.rs

use bevy::prelude::*;

/// A single hotbar slot with left/right hand items.
#[derive(Clone, Debug, Default)]
pub struct HotbarSlot {
    pub left_hand: Option<String>,
    pub right_hand: Option<String>,
}

/// Player hotbar component (Starbound-style).
#[derive(Component, Debug)]
pub struct Hotbar {
    pub slots: [HotbarSlot; 6],
    pub active_slot: usize,
    pub locked: bool,
}

impl Hotbar {
    pub fn new() -> Self {
        Self {
            slots: Default::default(),
            active_slot: 0,
            locked: false,
        }
    }
    
    pub fn select_slot(&mut self, slot: usize) {
        self.active_slot = slot % 6;
    }
    
    pub fn active_slot(&self) -> &HotbarSlot {
        &self.slots[self.active_slot]
    }
    
    pub fn get_item_for_hand(&self, is_left: bool) -> Option<&str> {
        let slot = self.active_slot();
        if is_left {
            slot.left_hand.as_deref()
        } else {
            slot.right_hand.as_deref()
        }
    }
}

impl Default for Hotbar {
    fn default() -> Self {
        Self::new()
    }
}
```

```rust
// src/inventory/mod.rs (update)

pub mod components;
pub mod hotbar;
pub mod plugin;
pub mod systems;

pub use components::*;
pub use hotbar::*;
pub use plugin::InventoryPlugin;
pub use systems::*;
```

**Step 4: Run test to verify it passes**

Run: `cargo test hotbar 2>&1`
Expected: All PASS

**Step 5: Commit**

```bash
git add src/inventory/hotbar.rs src/inventory/mod.rs
git commit -m "feat(inventory): add Hotbar component with L/R hand slots"
```

---

### Task 6.2: Add Hotbar Input System

**Files:**
- Modify: `src/inventory/systems.rs`
- Modify: `src/inventory/plugin.rs`

**Step 1: Create hotbar input system**

```rust
// src/inventory/systems.rs (add)

use super::hotbar::Hotbar;

/// System that handles hotbar slot selection via number keys.
pub fn hotbar_input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut hotbar_query: Query<&mut Hotbar>,
) {
    let Ok(mut hotbar) = hotbar_query.single() else { return };
    
    // Number keys 1-6 select slots
    for (i, key) in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3,
                     KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6]
                     .iter().enumerate() {
        if keyboard.just_pressed(*key) {
            hotbar.select_slot(i);
        }
    }
}
```

**Step 2: Register system**

```rust
// src/inventory/plugin.rs (update)

use super::systems::{hotbar_input_system, item_magnetism_system, item_pickup_system, ItemPickupEvent};

// In build():
.add_systems(Update, (
    hotbar_input_system,
    item_magnetism_system,
    item_pickup_system,
).chain());
```

**Step 3: Run to verify it compiles**

Run: `cargo build 2>&1`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add src/inventory/systems.rs src/inventory/plugin.rs
git commit -m "feat(inventory): add hotbar input system for slot selection"
```

---

## Phase 7: Equipment System

### Task 7.1: Create Equipment Component

**Files:**
- Create: `src/inventory/equipment.rs`
- Modify: `src/inventory/mod.rs`

**Step 1: Write the failing test**

```rust
// src/inventory/equipment.rs (at bottom)

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::EquipmentSlot;

    #[test]
    fn equipment_starts_empty() {
        let equip = Equipment::new();
        
        assert!(equip.get(EquipmentSlot::Head).is_none());
        assert!(equip.get(EquipmentSlot::Chest).is_none());
    }

    #[test]
    fn equipment_can_equip_item() {
        let mut equip = Equipment::new();
        
        equip.equip(EquipmentSlot::Head, "iron_helmet".into());
        
        assert_eq!(equip.get(EquipmentSlot::Head), Some(&"iron_helmet".into()));
    }

    #[test]
    fn equipment_unequip_returns_item() {
        let mut equip = Equipment::new();
        equip.equip(EquipmentSlot::Head, "iron_helmet".into());
        
        let item = equip.unequip(EquipmentSlot::Head);
        
        assert_eq!(item, Some("iron_helmet".into()));
        assert!(equip.get(EquipmentSlot::Head).is_none());
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test equipment_starts_empty 2>&1`
Expected: Compilation error

**Step 3: Write minimal implementation**

```rust
// src/inventory/equipment.rs

use std::collections::HashMap;

use bevy::prelude::*;
use crate::item::EquipmentSlot;

/// Player equipment component.
#[derive(Component, Debug)]
pub struct Equipment {
    slots: HashMap<EquipmentSlot, Option<String>>,
}

impl Equipment {
    pub fn new() -> Self {
        let mut slots = HashMap::new();
        for slot in [
            EquipmentSlot::Head,
            EquipmentSlot::Chest,
            EquipmentSlot::Legs,
            EquipmentSlot::Back,
            EquipmentSlot::Accessory1,
            EquipmentSlot::Accessory2,
            EquipmentSlot::Accessory3,
            EquipmentSlot::Accessory4,
            EquipmentSlot::Weapon1,
            EquipmentSlot::Weapon2,
            EquipmentSlot::Pet,
            EquipmentSlot::CosmeticHead,
            EquipmentSlot::CosmeticChest,
            EquipmentSlot::CosmeticLegs,
            EquipmentSlot::CosmeticBack,
        ] {
            slots.insert(slot, None);
        }
        Self { slots }
    }
    
    pub fn get(&self, slot: EquipmentSlot) -> Option<&String> {
        self.slots.get(&slot).and_then(|s| s.as_ref())
    }
    
    pub fn equip(&mut self, slot: EquipmentSlot, item_id: String) {
        self.slots.insert(slot, Some(item_id));
    }
    
    pub fn unequip(&mut self, slot: EquipmentSlot) -> Option<String> {
        self.slots.insert(slot, None).flatten()
    }
}

impl Default for Equipment {
    fn default() -> Self {
        Self::new()
    }
}
```

```rust
// src/inventory/mod.rs (update)

pub mod components;
pub mod equipment;
pub mod hotbar;
pub mod plugin;
pub mod systems;

pub use components::*;
pub use equipment::*;
pub use hotbar::*;
pub use plugin::InventoryPlugin;
pub use systems::*;
```

**Step 4: Run test to verify it passes**

Run: `cargo test equipment 2>&1`
Expected: All PASS

**Step 5: Commit**

```bash
git add src/inventory/equipment.rs src/inventory/mod.rs
git commit -m "feat(inventory): add Equipment component with all slots"
```

---

## Phase 8: Crafting System

### Task 8.1: Create Recipe Types

**Files:**
- Create: `src/crafting/mod.rs`
- Create: `src/crafting/recipe.rs`

**Step 1: Write the failing test**

```rust
// src/crafting/recipe.rs (at bottom)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_has_result_and_ingredients() {
        let recipe = Recipe {
            id: "torch".into(),
            result: RecipeResult { item_id: "torch".into(), count: 4 },
            ingredients: vec![
                Ingredient { item_id: "coal".into(), count: 1 },
                Ingredient { item_id: "wood".into(), count: 1 },
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
```

**Step 2: Run test to verify it fails**

Run: `cargo test recipe_has_result 2>&1`
Expected: Compilation error

**Step 3: Write minimal implementation**

```rust
// src/crafting/recipe.rs

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
            UnlockCondition::Blueprint(_) => false, // TODO: implement blueprint tracking
            UnlockCondition::Station(_) => false,   // TODO: implement station tracking
        }
    }
}
```

```rust
// src/crafting/mod.rs

pub mod recipe;

pub use recipe::*;
```

**Step 4: Run test to verify it passes**

Run: `cargo test recipe 2>&1`
Expected: All PASS

**Step 5: Commit**

```bash
git add src/crafting/mod.rs src/crafting/recipe.rs
git commit -m "feat(crafting): add Recipe types and unlock conditions"
```

---

### Task 8.2: Create Recipe Registry

**Files:**
- Create: `src/crafting/registry.rs`
- Modify: `src/crafting/mod.rs`

**Step 1: Write the failing test**

```rust
// src/crafting/registry.rs (at bottom)

#[cfg(test)]
mod tests {
    use super::*;

    fn test_recipe() -> Recipe {
        Recipe {
            id: "torch".into(),
            result: RecipeResult { item_id: "torch".into(), count: 4 },
            ingredients: vec![
                Ingredient { item_id: "coal".into(), count: 1 },
                Ingredient { item_id: "wood".into(), count: 1 },
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
```

**Step 2: Run test to verify it fails**

Run: `cargo test registry_stores_recipes 2>&1`
Expected: Compilation error

**Step 3: Write minimal implementation**

```rust
// src/crafting/registry.rs

use std::collections::HashMap;

use bevy::prelude::*;
use super::recipe::{Recipe, UnlockCondition};

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
                    && r.ingredients.iter().all(|ing| {
                        inventory.count_item(&ing.item_id) >= ing.count
                    })
            })
            .collect()
    }
}

impl Default for RecipeRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

```rust
// src/crafting/mod.rs (update)

pub mod recipe;
pub mod registry;

pub use recipe::*;
pub use registry::*;
```

**Step 4: Run test to verify it passes**

Run: `cargo test registry 2>&1`
Expected: All PASS

**Step 5: Commit**

```bash
git add src/crafting/registry.rs src/crafting/mod.rs
git commit -m "feat(crafting): add RecipeRegistry with station filtering"
```

---

### Task 8.3: Create Crafting Plugin

**Files:**
- Create: `src/crafting/plugin.rs`
- Modify: `src/crafting/mod.rs`
- Modify: `src/main.rs`

**Step 1: Create plugin**

```rust
// src/crafting/plugin.rs

use bevy::prelude::*;

use super::registry::RecipeRegistry;

pub struct CraftingPlugin;

impl Plugin for CraftingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(RecipeRegistry::new());
    }
}
```

```rust
// src/crafting/mod.rs (update)

pub mod plugin;
pub mod recipe;
pub mod registry;

pub use plugin::CraftingPlugin;
pub use recipe::*;
pub use registry::*;
```

```rust
// src/main.rs (add)

mod crafting;

// In add_plugins:
.add_plugins(crafting::CraftingPlugin)
```

**Step 2: Run to verify it compiles**

Run: `cargo build 2>&1`
Expected: SUCCESS

**Step 3: Commit**

```bash
git add src/crafting/plugin.rs src/crafting/mod.rs src/main.rs
git commit -m "feat(crafting): add CraftingPlugin"
```

---

## Phase 9: Final Integration

### Task 9.1: Run Full Test Suite

**Step 1: Run all tests**

Run: `cargo test 2>&1`
Expected: All PASS

**Step 2: Run clippy**

Run: `cargo clippy 2>&1`
Expected: No errors (warnings OK)

**Step 3: Build release**

Run: `cargo build --release 2>&1`
Expected: SUCCESS

---

### Task 9.2: Create Summary Commit

```bash
git add -A
git commit -m "feat: implement inventory, drops, magnetism, and crafting systems

- Item registry with ItemDef, DropDef, ItemId
- Dropped item entities with physics (shoot + gravity + bounce)
- Item magnetism (passive pull within 3 tiles)
- Auto-pickup on contact
- Inventory with dynamic slots and stacking
- Hotbar with L/R hand slots
- Equipment system with 15 slots
- Crafting with recipes and unlock conditions
- Block drops spawn on tile break"
```

---

## Summary

This implementation plan covers:

| Phase | Tasks | Description |
|-------|-------|-------------|
| 1 | 1.1-1.4 | Item Registry (ItemDef, DropDef, ItemRegistry) |
| 2 | 2.1-2.3 | Dropped Item Entity (components, spawn, physics) |
| 3 | 3.1-3.3 | Inventory Core (slots, stacking, plugin) |
| 4 | 4.1-4.3 | Magnetism + Pickup (systems, player integration) |
| 5 | 5.1 | Block Drop Integration |
| 6 | 6.1-6.2 | Hotbar (component, input system) |
| 7 | 7.1 | Equipment (component) |
| 8 | 8.1-8.3 | Crafting (recipes, registry, plugin) |
| 9 | 9.1-9.2 | Final Integration |

**Total: ~20 bite-sized tasks**

Each task follows TDD: Write test → Verify fail → Implement → Verify pass → Commit.
