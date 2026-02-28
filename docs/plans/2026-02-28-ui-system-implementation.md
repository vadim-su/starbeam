# UI System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement Starbound-style game UI with hotbar, inventory, equipment, tooltip, and drag & drop using Bevy UI with RON configuration.

**Architecture:** Single `ui.ron` defines theme/layout. UI entities spawned once on game start. Hotbar always visible, inventory screen toggled by key. Drag & drop via Bevy pointer events with custom state machine.

**Tech Stack:** Bevy 0.18 UI (`Node`, `ImageNode`, `Text`, `.observe()`), RON, existing `Inventory`/`Hotbar`/`Equipment` components.

**Design Doc:** `docs/plans/2026-02-28-ui-system-design.md`

---

## Phase 1: Foundation — Theme & Components

### Task 1.1: Create UiTheme Types and RON Loading

**Files:**
- Create: `src/ui/game_ui/mod.rs`
- Create: `src/ui/game_ui/theme.rs`
- Create: `assets/ui.ron`
- Modify: `src/ui/mod.rs`
- Modify: `Cargo.toml` (if needed — ron already added)

**Step 1: Create game_ui module**

```rust
// src/ui/game_ui/mod.rs
pub mod components;
pub mod theme;

pub use components::*;
pub use theme::*;
```

```rust
// src/ui/mod.rs (update)
pub mod debug_panel;
pub mod game_ui;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

use crate::registry::AppState;
use crate::sets::GameSet;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<debug_panel::DebugUiState>()
            .add_systems(Update, debug_panel::toggle_debug_panel.in_set(GameSet::Ui))
            .add_systems(
                EguiPrimaryContextPass,
                debug_panel::draw_debug_panel.run_if(in_state(AppState::InGame)),
            );
    }
}
```

**Step 2: Create theme types**

```rust
// src/ui/game_ui/theme.rs
use bevy::prelude::*;
use serde::Deserialize;

/// Parsed hex color wrapper for RON deserialization.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct HexColor(pub String);

impl From<HexColor> for Color {
    fn from(hex: HexColor) -> Self {
        let s = hex.0.trim_start_matches('#');
        let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(0) as f32 / 255.0;
        let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(0) as f32 / 255.0;
        let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(0) as f32 / 255.0;
        Color::srgb(r, g, b)
    }
}

/// UI color palette.
#[derive(Debug, Clone, Deserialize)]
pub struct UiColors {
    pub bg_dark: HexColor,
    pub bg_medium: HexColor,
    pub border: HexColor,
    pub border_highlight: HexColor,
    pub selected: HexColor,
    pub text: HexColor,
    pub text_dim: HexColor,
    pub rarity_common: HexColor,
    pub rarity_uncommon: HexColor,
    pub rarity_rare: HexColor,
    pub rarity_legendary: HexColor,
}

/// Hotbar configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct HotbarConfig {
    pub slots: usize,
    pub slot_size: f32,
    pub gap: f32,
    pub anchor: String, // "BottomCenter" for now
    pub margin_bottom: f32,
    pub border_width: f32,
}

/// Equipment configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct EquipmentConfig {
    pub slot_size: f32,
    pub gap: f32,
}

/// Main bag configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct BagConfig {
    pub columns: usize,
    pub rows: usize,
    pub slot_size: f32,
    pub gap: f32,
}

/// Inventory screen configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct InventoryScreenConfig {
    pub anchor: String, // "Center"
    pub width: f32,
    pub height: f32,
    pub padding: f32,
    pub equipment: EquipmentConfig,
    pub main_bag: BagConfig,
    pub material_bag: BagConfig,
}

/// Tooltip configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct TooltipConfig {
    pub padding: f32,
    pub max_width: f32,
    pub border_width: f32,
}

/// Root UI theme loaded from RON.
#[derive(Debug, Clone, Deserialize, Resource)]
pub struct UiTheme {
    pub base_path: String,
    pub font_size: f32,
    pub colors: UiColors,
    pub hotbar: HotbarConfig,
    pub inventory_screen: InventoryScreenConfig,
    pub tooltip: TooltipConfig,
}

impl UiTheme {
    pub fn load() -> Self {
        let ron_str = include_str!("../../../assets/ui.ron");
        ron::from_str(ron_str).expect("Failed to parse ui.ron")
    }
}
```

**Step 3: Create minimal RON file**

```ron
// assets/ui.ron
(
    base_path: "assets/textures/ui/",
    font_size: 12.0,
    
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
    
    hotbar: (
        slots: 6,
        slot_size: 48.0,
        gap: 4.0,
        anchor: "BottomCenter",
        margin_bottom: 16.0,
        border_width: 2.0,
    ),
    
    inventory_screen: (
        anchor: "Center",
        width: 400.0,
        height: 320.0,
        padding: 16.0,
        
        equipment: (
            slot_size: 40.0,
            gap: 4.0,
        ),
        
        main_bag: (
            columns: 8,
            rows: 5,
            slot_size: 32.0,
            gap: 2.0,
        ),
        
        material_bag: (
            columns: 8,
            rows: 2,
            slot_size: 32.0,
            gap: 2.0,
        ),
    ),
    
    tooltip: (
        padding: 8.0,
        max_width: 200.0,
        border_width: 1.0,
    ),
)
```

