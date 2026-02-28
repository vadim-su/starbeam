# Slot Icons and Item Counts Design

**Date:** 2026-03-01
**Status:** Approved
**Author:** User + OpenAgent

## Overview

Add visual item icons with frames and quantity display to inventory and hotbar slots, following Starbound's UI style.

## Goals

1. Display item textures (blocks) inside inventory/hotbar slots
2. Show white rounded frame around each item icon
3. Display item count in bottom-right corner of each slot
4. Apply to both hotbar and inventory UI

## Non-Goals

- Rarity-colored frames (future enhancement)
- 3D/isometric item previews
- Drag-and-drop visual changes

## Architecture

### New Components

```rust
// Marker for item icon in slot
#[derive(Component)]
struct ItemIcon;

// Marker for slot frame
#[derive(Component)]
struct SlotFrame;

// Marker for count text
#[derive(Component)]
struct ItemCount;
```

### Data Model Changes

**HotbarSlot** — add quantity support:

```rust
// Before:
pub struct HotbarSlot {
    pub left_hand: Option<String>,
    pub right_hand: Option<String>,
}

// After:
pub struct HotbarSlot {
    pub left_hand: Option<Stack>,
    pub right_hand: Option<Stack>,
}

#[derive(Clone, Debug, Default)]
pub struct Stack {
    pub item_id: String,
    pub count: u16,
}
```

### UI Slot Structure

```
UiSlot (container)
├── ItemIcon (ImageNode — item texture)
├── SlotFrame (ImageNode — frame PNG)
└── ItemCount (Text — quantity, bottom-right)
```

### New Resources

```rust
#[derive(Resource)]
struct SlotFrames {
    common: Handle<Image>,      // White frame (current)
    // Future: uncommon, rare, legendary
}
```

## Data Flow

### Asset Loading

```
Startup
    │
    ▼
Load slot_frame.png → SlotFrames resource
Load terrain/*.png → Assets<Image> (existing)
```

### UI Update Loop

```
Update (every frame)
    │
    ▼
update_slot_icons system
    │
    ├─► Read: Inventory, Hotbar, ItemRegistry
    │
    ├─► For each UiSlot:
    │       │
    │       ├─► Get item_id + count from data
    │       │
    │       ├─► If has item:
    │       │       ├─► Show ItemIcon (texture from ItemDef.icon)
    │       │       ├─► Show SlotFrame
    │       │       └─► Update ItemCount text
    │       │
    │       └─► If empty:
    │               ├─► Hide ItemIcon
    │               ├─► Hide SlotFrame
    │               └─► Clear ItemCount
    │
    ▼
UI reflects inventory/hotbar state
```

## Implementation Details

### Frame Texture

**File:** `assets/ui/slot_frame.png`
- Size: 32×32 px (matching slot size)
- Format: PNG with transparency
- Style: white rounded border, transparent center

### Slot Layout

```
┌─────────────────┐
│ [1]             │  ← slot number (existing)
│                 │
│    ┌─────┐      │  ← frame (center)
│    │ dirt│      │  ← icon (center)
│    └─────┘      │
│            [50] │  ← count (bottom-right)
└─────────────────┘
```

### Files to Modify

| File | Change |
|------|--------|
| `src/inventory/hotbar.rs` | Add `Stack`, modify `HotbarSlot` |
| `src/inventory/components.rs` | Add `Stack` struct |
| `src/ui/game_ui/hotbar.rs` | Add children: `ItemIcon`, `SlotFrame`, `ItemCount` |
| `src/ui/game_ui/inventory.rs` | Same for inventory slots |
| `src/ui/game_ui/mod.rs` | Add `update_slot_icons` system |
| `src/ui/game_ui/slot_sync.rs` | Update sync logic |
| `assets/ui/slot_frame.png` | New file |

## Design Decisions

### Why UI Layers (Approach 1)

- Matches Starbound's implementation
- Simple with standard Bevy UI
- Flexible for future features (rarity frames, effects)
- No runtime texture generation needed

### Why Flat Icons (not 3D)

- Starbound uses flat textures
- Simpler implementation
- Consistent with existing terrain textures

## Future Enhancements

1. **Rarity-colored frames** — different frame colors based on item rarity
2. **Animated frames** — glow effects for rare items
3. **Stack splitting** — right-click to split stacks
4. **Item tooltips** — hover info with stats
