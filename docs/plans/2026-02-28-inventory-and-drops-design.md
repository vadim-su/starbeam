# Inventory, Block Drops, and Item Pickup System Design

**Date:** 2026-02-28
**Status:** Approved
**Related:** Data-driven Registry Design

## Overview

Implementation of a Starbound-style inventory system with block drops, item magnetism, and auto-pickup mechanics.

## Architecture

### New Modules

```
src/
├── item/                    # NEW: Items and drops
│   ├── mod.rs
│   ├── definition.rs        # ItemDef, DropDef
│   ├── registry.rs          # ItemRegistry
│   └── dropped_item.rs      # DroppedItem entity + physics
│
├── inventory/               # NEW: Player inventory
│   ├── mod.rs
│   ├── components.rs        # Inventory, Equipment, Hotbar
│   ├── systems.rs           # Pickup, magnetism, stacking
│   └── ui.rs                # Inventory window (bevy_egui)
│
├── crafting/                # NEW: Crafting system
│   ├── mod.rs
│   ├── recipe.rs            # Recipe definition
│   ├── registry.rs          # RecipeRegistry
│   └── station.rs           # CraftingStation component
│
└── registry/
    └── tile.rs              # EXTEND: Add drops to TileDef
```

### Integration Points

- `src/interaction/block_action.rs` — spawn drops on block break
- `src/player/` — add Inventory component
- `src/ui/` — extend for inventory UI

---

## Item Registry

### Item Definition

```rust
#[derive(Clone, Debug)]
pub struct ItemDef {
    pub id: String,                    // "dirt", "iron_ore", "wooden_sword"
    pub display_name: String,          // "Dirt Block"
    pub description: String,           // "A block of dirt"
    pub max_stack: u16,                // 999 for blocks, 1 for weapons
    pub rarity: Rarity,                // Common, Uncommon, Rare, Legendary
    pub item_type: ItemType,           // Block, Resource, Tool, Weapon, Armor, Consumable
    pub icon: String,                  // sprite path
    pub placeable: Option<String>,     // Some("dirt") if can be placed as block
    pub equipment_slot: Option<EquipmentSlot>, // Some(Head) if equippable
    pub stats: Option<ItemStats>,      // Damage, defense, etc.
}

#[derive(Clone, Debug)]
pub struct DropDef {
    pub item_id: String,
    pub min: u16,
    pub max: u16,
    pub chance: f32,                   // 1.0 = 100%
}
```

### TileDef Extension

```rust
pub struct TileDef {
    // ... existing fields ...
    pub drops: Vec<DropDef>,           // What drops when broken
}
```

### Example Registry (RON)

```ron
(
    items: [
        (
            id: "dirt",
            display_name: "Dirt Block",
            description: "A block of dirt. How exciting!",
            max_stack: 999,
            rarity: Common,
            item_type: Block,
            icon: "items/dirt.png",
            placeable: Some("dirt"),
        ),
        (
            id: "iron_ore",
            display_name: "Iron Ore",
            description: "Raw iron ore. Needs smelting.",
            max_stack: 500,
            rarity: Uncommon,
            item_type: Resource,
            icon: "items/iron_ore.png",
        ),
    ],
)
```

---

## Dropped Item Entity

### Components

```rust
#[derive(Component)]
pub struct DroppedItem {
    pub item_id: String,
    pub count: u16,
    pub velocity: Vec2,
    pub lifetime: Timer,              // ~5 minutes before despawn
    pub magnetized: bool,             // Activates when player is near
}

#[derive(Component)]
pub struct DroppedItemPhysics {
    pub gravity: f32,                 // 400.0
    pub friction: f32,                // 0.9
    pub bounce: f32,                  // 0.3
}
```

### Spawn Behavior (Starbound-style)

1. Calculate random angle (60°-150°) for upward trajectory
2. Calculate random speed (80-150)
3. Apply velocity in that direction
4. Gravity pulls down
5. Bounce on ground contact with velocity loss
6. After ~1 second, item settles and lies still