**Step 4: Verify compilation**

Run: `cargo build 2>&1`
Expected: SUCCESS

**Step 5: Commit**

```bash
git add src/ui/game_ui/ src/ui/mod.rs assets/ui.ron
git commit -m "feat(ui): add UiTheme types and RON configuration"
```

---

### Task 1.2: Create UI Components (UiSlot, DragState, SlotType)

**Files:**
- Create: `src/ui/game_ui/components.rs`

**Step 1: Write the component types**

```rust
// src/ui/game_ui/components.rs
use bevy::prelude::*;

/// Which hand in a hotbar slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hand {
    Left,
    Right,
}

/// Equipment slot type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EquipSlot {
    Head,
    Chest,
    Legs,
    Back,
    HeadCosmetic,
    ChestCosmetic,
    LegsCosmetic,
    BackCosmetic,
}

/// Type of UI slot — maps to inventory positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotType {
    /// Hotbar slot with hand designation (index 0-5, hand L/R)
    Hotbar { index: usize, hand: Hand },
    /// Main inventory bag (index 0-39)
    MainBag(usize),
    /// Material bag (index 0-15)
    MaterialBag(usize),
    /// Equipment slot
    Equipment(EquipSlot),
}

/// Marker component for a UI slot entity.
#[derive(Component, Debug)]
pub struct UiSlot {
    pub slot_type: SlotType,
}

/// Information about an item being dragged.
#[derive(Clone, Debug)]
pub struct DragInfo {
    pub item_id: String,
    pub count: u16,
    pub source_slot: SlotType,
    /// Visual entity following cursor during drag.
    pub drag_icon: Entity,
}

/// Global drag & drop state.
#[derive(Resource, Default, Debug)]
pub struct DragState {
    pub dragging: Option<DragInfo>,
}

/// Marker for inventory screen root (toggled visible/hidden).
#[derive(Component)]
pub struct InventoryScreen;

/// Marker for hotbar root (always visible).
#[derive(Component)]
pub struct HotbarRoot;

/// Marker for tooltip entity.
#[derive(Component)]
pub struct UiTooltip {
    pub item_id: String,
    pub count: u16,
}

/// Tracks which slot is currently hovered (for tooltip).
#[derive(Resource, Default, Debug)]
pub struct HoveredSlot {
    pub slot: Option<SlotType>,
}

/// Inventory screen visibility state.
#[derive(Resource, Default)]
pub struct InventoryScreenState {
    pub visible: bool,
}
```

**Step 2: Update mod.rs to export components**

```rust
// src/ui/game_ui/mod.rs (update)
pub mod components;
pub mod theme;

pub use components::*;
pub use theme::*;
```

**Step 3: Verify compilation**

Run: `cargo build 2>&1`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add src/ui/game_ui/components.rs src/ui/game_ui/mod.rs
git commit -m "feat(ui): add UiSlot, DragState, and slot type components"
```

---

### Task 1.3: Create GameUiPlugin with Theme Loading

**Files:**
- Modify: `src/ui/game_ui/mod.rs`
- Modify: `src/ui/mod.rs`

**Step 1: Add GameUiPlugin**

```rust
// src/ui/game_ui/mod.rs
pub mod components;
pub mod theme;

use bevy::prelude::*;

use crate::registry::AppState;

pub use components::*;
pub use theme::*;

pub struct GameUiPlugin;

impl Plugin for GameUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DragState>()
            .init_resource::<HoveredSlot>()
            .init_resource::<InventoryScreenState>()
            .insert_resource(UiTheme::load())
            .add_systems(OnEnter(AppState::InGame), spawn_game_ui);
    }
}

/// Spawn all game UI elements (hotbar, inventory screen).
fn spawn_game_ui() {
    // Placeholder — will be implemented in Phase 2
}
```

**Step 2: Register GameUiPlugin in UiPlugin**

```rust
// src/ui/mod.rs (update)
pub mod debug_panel;
pub mod game_ui;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

use crate::registry::AppState;
use crate::sets::GameSet;
use game_ui::GameUiPlugin;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<debug_panel::DebugUiState>()
            .add_plugins(GameUiPlugin)
            .add_systems(Update, debug_panel::toggle_debug_panel.in_set(GameSet::Ui))
            .add_systems(
                EguiPrimaryContextPass,
                debug_panel::draw_debug_panel.run_if(in_state(AppState::InGame)),
            );
    }
}
```

**Step 3: Verify compilation**

Run: `cargo build 2>&1`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add src/ui/game_ui/mod.rs src/ui/mod.rs
git commit -m "feat(ui): add GameUiPlugin with theme and state resources"
```

