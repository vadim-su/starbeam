use bevy::prelude::*;
use serde::Deserialize;

/// Parsed hex color wrapper for RON deserialization.
#[derive(Debug, Clone, Deserialize)]
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