```rust
fn spawn_dropped_item(
    commands: &mut Commands,
    item_id: &str,
    count: u16,
    position: Vec2,
) {
    let angle = rand::thread_rng().gen_range(0.6..2.5);
    let speed = rand::thread_rng().gen_range(80.0..150.0);
    let velocity = Vec2::new(angle.cos(), angle.sin()) * speed;
    
    commands.spawn((
        DroppedItem { item_id: item_id.to_string(), count, velocity, ... },
        DroppedItemPhysics { gravity: 400.0, friction: 0.9, bounce: 0.3 },
        Transform::from_translation(position.extend(1.0)),
        SpriteBundle { ... },
    ));
}
```

---

## Item Magnetism and Pickup

### Configuration

```rust
#[derive(Resource)]
pub struct PickupConfig {
    pub magnet_radius: f32,           // 48.0 (3 tiles × 16px)
    pub magnet_strength: f32,         // 200.0 (acceleration toward player)
    pub pickup_radius: f32,           // 16.0 (1 tile)
}
```

### Behavior

| Distance | Action |
|----------|--------|
| > 3 tiles | Item lies still |
| 1-3 tiles | Item accelerates toward player (strength increases closer to player) |
| < 1 tile | Instant pickup into inventory |

### System

```rust
fn item_magnetism_system(
    config: Res<PickupConfig>,
    player_query: Query<&Transform, With<Player>>,
    mut item_query: Query<(&Transform, &mut DroppedItem, &mut Velocity)>,
) {
    let player_pos = player_query.single().translation.truncate();
    
    for (item_transform, mut item, mut velocity) in &mut item_query {
        let item_pos = item_transform.translation.truncate();
        let distance = player_pos.distance(item_pos);
        
        // Activate magnet
        if distance < config.magnet_radius {
            item.magnetized = true;
        }
        
        // Pull toward player
        if item.magnetized {
            let direction = (player_pos - item_pos).normalize_or_zero();
            let strength = config.magnet_strength * (1.0 - distance / config.magnet_radius);
            velocity.x += direction.x * strength * delta;
            velocity.y += direction.y * strength * delta;
        }
        
        // Pickup on contact
        if distance < config.pickup_radius {
            // Send pickup event → handled by inventory_system
        }
    }
}
```

**Note:** Magnetism works through walls (as in Starbound).

---

## Inventory System

### Components

```rust
#[derive(Component)]
pub struct Inventory {
    pub main_bag: Vec<Option<InventorySlot>>,    // Dynamic, starts at 40 slots
    pub material_bag: Vec<Option<InventorySlot>>, // 40 slots for blocks
    pub max_slots_base: usize,                    // Base limit (40)
    pub max_slots_bonus: usize,                    // Bonus from backpacks
}

#[derive(Clone, Debug)]
pub struct InventorySlot {
    pub item_id: String,
    pub count: u16,
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
    
    // Try to add item (returns remainder if doesn't fit)
    pub fn try_add_item(&mut self, item_id: &str, count: u16, registry: &ItemRegistry) -> u16;
}
```

### Stacking Logic

```rust
fn try_stack_item(
    slots: &mut [Option<InventorySlot>],
    item_id: &str,
    count: u16,
    max_stack: u16,
) -> u16 {
    let mut remaining = count;
    
    // First, stack into existing slots
    for slot in slots.iter_mut() {
        if let Some(s) = slot {
            if s.item_id == item_id && s.count < max_stack {
                let can_add = max_stack - s.count;
                let to_add = remaining.min(can_add);
                s.count += to_add;
                remaining -= to_add;
                if remaining == 0 { return 0; }
            }
        }
    }
    
    // Then, create new slots if space available
    // ...
    
    remaining
}
```

### Extensibility

- `max_slots_bonus` increases with backpacks/upgrades
- Slots dynamically expand: `main_bag.resize(new_size, None)`

---

## Equipment System

### Equipment Slots

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum EquipmentSlot {
    // Functional slots
    Head,           // Helmet
    Chest,          // Armor
    Legs,           // Leggings
    Back,           // Cape/wings (functional)
    Accessory1,      // Ring/amulet
    Accessory2,
    Accessory3,
    Accessory4,
    Weapon1,        // Right hand
    Weapon2,        // Left hand
    Pet,            // Pet
    
    // Cosmetic slots (visual only)
    CosmeticHead,
    CosmeticChest,
    CosmeticLegs,
    CosmeticBack,
}