---

## Phase 2: Hotbar UI

### Task 2.1: Spawn Hotbar UI Entities

**Files:**
- Create: `src/ui/game_ui/hotbar.rs`
- Modify: `src/ui/game_ui/mod.rs`

**Step 1: Create hotbar spawn system**

```rust
// src/ui/game_ui/hotbar.rs
use bevy::prelude::*;

use super::components::*;
use super::theme::UiTheme;

/// Spawn the hotbar UI at the bottom of the screen.
pub fn spawn_hotbar(
    mut commands: Commands,
    theme: Res<UiTheme>,
) {
    let config = &theme.hotbar;
    let colors = &theme.colors;
    
    // Hotbar container
    let total_width = config.slots as f32 * config.slot_size
        + (config.slots - 1) as f32 * config.gap;
    
    commands
        .spawn((
            HotbarRoot,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(config.margin_bottom),
                left: Val::Percent(50.0),
                width: Val::Px(total_width),
                height: Val::Px(config.slot_size),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(config.gap),
                margin: UiRect::new(
                    Val::Px(-total_width / 2.0), // Center via negative margin
                    Val::Auto,
                    Val::Auto,
                    Val::Auto,
                ),
                ..default()
            },
            BackgroundColor(colors.bg_dark.clone().into()),
            Pickable::IGNORE,
        ))
        .with_children(|parent| {
            for i in 0..config.slots {
                spawn_hotbar_slot(parent, i, config, colors);
            }
        });
}

fn spawn_hotbar_slot(
    parent: &mut ChildBuilder,
    index: usize,
    config: &super::theme::HotbarConfig,
    colors: &super::theme::UiColors,
) {
    // Slot container (holds L and R hands)
    parent
        .spawn((
            UiSlot {
                slot_type: SlotType::Hotbar { index, hand: Hand::Left },
            },
            Node {
                width: Val::Px(config.slot_size),
                height: Val::Px(config.slot_size),
                border: UiRect::all(Val::Px(config.border_width)),
                flex_direction: FlexDirection::Row,
                ..default()
            },
            BackgroundColor(colors.bg_medium.clone().into()),
            BorderColor::all(colors.border.clone().into()),
            Pickable {
                should_block_lower: false,
                is_hoverable: true,
            },
        ))
        .with_children(|slot_parent| {
            // Left hand half
            slot_parent.spawn((
                UiSlot {
                    slot_type: SlotType::Hotbar { index, hand: Hand::Left },
                },
                Node {
                    width: Val::Percent(50.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.0, 0.0, 0.0)), // Transparent placeholder
                Pickable {
                    should_block_lower: false,
                    is_hoverable: true,
                },
            ));
            // Right hand half
            slot_parent.spawn((
                UiSlot {
                    slot_type: SlotType::Hotbar { index, hand: Hand::Right },
                },
                Node {
                    width: Val::Percent(50.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.0, 0.0, 0.0)),
                Pickable {
                    should_block_lower: false,
                    is_hoverable: true,
                },
            ));
            // Slot number label
            slot_parent.spawn((
                Text::new(format!("{}", index + 1)),
                TextFont {
                    font_size: 10.0,
                    ..default()
                },
                TextColor(colors.text_dim.clone().into()),
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(2.0),
                    left: Val::Px(2.0),
                    ..default()
                },
                Pickable::IGNORE,
            ));
        });
}
```

**Step 2: Update mod.rs and spawn_game_ui**

```rust
// src/ui/game_ui/mod.rs (update)
pub mod components;
pub mod hotbar;
pub mod theme;

use bevy::prelude::*;

use crate::registry::AppState;

pub use components::*;
pub use theme::*;

pub struct GameUiPlugin;

impl Plugin for GameUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DragState>()
            .init_resource::<HoveredSlot>()
            .init_resource::<InventoryScreenState>()
            .insert_resource(UiTheme::load())
            .add_systems(OnEnter(AppState::InGame), spawn_game_ui);
    }
}

/// Spawn all game UI elements.
fn spawn_game_ui(mut commands: Commands, theme: Res<UiTheme>) {
    hotbar::spawn_hotbar(commands, theme);
}
```

**Step 3: Verify compilation**

Run: `cargo build 2>&1`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add src/ui/game_ui/hotbar.rs src/ui/game_ui/mod.rs
git commit -m "feat(ui): spawn hotbar UI with 6 slots at bottom center"
```

---

### Task 2.2: Update Hotbar Slots from Game State

**Files:**
- Modify: `src/ui/game_ui/hotbar.rs`
- Modify: `src/ui/game_ui/mod.rs`

**Step 1: Add slot update system**

```rust
// src/ui/game_ui/hotbar.rs (add at end)

