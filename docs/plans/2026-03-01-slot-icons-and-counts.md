# Slot Icons and Item Counts Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Display item textures with white rounded frames and quantity counts in inventory and hotbar slots.

**Architecture:** UI layers approach — each slot contains icon image, frame image, and count text as separate children. Use existing terrain textures for block icons.

**Tech Stack:** Bevy 0.15, Rust, UI with ImageNode and Text

---

## Task 1: Add Stack struct for item quantities

**Files:**
- Modify: `src/inventory/components.rs`

**Step 1: Add Stack struct**

Add after `InventorySlot` definition (around line 8):

```rust
/// A stack of items with ID and count.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stack {
    pub item_id: String,
    pub count: u16,
}
```

**Step 2: Run tests**

Run: `cargo test --lib inventory::components`
Expected: All tests pass

**Step 3: Commit**

```bash
git add src/inventory/components.rs
git commit -m "feat(inventory): add Stack struct for item quantities"
```

---

## Task 2: Update HotbarSlot to use Stack

**Files:**
- Modify: `src/inventory/hotbar.rs`

**Step 1: Import Stack and update HotbarSlot**

Replace the `HotbarSlot` struct (lines 4-8) with:

```rust
use bevy::prelude::*;

use super::components::Stack;

/// A single hotbar slot with left/right hand items.
#[derive(Clone, Debug, Default)]
pub struct HotbarSlot {
    pub left_hand: Option<Stack>,
    pub right_hand: Option<Stack>,
}
```

**Step 2: Update get_item_for_hand method**

Replace the `get_item_for_hand` method (lines 35-42) with:

```rust
    pub fn get_item_for_hand(&self, is_left: bool) -> Option<&Stack> {
        let slot = self.active_slot();
        if is_left {
            slot.left_hand.as_ref()
        } else {
            slot.right_hand.as_ref()
        }
    }
```

**Step 3: Run tests**

Run: `cargo test --lib inventory::hotbar`
Expected: All tests pass

**Step 4: Commit**

```bash
git add src/inventory/hotbar.rs
git commit -m "feat(hotbar): update HotbarSlot to use Stack with quantities"
```

---

## Task 3: Add UI marker components

**Files:**
- Modify: `src/ui/game_ui/components.rs`

**Step 1: Add marker components**

Add after `SlotLabel` component (around line 81):

```rust
/// Marker for the item icon image inside a slot.
#[derive(Component)]
pub struct ItemIcon;

/// Marker for the slot frame image.
#[derive(Component)]
pub struct SlotFrame;

/// Marker for the item count text.
#[derive(Component)]
pub struct ItemCount;
```

**Step 2: Commit**

```bash
git add src/ui/game_ui/components.rs
git commit -m "feat(ui): add ItemIcon, SlotFrame, ItemCount marker components"
```

---

## Task 4: Create slot frame texture

**Files:**
- Create: `assets/ui/slot_frame.png`

**Step 1: Create assets/ui directory**

Run: `mkdir -p assets/ui`

**Step 2: Create a simple white rounded frame PNG**

This requires creating a 32x32 PNG with:
- White rounded border (2-3px width)
- Transparent center
- Rounded corners (radius ~4px)

For now, create a placeholder using ImageMagick:

```bash
convert -size 32x32 xc:none \
  -fill none -stroke white -strokewidth 2 \
  -draw "roundrectangle 2,2 29,29 4,4" \
  assets/ui/slot_frame.png
```

If ImageMagick is not available, note in commit that a proper frame needs to be created.

**Step 3: Verify file exists**

Run: `ls -la assets/ui/slot_frame.png`

**Step 4: Commit**

```bash
git add assets/ui/slot_frame.png
git commit -m "asset(ui): add slot frame texture placeholder"
```

---

## Task 5: Add SlotFrames resource and loading

**Files:**
- Modify: `src/ui/game_ui/mod.rs`

**Step 1: Add SlotFrames resource**

Add after imports (around line 9):