#[derive(Component)]
pub struct Equipment {
    pub slots: HashMap<EquipmentSlot, Option<EquippedItem>>,
}
```

### Equipment Logic

| Action | Result |
|--------|--------|
| Drag armor to Head/Chest/Legs | Stats applied + sprite changed |
| Drag to CosmeticHead | Visual only, no stats |
| Drag weapon to Weapon1/2 | Active weapon for combat |
| Drag to Accessory | Bonuses applied (e.g., +10% speed) |

**Visual:** Cosmetic slot overrides functional slot appearance. If cosmetic is empty, functional armor is shown.

---

## Hotbar System

### Design (Starbound-style)

```
┌──┬──┐ ┌──┬──┐ ┌──┬──┐ ┌──┬──┐ ┌──┬──┐ ┌──┬──┐
│L │R │ │L │R │ │L │R │ │L │R │ │L │R │ │L │R │
└──┴──┘ └──┴──┘ └──┴──┘ └──┴──┘ └──┴──┘ └──┴──┘
      ┌────┬────┬────┬────┐
      │ MM │Wire│Paint│Scan│   [🔒] [X]
      └────┴────┴────┴────┘
[1] [2] [3] [4] [5] [6]    Pixels: 1234
```

### Features

| Element | Description |
|---------|-------------|
| L/R slots | Left and right hand in each slot |
| Fixed tools | Matter Manipulator, Wire, Paint, Scan in center |
| X key | Switch between slot sets |
| Lock | Prevent accidental hotbar changes |
| Pixels | Currency shown next to hotbar |

### Component

```rust
#[derive(Component)]
pub struct Hotbar {
    pub slots: [HotbarSlot; 6],
    pub active_slot: usize,
    pub locked: bool,
    pub active_set: usize,  // 0 or 1 (X to toggle)
}

pub struct HotbarSlot {
    pub left_hand: Option<String>,   // Item ID
    pub right_hand: Option<String>,  // Item ID
}
```

---

## Crafting System

### Recipe Definition

```rust
#[derive(Clone, Debug)]
pub struct Recipe {
    pub id: String,
    pub result: RecipeResult,
    pub ingredients: Vec<Ingredient>,
    pub craft_time: f32,              // Seconds
    pub station: Option<String>,      // None = hand crafting
    pub unlocked_by: UnlockCondition,
}

#[derive(Clone, Debug)]
pub enum UnlockCondition {
    Always,                           // Available immediately
    PickupItem(String),               // On item pickup
    Blueprint(String),                // On blueprint use
    Station(String),                  // When station is present
}
```

### Example Recipes (RON)

```ron
(
    recipes: [
        // Hand crafting
        (
            id: "torch",
            result: (item_id: "torch", count: 4),
            ingredients: [(item_id: "coal", count: 1), (item_id: "wood", count: 1)],
            craft_time: 0.5,
            station: None,
            unlocked_by: Always,
        ),
        // Station crafting
        (
            id: "iron_ingot",
            result: (item_id: "iron_ingot", count: 1),
            ingredients: [(item_id: "iron_ore", count: 2)],
            craft_time: 2.0,
            station: Some("furnace"),
            unlocked_by: Always,
        ),
        // Unlock on pickup
        (
            id: "stone_sword",
            result: (item_id: "stone_sword", count: 1),
            ingredients: [(item_id: "stone", count: 2), (item_id: "wood", count: 1)],
            craft_time: 1.0,
            station: Some("workbench"),
            unlocked_by: PickupItem("stone"),
        ),
    ],
)
```

### Crafting State

```rust
#[derive(Resource)]
pub struct CraftingState {
    pub available_recipes: HashSet<String>,    // Unlocked recipes
    pub current_station: Option<String>,        // Current station (None = hands)
}
```

---

## UI Layout

### Hotbar (Always visible, bottom of screen)

- 6 slots with L/R hand separation
- 4 fixed tool slots in center
- Lock button
- Pixels display
- Key hints (1-6, X)

### Inventory Window (I key)

```
┌─────────────────────────────────────────────────────────────────────────┐
│  [I] Inventory                                                    [X]   │
├─────────────────────────────────────────────────────────────────────────┤
│                                         │                               │
│  ┌─── EQUIPMENT ───┐                   │   ┌─── MAIN BAG ─────┐        │
│  │ [Head]  [CHead] │                   │   │[•][•][•][•][•]   │        │
│  │ [Chest] [CChest]│                   │   │[•][•][•][•][•]   │        │
│  │ [Legs]  [CLegs] │                   │   │ ... 40 slots     │        │
│  │ [Back]  [CBack] │                   │   └──────────────────┘        │
│  │ [Acc1][Acc2]    │                   │                               │
│  │ [Acc3][Acc4]    │                   │   ┌─── MATERIAL BAG ──┐       │
│  │ [Wpn1][Wpn2]    │                   │   │[•][•][•][•][•]    │       │
│  │ [Pet]           │                   │   │ ... 40 slots      │       │
│  └─────────────────┘                   │   └──────────────────┘       │
│                                        │                               │
│  ┌─── STATS ───────┐                  │   ┌─── CRAFTING ──────┐       │
│  │ HP:    100/100  │                  │   │ [🔍 Search]       │       │
│  │ Def:   15       │                  │   │ [☐] Materials     │       │
│  │ Atk:   25       │                  │   │ ────────────────  │       │
│  │ Speed: 150      │                  │   │ > Torch (4)       │       │
│  └─────────────────┘                  │   │ > Campfire       │       │
│                                        │   │ ────────────────  │       │
│                                        │   │ [Craft ×1]        │       │
│                                        │   └──────────────────┘       │
│                                        │                               │
│  ←────── LEFT ────────→               │   ←────── RIGHT ───────→      │
└─────────────────────────────────────────────────────────────────────────┘
```

### UI Interactions

| Action | Result |
|--------|--------|
| Click slot | Select, show item info |
| Drag | Move between slots |
| Shift+click | Quick move to other bag |
| Right click | Use/eat/equip |
| Scroll | Change hotbar slot |
| Keys 1-6 | Select hotbar slot |

### Crafting Panel

- Search by name
- "Materials" filter — show only craftable
- Progress bar during crafting
- Click recipe → show ingredients

---

## Integration with Existing Code

### Block Breaking (block_action.rs)

```rust
// BEFORE:
world_map.set_tile(chunk_coord, tile_pos, TileId::AIR, layer);