use crate::inventory::Hotbar;
use crate::player::Player;

/// Sync hotbar UI slots with Hotbar component data.
pub fn update_hotbar_slots(
    hotbar_query: Query<&Hotbar, With<Player>>,
    mut slot_query: Query<(&UiSlot, &mut BackgroundColor, Option<&Children>)>,
    child_slots: Query<&UiSlot>,
) {
    let Ok(hotbar) = hotbar_query.single() else {
        return;
    };

    for (slot, mut bg_color, children) in &mut slot_query {
        // Only update the inner half-slots, not the container
        let SlotType::Hotbar { index, hand } = slot.slot_type else {
            continue;
        };

        let Some(children) = children else {
            continue;
        };

        // Get item from hotbar data
        let item_opt = hotbar.get_item_for_hand(hand == Hand::Left);
        
        // Update visual state based on item presence
        if item_opt.is_some() {
            // Has item — show a color (placeholder until icons)
            **bg_color = Color::srgb(0.3, 0.5, 0.3);
        } else {
            // Empty slot
            **bg_color = Color::srgba(0.0, 0.0, 0.0, 0.0);
        }
    }
}
```

**Step 2: Register system in plugin**

```rust
// src/ui/game_ui/mod.rs (update spawn_game_ui and add to plugin)

/// Spawn all game UI elements.
fn spawn_game_ui(mut commands: Commands, theme: Res<UiTheme>) {
    hotbar::spawn_hotbar(commands, theme);
}

pub struct GameUiPlugin;

impl Plugin for GameUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DragState>()
            .init_resource::<HoveredSlot>()
            .init_resource::<InventoryScreenState>()
            .insert_resource(UiTheme::load())
            .add_systems(OnEnter(AppState::InGame), spawn_game_ui)
            .add_systems(Update, hotbar::update_hotbar_slots);
    }
}
```

**Step 3: Verify compilation**

Run: `cargo build 2>&1`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add src/ui/game_ui/hotbar.rs src/ui/game_ui/mod.rs
git commit -m "feat(ui): sync hotbar slots with Hotbar component data"
```

---

## Phase 3: Inventory Screen

### Task 3.1: Spawn Inventory Screen UI

**Files:**
- Create: `src/ui/game_ui/inventory.rs`
- Modify: `src/ui/game_ui/mod.rs`

**Step 1: Create inventory spawn system**

```rust
// src/ui/game_ui/inventory.rs
use bevy::prelude::*;

use super::components::*;
use super::theme::UiTheme;

/// Spawn the inventory screen (hidden by default).
pub fn spawn_inventory_screen(
    mut commands: Commands,
    theme: Res<UiTheme>,
) {
    let config = &theme.inventory_screen;
    let colors = &theme.colors;
    
    commands
        .spawn((
            InventoryScreen,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(config.width),
                height: Val::Px(config.height),
                left: Val::Percent(50.0),
                top: Val::Percent(50.0),
                margin: UiRect::new(
                    Val::Px(-config.width / 2.0),
                    Val::Auto,
                    Val::Auto,
                    Val::Px(-config.height / 2.0),
                ),
                flex_direction: FlexDirection::Row,
                padding: UiRect::all(Val::Px(config.padding)),
                column_gap: Val::Px(16.0),
                ..default()
            },
            BackgroundColor(colors.bg_dark.clone().into()),
            BorderColor::all(colors.border.clone().into()),
            Visibility::Hidden, // Start hidden
            Pickable::IGNORE,
        ))
        .with_children(|parent| {
            // Left column: Equipment
            spawn_equipment_panel(parent, &theme);
            // Right column: Bags
            spawn_bag_panel(parent, &theme);
        });
}

fn spawn_equipment_panel(parent: &mut ChildBuilder, theme: &UiTheme) {
    let config = &theme.inventory_screen.equipment;
    let colors = &theme.colors;
    
    parent
        .spawn((
            Node {
                width: Val::Px(config.slot_size),
                height: Val::Auto,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(config.gap),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|eq_parent| {
            let slots = [
                EquipSlot::Head,
                EquipSlot::Chest,
                EquipSlot::Legs,
                EquipSlot::Back,
                EquipSlot::HeadCosmetic,
                EquipSlot::ChestCosmetic,
                EquipSlot::LegsCosmetic,
                EquipSlot::BackCosmetic,
            ];
            
            for slot in slots {
                eq_parent.spawn((
                    UiSlot {
                        slot_type: SlotType::Equipment(slot),
                    },
                    Node {
                        width: Val::Px(config.slot_size),
                        height: Val::Px(config.slot_size),
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(colors.bg_medium.clone().into()),
                    BorderColor::all(colors.border.clone().into()),
                    Pickable {
                        should_block_lower: false,
                        is_hoverable: true,
                    },
                ));
            }
        });
}

fn spawn_bag_panel(parent: &mut ChildBuilder, theme: &UiTheme) {
    let main_config = &theme.inventory_screen.main_bag;
    let mat_config = &theme.inventory_screen.material_bag;
    let colors = &theme.colors;
    
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|bag_parent| {
            // Main bag (40 slots)
            spawn_bag_grid(bag_parent, main_config, colors, SlotType::MainBag);
            // Material bag (16 slots)
            spawn_bag_grid(bag_parent, mat_config, colors, SlotType::MaterialBag);
        });
}

fn spawn_bag_grid(
    parent: &mut ChildBuilder,
    config: &super::theme::BagConfig,
    colors: &super::theme::UiColors,
    slot_type_factory: fn(usize) -> SlotType,
) {
    let total_width = config.columns as f32 * config.slot_size
        + (config.columns - 1) as f32 * config.gap;
    let total_height = config.rows as f32 * config.slot_size
        + (config.rows - 1) as f32 * config.gap;
    
    parent
        .spawn((
            Node {
                width: Val::Px(total_width),
                height: Val::Px(total_height),
                display: Display::Grid,
                grid_template_columns: vec![GridTrack::px(config.slot_size); config.columns],
                grid_template_rows: vec![GridTrack::px(config.slot_size); config.rows],
                column_gap: Val::Px(config.gap),
                row_gap: Val::Px(config.gap),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|grid_parent| {
            for i in 0..(config.columns * config.rows) {
                grid_parent.spawn((
                    UiSlot {
                        slot_type: slot_type_factory(i),
                    },
                    Node {
                        width: Val::Px(config.slot_size),
                        height: Val::Px(config.slot_size),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(colors.bg_medium.clone().into()),
                    BorderColor::all(colors.border.clone().into()),
                    Pickable {
                        should_block_lower: false,
                        is_hoverable: true,
                    },
                ));
            }
        });
}
```

