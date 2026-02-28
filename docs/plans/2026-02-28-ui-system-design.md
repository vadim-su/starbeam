# UI System Design

**Date:** 2026-02-28
**Status:** Approved
**Scope:** Hotbar, Inventory, Equipment, Tooltip, Drag & Drop

## Overview

Implement a Starbound-style game UI using Bevy UI with pixel-art styling. UI layout and theme are configured via a single RON file for easy tweaking without code changes.

## Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| UI Framework | Bevy UI | Pixel-art native look, nearest-neighbor sampling, authentic Starbound feel |
| RON Scope | Themes + Layout | Colors, sizes, slot counts, anchor positions — flexible without complexity |
| Layout | Starbound-style | Single screen with zones: Equipment (left), Inventory (right), Hotbar (always visible bottom) |
| RON Files | Single `ui.ron` | Simpler for MVP, can split later if needed |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      assets/ui.ron                          │
│  (themes, layout parameters: colors, sizes, positions)      │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                   src/ui/game_ui/                           │
│                                                             │
│  UiTheme (resource) ← loaded from RON                       │
│  UiState (resource) ← which panel open, drag state          │
│                                                             │
│  Spawn systems:                                              │
│  - spawn_hotbar()    → creates hotbar entities              │
│  - spawn_inventory() → creates inventory entities           │
│  - spawn_equipment() → creates equipment entities           │
│                                                             │
│  Update systems:                                             │
│  - update_slot_contents() → sync with Inventory              │
│  - handle_drag_drop()    → item drag & drop                 │
│  - show_tooltip()        → show item description            │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                   Existing components                        │
│  Inventory, Hotbar, Equipment, ItemRegistry                  │
└─────────────────────────────────────────────────────────────┘
```

## RON Configuration

```ron
// assets/ui.ron
(
    // Common settings
    base_path: "assets/textures/ui/",
    font_size: 12,
    
    // Colors (pixel-art palette)
    colors: (
        bg_dark: "#1a1410",
        bg_medium: "#2a2420",
        border: "#5a4a3a",
        border_highlight: "#8a7a6a",
        selected: "#ffcc00",
        text: "#e0d0c0",
        text_dim: "#8a7a6a",
        rarity_common: "#aaaaaa",
        rarity_uncommon: "#55ff55",
        rarity_rare: "#5555ff",
        rarity_legendary: "#ffaa00",
    ),
    
    // Hotbar (always visible at bottom)
    hotbar: (
        slots: 6,
        slot_size: 48,
        gap: 4,
        anchor: BottomCenter,
        margin_bottom: 16,
        border_width: 2,
        corner_radius: 0,  // pixel-art = no rounding
    ),
    
    // Inventory screen (toggled by key)
    inventory_screen: (
        anchor: Center,
        width: 400,
        height: 320,
        padding: 16,
        
        // Left column - Equipment
        equipment: (
            slot_size: 40,
            gap: 4,
        ),
        
        // Right column - main bag
        main_bag: (
            columns: 8,
            rows: 5,  // 40 slots
            slot_size: 32,
            gap: 2,
        ),
        
        // Material bag (below main)
        material_bag: (
            columns: 8,
            rows: 2,  // 16 slots
            slot_size: 32,
            gap: 2,
        ),
    ),
    
    // Tooltip
    tooltip: (
        padding: 8,
        max_width: 200,
        border_width: 1,
    ),
)
```

## Module Structure

```
src/ui/
├── mod.rs              // UiPlugin, exports
├── debug_panel.rs      // existing (keep)
├── game_ui/
│   ├── mod.rs          // GameUiPlugin
│   ├── theme.rs        // UiTheme, RON loading
│   ├── components.rs   // UiSlot, UiTooltip, DragState
│   ├── hotbar.rs       // spawn + update hotbar
│   ├── inventory.rs    // spawn + update inventory screen
│   ├── equipment.rs    // spawn + update equipment slots
│   ├── tooltip.rs      // show/hide tooltip
│   └── drag_drop.rs    // drag & drop logic
```

## Key Components

```rust
// src/ui/game_ui/components.rs