// AFTER:
let old_tile = world_map.get_tile(chunk_coord, tile_pos, layer);
world_map.set_tile(chunk_coord, tile_pos, TileId::AIR, layer);

// Spawn drops
if let Some(tile_def) = tile_registry.get(&old_tile) {
    for drop in &tile_def.drops {
        if rand::thread_rng().gen::<f32>() < drop.chance {
            let count = rand::thread_rng().gen_range(drop.min..=drop.max);
            spawn_dropped_item(
                &mut commands,
                &item_registry,
                &drop.item_id,
                count,
                tile_world_pos,
            );
        }
    }
}
```

### Player Plugin

```rust
impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PickupConfig::default())
            .add_systems(Startup, spawn_player
                // Add Inventory::new() and Equipment::new()
            )
            .add_systems(Update, (
                item_magnetism_system,
                item_pickup_system,
            ).chain().after(Physics));
    }
}
```

### Main.rs

```rust
App::new()
    // ... existing plugins ...
    .add_plugins(InventoryPlugin)    // NEW
    .add_plugins(ItemPlugin)          // NEW
    .add_plugins(CraftingPlugin)      // NEW
```

---

## Implementation Phases

| Phase | What | Est. Time |
|-------|------|-----------|
| 1. Dropped Items | Entity, physics (shoot + gravity) | 2-3 days |
| 2. Item Registry | ItemDef, DropDef, registry | 1-2 days |
| 3. Inventory Core | Hotbar + bags, stacking, drag | 3-4 days |
| 4. Magnetism + Pickup | Magnet, auto-pickup | 1-2 days |
| 5. Equipment Slots | Equipment, cosmetics | 2-3 days |
| 6. Crafting | Basic + stations | 3-4 days |
| 7. UI | Full inventory window | 4-5 days |

**Total:** ~16-23 days

---

## Summary

This design implements a Starbound-style inventory system with:

1. **Block Drops** — Items shoot out when blocks break, fall with gravity, bounce
2. **Item Magnetism** — Passive magnet pulls items within 3 tiles
3. **Auto-Pickup** — Items collected on contact
4. **Dynamic Inventory** — Expandable bags with configurable stack limits
5. **Equipment System** — Full Starbound-style slots with cosmetics
6. **Crafting** — Hand crafting + station-based recipes
7. **Hotbar** — L/R hand slots, fixed tools, lock feature