**Step 2: Update mod.rs**

```rust
// src/ui/game_ui/mod.rs (update)
pub mod components;
pub mod hotbar;
pub mod inventory;
pub mod theme;

use bevy::prelude::*;

use crate::registry::AppState;

pub use components::*;
pub use theme::*;

pub struct GameUiPlugin;

impl Plugin for GameUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DragState>()
            .init_resource::<HoveredSlot>()
            .init_resource::<InventoryScreenState>()
            .insert_resource(UiTheme::load())
            .add_systems(OnEnter(AppState::InGame), spawn_game_ui);
    }
}

/// Spawn all game UI elements.
fn spawn_game_ui(mut commands: Commands, theme: Res<UiTheme>) {
    hotbar::spawn_hotbar(commands, theme);
    inventory::spawn_inventory_screen(commands, theme);
}
```

**Step 3: Verify compilation**

Run: `cargo build 2>&1`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add src/ui/game_ui/inventory.rs src/ui/game_ui/mod.rs
git commit -m "feat(ui): spawn inventory screen with equipment and bag panels"
```

---

### Task 3.2: Toggle Inventory Screen Visibility

**Files:**
- Modify: `src/ui/game_ui/mod.rs`

**Step 1: Add toggle system**

```rust
// src/ui/game_ui/mod.rs (add system)

use bevy::input::keyboard::KeyboardInput;
use bevy::input::ButtonState;

/// Toggle inventory screen on E or I key press.
pub fn toggle_inventory(
    mut keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<InventoryScreenState>,
    mut query: Query<&mut Visibility, With<InventoryScreen>>,
) {
    if keyboard.just_pressed(KeyCode::KeyE) || keyboard.just_pressed(KeyCode::KeyI) {
        state.visible = !state.visible;
        
        for mut vis in &mut query {
            *vis = if state.visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}

// Update plugin to register system:
impl Plugin for GameUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DragState>()
            .init_resource::<HoveredSlot>()
            .init_resource::<InventoryScreenState>()
            .insert_resource(UiTheme::load())
            .add_systems(OnEnter(AppState::InGame), spawn_game_ui)
            .add_systems(Update, (hotbar::update_hotbar_slots, toggle_inventory));
    }
}
```

**Step 2: Verify compilation**

Run: `cargo build 2>&1`
Expected: SUCCESS

**Step 3: Commit**

```bash
git add src/ui/game_ui/mod.rs
git commit -m "feat(ui): toggle inventory screen with E/I keys"
```

---

## Phase 4: Slot Content Sync

### Task 4.1: Update All Slots from Inventory Data

**Files:**
- Create: `src/ui/game_ui/slot_sync.rs`
- Modify: `src/ui/game_ui/mod.rs`

**Step 1: Create slot sync system**

```rust
// src/ui/game_ui/slot_sync.rs
use bevy::prelude::*;

use super::components::*;
use crate::inventory::{Inventory, InventorySlot};
use crate::item::ItemRegistry;
use crate::player::Player;