/// Marker for UI slot (bound to position in Inventory/Hotbar/Equipment)
#[derive(Component)]
pub struct UiSlot {
    pub slot_type: SlotType,
    pub index: usize,
}

#[derive(Clone, Copy)]
pub enum SlotType {
    Hotbar(Hand),        // HotbarSlot + L/R hand
    MainBag,             // main_bag 0..39
    MaterialBag,         // material_bag 0..15
    Equipment(EquipSlot), // equipment slot
}

#[derive(Clone, Copy)]
pub enum Hand { Left, Right }

#[derive(Clone, Copy)]
pub enum EquipSlot {
    Head, Chest, Legs, Back,
    HeadCosmetic, ChestCosmetic, LegsCosmetic, BackCosmetic,
}

/// Drag & drop state
#[derive(Resource, Default)]
pub struct DragState {
    pub dragging: Option<DragInfo>,
    pub source: Option<(SlotType, usize)>,
}

pub struct DragInfo {
    pub item_id: String,
    pub count: u16,
    pub icon_entity: Entity,  // visual sprite during drag
}

/// Tooltip (spawned on hover)
#[derive(Component)]
pub struct UiTooltip {
    pub item_id: String,
}
```

## Data Flow

```
┌──────────────┐     update_slot_contents     ┌─────────────┐
│  Inventory   │ ───────────────────────────▶ │  UI Slots   │
│  Hotbar      │                              │  (entities) │
│  Equipment   │ ◀─────────────────────────── │             │
└──────────────┘       drag_drop_apply        └─────────────┘
```

## Systems (in Update, GameSet::Ui)

1. **spawn_game_ui()** — Spawn hotbar + inventory_screen entities once on game start
2. **toggle_inventory_screen()** — Handle E/I key, toggle inventory visibility
3. **update_slot_contents()** — Read Inventory/Hotbar/Equipment, update UiSlot visuals
4. **handle_slot_interaction()** — Detect hover, show tooltip, detect click start drag
5. **handle_drag_drop()** — Track cursor, find target slot, apply swap/stack on release
6. **update_tooltip_position()** — Tooltip follows cursor

## Edge Cases

| Situation | Solution |
|-----------|----------|
| Drag from empty slot | Ignore, do nothing |
| Drop on same slot | Cancel drag, no change |
| Drop on occupied slot | Swap items |
| Drop on slot with same item_id | Stack to max_stack, remainder returns |
| Drop on incompatible slot (sword in head) | Cancel, return to source |
| Inventory overflow on pickup | Item stays on ground |
| RON file missing/invalid | Panic at startup (fail-fast) + error log |

## Drag & Drop State Machine

```
         ┌───────────────┐
         │    Idle       │
         └───────┬───────┘
                 │ click on filled slot
                 ▼
         ┌───────────────┐
         │  Dragging     │  ← cursor holds icon
         └───────┬───────┘
                 │ release
                 ▼
    ┌────────────┴────────────┐
    │                         │
    ▼                         ▼
┌─────────┐            ┌─────────────┐
│  Apply  │ (valid)    │  Cancel     │ (invalid/empty)
│  swap/  │            │  return to  │
│  stack  │            │  source     │
└─────────┘            └─────────────┘
    │                         │
    └────────────┬────────────┘
                 ▼
         ┌───────────────┐
         │    Idle       │
         └───────────────┘
```

## Dependencies

- Bevy 0.18 UI (`bevy::ui`)
- Existing: `Inventory`, `Hotbar`, `Equipment`, `ItemRegistry`
- RON crate (already in dependencies)
- Pixel-art UI textures (to be created in `assets/textures/ui/`)

## Next Steps

1. Create implementation plan with task breakdown
2. Implement UiTheme + RON loading
3. Implement Hotbar UI (simplest, visible always)
4. Implement Inventory screen
5. Implement Equipment panel
6. Implement Tooltip
7. Implement Drag & Drop