```rust
use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
```

Add resource struct before `GameUiPlugin`:

```rust
/// Handles for slot frame textures.
#[derive(Resource)]
pub struct SlotFrames {
    pub common: Handle<Image>,
}

impl SlotFrames {
    /// Create with a generated white frame.
    pub fn new(images: &mut Assets<Image>) -> Self {
        let frame = Self::generate_frame();
        Self {
            common: images.add(frame),
        }
    }

    /// Generate a 32x32 white rounded frame.
    fn generate_frame() -> Image {
        let size = 32u32;
        let mut data = vec![0u8; (size * size * 4) as usize];
        
        // Draw rounded rectangle border
        let border = 2u32;
        let radius = 4u32;
        
        for y in 0..size {
            for x in 0..size {
                // Distance from edges
                let dx = if x < size / 2 { x } else { size - 1 - x };
                let dy = if y < size / 2 { y } else { size - 1 - y };
                
                // Check if in corner
                let in_corner = dx < radius && dy < radius;
                let corner_dist = ((dx as f32 - radius as f32).powi(2) + (dy as f32 - radius as f32).powi(2)).sqrt();
                
                // Check if on border
                let on_border = if in_corner {
                    corner_dist <= radius as f32 && corner_dist >= (radius - border) as f32
                } else {
                    dx < border || dy < border
                };
                
                if on_border {
                    let idx = ((y * size + x) * 4) as usize;
                    data[idx] = 255;     // R
                    data[idx + 1] = 255; // G
                    data[idx + 2] = 255; // B
                    data[idx + 3] = 255; // A
                }
            }
        }
        
        Image::new(
            Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        )
    }
}
```

**Step 2: Update GameUiPlugin to insert resource**

Modify `build` method to add resource before systems:

```rust
impl Plugin for GameUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DragState>()
            .init_resource::<HoveredSlot>()
            .init_resource::<InventoryScreenState>()
            .insert_resource(UiTheme::load())
            .add_systems(
                OnEnter(AppState::InGame),
                (init_slot_frames, spawn_game_ui, tooltip::spawn_tooltip).chain(),
            )
            .add_systems(
                Update,
                (
                    hotbar::update_hotbar_slots,
                    slot_sync::sync_slot_contents,
                    toggle_inventory,
                    drag_drop::update_drag_position,
                    tooltip::update_tooltip,
                ),
            );
    }
}
```

**Step 3: Add init_slot_frames system**

Add after `spawn_game_ui` function:

```rust
/// Initialize slot frame textures.
fn init_slot_frames(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(SlotFrames::new(&mut images));
}
```

**Step 4: Run build**

Run: `cargo build`
Expected: Compiles without errors

**Step 5: Commit**

```bash
git add src/ui/game_ui/mod.rs
git commit -m "feat(ui): add SlotFrames resource with generated frame texture"
```

---

## Task 6: Add ItemIconRegistry for mapping items to textures

**Files:**
- Create: `src/ui/game_ui/icon_registry.rs`

**Step 1: Create icon registry module**

Create file `src/ui/game_ui/icon_registry.rs`:

```rust
//! Maps item IDs to their icon textures.

use std::collections::HashMap;

use bevy::prelude::*;

/// Registry mapping item IDs to icon image handles.
#[derive(Resource)]
pub struct ItemIconRegistry {
    icons: HashMap<String, Handle<Image>>,
}

impl ItemIconRegistry {
    pub fn new() -> Self {
        Self {
            icons: HashMap::new(),
        }
    }

    /// Register an icon for an item.
    pub fn register(&mut self, item_id: &str, handle: Handle<Image>) {
        self.icons.insert(item_id.to_string(), handle);
    }

    /// Get icon handle for an item.
    pub fn get(&self, item_id: &str) -> Option<&Handle<Image>> {
        self.icons.get(item_id)
    }
}

impl Default for ItemIconRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 2: Add module to mod.rs**

Add to `src/ui/game_ui/mod.rs` after other module declarations:

```rust
pub mod icon_registry;
```

And export it:

```rust
pub use icon_registry::*;
```

**Step 3: Commit**

```bash
git add src/ui/game_ui/icon_registry.rs src/ui/game_ui/mod.rs
git commit -m "feat(ui): add ItemIconRegistry for item icon textures"
```

---

## Task 7: Load item icons on startup

**Files:**
- Modify: `src/ui/game_ui/mod.rs`

**Step 1: Add icon loading system**

Add after `init_slot_frames` function:

```rust
/// Load item icons from terrain textures.
fn load_item_icons(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    item_registry: Res<crate::item::ItemRegistry>,
) {
    let mut icon_registry = ItemIconRegistry::new();
    
    // Map item IDs to their icon paths
    // For blocks, use terrain textures
    let icon_paths: HashMap<&str, &str> = [
        ("dirt", "world/terrain/dirt.png"),
        ("stone", "world/terrain/stone.png"),
        ("grass", "world/terrain/grass.png"),
        ("torch", "world/terrain/dirt.png"), // Placeholder
    ].iter().cloned().collect();
    
    for i in 0..item_registry.len() {
        let id = crate::item::ItemId(i as u16);
        let def = item_registry.get(id);
        
        // Try to load from icon_paths map, fallback to terrain texture
        let path = icon_paths.get(def.id.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("world/terrain/{}.png", def.id));
        
        let handle: Handle<Image> = asset_server.load(path);
        icon_registry.register(&def.id, handle);
    }
    
    commands.insert_resource(icon_registry);
}
```

**Step 2: Update plugin to call loading system**

Modify the `OnEnter(AppState::InGame)` systems:

```rust
.add_systems(
    OnEnter(AppState::InGame),
    (init_slot_frames, load_item_icons, spawn_game_ui, tooltip::spawn_tooltip).chain(),
)
```

**Step 3: Add HashMap import**

Add at top of file:

```rust
use std::collections::HashMap;
```

**Step 4: Run build**

Run: `cargo build`
Expected: Compiles without errors

**Step 5: Commit**

```bash
git add src/ui/game_ui/mod.rs
git commit -m "feat(ui): load item icons from terrain textures on startup"
```

---

## Task 8: Update hotbar slot spawning with icon/frame/count

**Files:**
- Modify: `src/ui/game_ui/hotbar.rs`

**Step 1: Update spawn_hotbar to add children**

Replace the slot spawning section (lines 49-160) with updated version that includes icon, frame, and count:

Find the section starting with `// Slot container` and modify the `.with_children` block to add:

```rust
.with_children(|slot_parent| {
    // Left hand half
    slot_parent
        .spawn((
            UiSlot {
                slot_type: SlotType::Hotbar {
                    index: i,
                    hand: Hand::Left,
                },
            },
            Node {
                width: Val::Percent(50.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            Pickable {
                should_block_lower: false,
                is_hoverable: true,
            },
        ))
        .observe(/* ... existing observers ... */)
        .with_children(|hand_parent| {
            // Item icon
            hand_parent.spawn((
                ItemIcon,
                ImageNode::default(),
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                Visibility::Hidden,
                Pickable::IGNORE,
            ));
            // Frame
            hand_parent.spawn((
                SlotFrame,
                ImageNode::default(),
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    position_type: PositionType::Absolute,
                    ..default()
                },
                Visibility::Hidden,
                Pickable::IGNORE,
            ));
            // Count
            hand_parent.spawn((
                ItemCount,
                Text::new(""),
                TextFont {
                    font_size: 9.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(1.0),
                    right: Val::Px(2.0),
                    ..default()
                },
                Pickable::IGNORE,
            ));
        });
    
    // Right hand half (similar structure)
    // ... copy and modify for Right hand ...
    
    // Slot number label
    slot_parent.spawn((
        Text::new(format!("{}", i + 1)),
        TextFont {
            font_size: 10.0,
            ..default()
        },
        TextColor(Color::from(text_dim)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(2.0),
            left: Val::Px(2.0),
            ..default()
        },
        Pickable::IGNORE,
    ));
});
```