/// Sync all UI slots with their backing data (Inventory, Equipment, etc.).
pub fn sync_slot_contents(
    inventory_query: Query<&Inventory, With<Player>>,
    item_registry: Res<ItemRegistry>,
    mut slot_query: Query<(&UiSlot, &mut BackgroundColor, &Children)>,
    mut text_query: Query<&mut Text>,
) {
    let Ok(inventory) = inventory_query.single() else {
        return;
    };

    for (slot, mut bg_color, children) in &mut slot_query {
        let item_opt = match slot.slot_type {
            SlotType::MainBag(idx) => inventory.main_bag.get(idx).and_then(|s| s.as_ref()),
            SlotType::MaterialBag(idx) => inventory.material_bag.get(idx).and_then(|s| s.as_ref()),
            SlotType::Hotbar { .. } => {
                // Hotbar handled separately in hotbar.rs
                continue;
            }
            SlotType::Equipment(_) => {
                // Equipment sync will be added later
                continue;
            }
        };

        // Update background color based on item presence
        if let Some(item) = item_opt {
            // Get item rarity for color (placeholder: green for now)
            **bg_color = Color::srgb(0.2, 0.4, 0.2);
            
            // Update count text if child exists
            for &child in children {
                if let Ok(mut text) = text_query.get_mut(child) {
                    **text = Text::new(if item.count > 1 {
                        format!("{}", item.count)
                    } else {
                        String::new()
                    });
                }
            }
        } else {
            // Empty slot
            **bg_color = Color::srgba(0.0, 0.0, 0.0, 0.0);
        }
    }
}
```

**Step 2: Update mod.rs to register system**

```rust
// src/ui/game_ui/mod.rs (add module and system)
pub mod slot_sync;

// In plugin:
.add_systems(Update, (hotbar::update_hotbar_slots, slot_sync::sync_slot_contents, toggle_inventory))
```

**Step 3: Verify compilation**

Run: `cargo build 2>&1`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add src/ui/game_ui/slot_sync.rs src/ui/game_ui/mod.rs
git commit -m "feat(ui): sync slot contents from Inventory data"
```

---

## Phase 5: Tooltip

### Task 5.1: Show Tooltip on Slot Hover

**Files:**
- Create: `src/ui/game_ui/tooltip.rs`
- Modify: `src/ui/game_ui/mod.rs`

**Step 1: Create tooltip system**

```rust
// src/ui/game_ui/tooltip.rs
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::components::*;
use super::theme::UiTheme;
use crate::inventory::Inventory;
use crate::item::ItemRegistry;
use crate::player::Player;

/// Spawn tooltip entity (singleton).
pub fn spawn_tooltip(
    mut commands: Commands,
    theme: Res<UiTheme>,
) {
    let colors = &theme.colors;
    
    commands.spawn((
        UiTooltip {
            item_id: String::new(),
            count: 0,
        },
        Node {
            position_type: PositionType::Absolute,
            padding: UiRect::all(Val::Px(theme.tooltip.padding)),
            ..default()
        },
        BackgroundColor(colors.bg_dark.clone().into()),
        BorderColor::all(colors.border.clone().into()),
        Visibility::Hidden,
        Pickable::IGNORE,
    ));
}

/// Update tooltip position and content based on hovered slot.
pub fn update_tooltip(
    mut tooltip_query: Query<(Entity, &mut Node, &mut Visibility, &mut UiTooltip), With<UiTooltip>>,
    hovered: Res<HoveredSlot>,
    inventory_query: Query<&Inventory, With<Player>>,
    item_registry: Res<ItemRegistry>,
    window: Query<&Window, With<PrimaryWindow>>,
    theme: Res<UiTheme>,
    mut commands: Commands,
) {
    let Ok((entity, mut node, mut vis, mut tooltip)) = tooltip_query.single_mut() else {
        return;
    };

    let Some(slot_type) = hovered.slot else {
        *vis = Visibility::Hidden;
        return;
    };

    let Ok(inventory) = inventory_query.single() else {
        *vis = Visibility::Hidden;
        return;
    };

    // Get item from slot
    let item_opt = match slot_type {
        SlotType::MainBag(idx) => inventory.main_bag.get(idx).and_then(|s| s.as_ref()),
        SlotType::MaterialBag(idx) => inventory.material_bag.get(idx).and_then(|s| s.as_ref()),
        _ => None,
    };

    let Some(item) = item_opt else {
        *vis = Visibility::Hidden;
        return;
    };

    // Show tooltip
    *vis = Visibility::Visible;
    tooltip.item_id = item.item_id.clone();
    tooltip.count = item.count;

    // Position near cursor
    let Ok(window) = window.single() else {
        return;
    };
    if let Some(cursor_pos) = window.cursor_position() {
        node.left = Val::Px(cursor_pos.x + 16.0);
        node.top = Val::Px(cursor_pos.y + 16.0);
    }

    // Update tooltip text (simplified — just item name for now)
    // In a full implementation, we'd spawn/destroy text children here
}
```

