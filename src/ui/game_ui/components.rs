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

/// Marker for the count/name text label inside a UI slot.
#[derive(Component)]
pub struct SlotLabel;

/// Inventory screen visibility state.
#[derive(Resource, Default)]
pub struct InventoryScreenState {
    pub visible: bool,
}