**Step 2: Add imports**

Add at top of file:

```rust
use super::components::{ItemCount, ItemIcon, SlotFrame};
```

**Step 3: Run build**

Run: `cargo build`
Expected: Compiles without errors

**Step 4: Commit**

```bash
git add src/ui/game_ui/hotbar.rs
git commit -m "feat(ui): add ItemIcon, SlotFrame, ItemCount to hotbar slots"
```

---

## Task 9: Update inventory slot spawning with icon/frame/count

**Files:**
- Modify: `src/ui/game_ui/inventory.rs`

**Step 1: Add imports**

Add at top of file:

```rust
use super::components::{ItemCount, ItemIcon, SlotFrame};
```

**Step 2: Update main bag slot spawning**

Find the main bag grid slot spawning (around line 145-201) and add children after the slot entity:

Replace the `.with_children` block with:

```rust
.with_children(|slot_parent| {
    // Item icon
    slot_parent.spawn((
        ItemIcon,
        ImageNode::default(),
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        Visibility::Hidden,
        Pickable::IGNORE,
    ));
    // Frame
    slot_parent.spawn((
        SlotFrame,
        ImageNode::default(),
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            ..default()
        },
        Visibility::Hidden,
        Pickable::IGNORE,
    ));
    // Count
    slot_parent.spawn((
        ItemCount,
        Text::new(""),
        TextFont {
            font_size: 9.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(1.0),
            right: Val::Px(2.0),
            ..default()
        },
        Pickable::IGNORE,
    ));
    // Existing SlotLabel for backward compatibility
    slot_parent.spawn((
        SlotLabel,
        Text::new(""),
        TextFont {
            font_size: 9.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(1.0),
            right: Val::Px(2.0),
            ..default()
        },
        Pickable::IGNORE,
    ));
})
```

**Step 3: Update material bag slot spawning similarly**

Apply the same changes to the material bag grid (around line 227-283).

**Step 4: Run build**

Run: `cargo build`
Expected: Compiles without errors

**Step 5: Commit**

```bash
git add src/ui/game_ui/inventory.rs
git commit -m "feat(ui): add ItemIcon, SlotFrame, ItemCount to inventory slots"
```

---

## Task 10: Create update_slot_icons system

**Files:**
- Modify: `src/ui/game_ui/slot_sync.rs`

**Step 1: Add imports**

Add at top of file:

```rust
use super::components::{ItemCount, ItemIcon, SlotFrame};
use super::icon_registry::ItemIconRegistry;
use super::SlotFrames;
use crate::inventory::Hotbar;
use crate::item::ItemRegistry;
use bevy::ui::widget::ImageNode;
```

**Step 2: Add update_slot_icons system**

Add after `sync_slot_contents` function:

```rust
/// Update slot icons, frames, and counts from inventory/hotbar data.
pub fn update_slot_icons(
    inventory_query: Query<&Inventory, With<Player>>,
    hotbar_query: Query<&Hotbar, With<Player>>,
    item_registry: Res<ItemRegistry>,
    icon_registry: Res<ItemIconRegistry>,
    slot_frames: Res<SlotFrames>,
    
    // Query for slots with their children
    slot_query: Query<
        &UiSlot,
        With<Children>,
    >,
    // Child queries
    mut icon_query: Query<&mut ImageNode, With<ItemIcon>>,
    mut frame_query: Query<&mut ImageNode, With<SlotFrame>>,
    mut count_query: Query<&mut Text, With<ItemCount>>,
    mut visibility_query: Query<&mut Visibility, Or<(With<ItemIcon>, With<SlotFrame>)>>,
    children_query: Query<&Children>,
) {
    let Ok(inventory) = inventory_query.single() else {
        return;
    };
    let Ok(hotbar) = hotbar_query.single() else {
        return;
    };

    for slot in &slot_query {
        // Get item data for this slot
        let item_data: Option<(&str, u16)> = match slot.slot_type {
            SlotType::MainBag(idx) => inventory.main_bag.get(idx)
                .and_then(|s| s.as_ref())
                .map(|s| (s.item_id.as_str(), s.count)),
            SlotType::MaterialBag(idx) => inventory.material_bag.get(idx)
                .and_then(|s| s.as_ref())
                .map(|s| (s.item_id.as_str(), s.count)),
            SlotType::Hotbar { index, hand } => {
                let slot_data = &hotbar.slots[index];
                match hand {
                    Hand::Left => slot_data.left_hand.as_ref(),
                    Hand::Right => slot_data.right_hand.as_ref(),
                }
                .map(|s| (s.item_id.as_str(), s.count))
            }
            SlotType::Equipment(_) => continue,
        };

        // Get children of this slot
        let Ok(children) = children_query.get(slot.entity()) else {
            continue;
        };

        // Update children based on item presence
        if let Some((item_id, count)) = item_data {
            // Show icon and frame
            for child in children.iter() {
                // Update icon
                if let Ok(mut image_node) = icon_query.get_mut(*child) {
                    if let Some(handle) = icon_registry.get(item_id) {
                        image_node.image = handle.clone();
                    }
                }
                // Update frame
                if let Ok(mut image_node) = frame_query.get_mut(*child) {
                    image_node.image = slot_frames.common.clone();
                }
                // Update count
                if let Ok(mut text) = count_query.get_mut(*child) {
                    *text = if count > 1 {
                        Text::new(format!("{}", count))
                    } else {
                        Text::new("")
                    };
                }
                // Show elements
                if let Ok(mut vis) = visibility_query.get_mut(*child) {
                    *vis = Visibility::Visible;
                }
            }
        } else {
            // Hide icon and frame
            for child in children.iter() {
                if let Ok(mut vis) = visibility_query.get_mut(*child) {
                    *vis = Visibility::Hidden;
                }
                if let Ok(mut text) = count_query.get_mut(*child) {
                    *text = Text::new("");
                }
            }
        }
    }
}
```

**Step 3: Register system in plugin**

Modify `src/ui/game_ui/mod.rs` to add the system:

```rust
.add_systems(
    Update,
    (
        hotbar::update_hotbar_slots,
        slot_sync::sync_slot_contents,
        slot_sync::update_slot_icons,
        toggle_inventory,
        drag_drop::update_drag_position,
        tooltip::update_tooltip,
    ),
)
```

**Step 4: Run build**

Run: `cargo build`
Expected: Compiles without errors

**Step 5: Commit**

```bash
git add src/ui/game_ui/slot_sync.rs src/ui/game_ui/mod.rs
git commit -m "feat(ui): add update_slot_icons system for icons, frames, counts"
```

---

## Task 11: Test and verify

**Step 1: Run all tests**

Run: `cargo test`
Expected: All tests pass

**Step 2: Run the game**

Run: `cargo run`
Expected: Game starts, hotbar shows at bottom

**Step 3: Verify visually**

- Hotbar slots should show item icons when items are present
- White frame should appear around items
- Count should show in bottom-right corner
- Empty slots should be transparent

**Step 4: Fix any issues**

If there are visual issues, debug and fix.

**Step 5: Final commit**

```bash
git add -A
git commit -m "feat(ui): complete slot icons and counts implementation"
```

---

## Summary

This plan implements:
1. `Stack` struct for item quantities
2. Updated `HotbarSlot` to use `Stack`
3. UI marker components (`ItemIcon`, `SlotFrame`, `ItemCount`)
4. Generated white rounded frame texture
5. `SlotFrames` resource
6. `ItemIconRegistry` for mapping items to textures
7. Icon loading from terrain textures
8. Updated slot spawning with icon/frame/count children
9. `update_slot_icons` system to sync UI with data