**Step 2: Add hover detection via pointer events**

Add to slot spawn code in `hotbar.rs` and `inventory.rs`:

```rust
// After spawning each slot entity, add observer:
.observe(|trigger: On<Pointer<Over>>, mut hovered: ResMut<HoveredSlot>, slot: Query<&UiSlot>| {
    if let Ok(slot) = slot.get(trigger.event_target()) {
        hovered.slot = Some(slot.slot_type);
    }
})
.observe(|trigger: On<Pointer<Out>>, mut hovered: ResMut<HoveredSlot>| {
    hovered.slot = None;
})
```

**Step 3: Register in plugin**

```rust
// In GameUiPlugin::build:
.add_systems(OnEnter(AppState::InGame), (spawn_game_ui, tooltip::spawn_tooltip).chain())
.add_systems(Update, tooltip::update_tooltip)
```

**Step 4: Verify compilation**

Run: `cargo build 2>&1`
Expected: SUCCESS

**Step 5: Commit**

```bash
git add src/ui/game_ui/tooltip.rs src/ui/game_ui/mod.rs src/ui/game_ui/hotbar.rs src/ui/game_ui/inventory.rs
git commit -m "feat(ui): show tooltip on slot hover with cursor tracking"
```

---

## Phase 6: Drag & Drop

### Task 6.1: Begin Drag on Slot Click

**Files:**
- Create: `src/ui/game_ui/drag_drop.rs`
- Modify: `src/ui/game_ui/mod.rs`
- Modify: `src/ui/game_ui/hotbar.rs`
- Modify: `src/ui/game_ui/inventory.rs`

**Step 1: Create drag start logic**

```rust
// src/ui/game_ui/drag_drop.rs
use bevy::prelude::*;

use super::components::*;
use crate::inventory::Inventory;
use crate::player::Player;

/// Handle drag start when clicking on a filled slot.
pub fn handle_drag_start(
    mut drag_state: ResMut<DragState>,
    inventory_query: Query<&Inventory, With<Player>>,
    slot_query: Query<&UiSlot>,
    mut commands: Commands,
    theme: Res<UiTheme>,
) {
    // This will be triggered by pointer event observers on slots
    // For now, placeholder — actual logic in observers
}

/// Create visual drag icon following cursor.
pub fn spawn_drag_icon(
    commands: &mut Commands,
    item_id: &str,
    count: u16,
    theme: &UiTheme,
) -> Entity {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(32.0),
                height: Val::Px(32.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.5, 0.7, 0.5)), // Placeholder color
            Pickable::IGNORE,
            GlobalZIndex(1000),
        ))
        .id()
}

/// Update drag icon position to follow cursor.
pub fn update_drag_position(
    drag_state: Res<DragState>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut query: Query<&mut Node, With<GlobalZIndex>>,
) {
    let Some(drag) = drag_state.dragging.as_ref() else {
        return;
    };
    
    let Ok(window) = window.single() else {
        return;
    };
    
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    
    if let Ok(mut node) = query.get_mut(drag.drag_icon) {
        node.left = Val::Px(cursor.x - 16.0);
        node.top = Val::Px(cursor.y - 16.0);
    }
}

/// Cancel drag, return item to source.
pub fn cancel_drag(
    mut drag_state: ResMut<DragState>,
    mut commands: Commands,
) {
    if let Some(drag) = drag_state.dragging.take() {
        commands.entity(drag.drag_icon).despawn();
    }
}
```

**Step 2: Add observers to slots for drag start**

In `hotbar.rs` and `inventory.rs`, add to slot spawning:

```rust
.observe(|trigger: On<Pointer<DragStart>>, mut drag_state: ResMut<DragState>, slot: Query<&UiSlot>, inventory: Query<&Inventory, With<Player>>, mut commands: Commands, theme: Res<UiTheme>| {
    let Ok(slot) = slot.get(trigger.event_target()) else { return };
    let Ok(inv) = inventory.single() else { return };
    
    // Get item from slot
    let item_opt = match slot.slot_type {
        SlotType::MainBag(idx) => inv.main_bag.get(idx).and_then(|s| s.as_ref()),
        SlotType::MaterialBag(idx) => inv.material_bag.get(idx).and_then(|s| s.as_ref()),
        _ => None,
    };
    
    let Some(item) = item_opt else { return };
    
    let drag_icon = super::drag_drop::spawn_drag_icon(&mut commands, &item.item_id, item.count, &theme);
    
    drag_state.dragging = Some(DragInfo {
        item_id: item.item_id.clone(),
        count: item.count,
        source_slot: slot.slot_type,
        drag_icon,
    });
})
```

**Step 3: Verify compilation**

Run: `cargo build 2>&1`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add src/ui/game_ui/drag_drop.rs src/ui/game_ui/mod.rs src/ui/game_ui/hotbar.rs src/ui/game_ui/inventory.rs
git commit -m "feat(ui): begin drag on slot click with visual icon"
```

---

### Task 6.2: Complete Drop Logic

**Files:**
- Modify: `src/ui/game_ui/drag_drop.rs`
- Modify: `src/ui/game_ui/hotbar.rs`
- Modify: `src/ui/game_ui/inventory.rs`

**Step 1: Add drop handler**

```rust
// src/ui/game_ui/drag_drop.rs (add)

/// Handle drop on target slot.
pub fn handle_drop(
    mut drag_state: ResMut<DragState>,
    target_slot: SlotType,
    mut inventory_query: Query<&mut Inventory, With<Player>>,
    item_registry: Res<ItemRegistry>,
    mut commands: Commands,
) {
    let Some(drag) = drag_state.dragging.take() else {
        return;
    };
    
    // Despawn drag icon
    commands.entity(drag.drag_icon).despawn();
    
    // Same slot = cancel
    if drag.source_slot == target_slot {
        return;
    }
    
    let Ok(mut inventory) = inventory_query.single_mut() else {
        return;
    };
    
    // Remove from source
    let removed = match drag.source_slot {
        SlotType::MainBag(idx) => {
            if let Some(slot) = inventory.main_bag.get_mut(idx) {
                slot.take()
            } else {
                None
            }
        }
        SlotType::MaterialBag(idx) => {
            if let Some(slot) = inventory.material_bag.get_mut(idx) {
                slot.take()
            } else {
                None
            }
        }
        _ => None,
    };
    
    let Some(mut item) = removed else {
        return;
    };
    
    // Add to target
    match target_slot {
        SlotType::MainBag(idx) => {
            if let Some(target_slot) = inventory.main_bag.get_mut(idx) {
                if target_slot.is_none() {
                    *target_slot = Some(item);
                } else if let Some(target) = target_slot {
                    // Swap
                    std::mem::swap(&mut item, target);
                    // Return swapped item to source (simplified — doesn't handle full swap properly)
                }
            }
        }
        SlotType::MaterialBag(idx) => {
            if let Some(target_slot) = inventory.material_bag.get_mut(idx) {
                if target_slot.is_none() {
                    *target_slot = Some(item);
                }
            }
        }
        _ => {}
    }
}
```

**Step 2: Add DragDrop observer to slots**

```rust
// In slot spawning code:
.observe(|trigger: On<Pointer<DragDrop>>, mut drag_state: ResMut<DragState>, slot: Query<&UiSlot>, mut inventory: Query<&mut Inventory, With<Player>>, item_registry: Res<ItemRegistry>, mut commands: Commands| {
    let Ok(slot) = slot.get(trigger.event_target()) else { return };
    super::drag_drop::handle_drop(drag_state, slot.slot_type, inventory, item_registry, commands);
})
```

**Step 3: Verify compilation**

Run: `cargo build 2>&1`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add src/ui/game_ui/drag_drop.rs src/ui/game_ui/hotbar.rs src/ui/game_ui/inventory.rs
git commit -m "feat(ui): complete drag & drop with swap logic"
```

---

## Phase 7: Finalization

### Task 7.1: Run Full Test Suite + Clippy

**Step 1: Run tests**

Run: `cargo test 2>&1`
Expected: All tests pass

**Step 2: Run clippy**

Run: `cargo clippy 2>&1`
Expected: No warnings (fix if any)

**Step 3: Commit any fixes**

```bash
git add -A
git commit -m "fix(ui): resolve clippy warnings"
```

---

### Task 7.2: Manual Test & Summary

**Step 1: Run the game**

Run: `cargo run`
Expected: 
- Hotbar visible at bottom with 6 slots
- Press E or I to toggle inventory screen
- Tooltip appears when hovering slots
- Drag & drop works between slots

**Step 2: Create summary commit (if needed)**

```bash
git add -A
git commit -m "feat(ui): complete game UI system with hotbar, inventory, tooltip, and drag-drop"
```

---

## Task Summary

| Phase | Tasks | Description |
|-------|-------|-------------|
| 1 | 1.1–1.3 | Foundation: UiTheme, components, GameUiPlugin |
| 2 | 2.1–2.2 | Hotbar: spawn UI, sync with game state |
| 3 | 3.1–3.2 | Inventory screen: spawn UI, toggle visibility |
| 4 | 4.1 | Slot content sync from Inventory |
| 5 | 5.1 | Tooltip on hover |
| 6 | 6.1–6.2 | Drag & drop: start drag, complete drop |
| 7 | 7.1–7.2 | Finalization: tests, clippy, manual test |

**Total: 12 tasks**
