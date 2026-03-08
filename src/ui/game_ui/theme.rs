use bevy::prelude::*;
use bevy::reflect::TypePath;
use serde::Deserialize;

/// Parsed hex color wrapper for RON deserialization.
#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct HexColor(pub String);

impl From<HexColor> for Color {
    fn from(hex: HexColor) -> Self {
        let s = hex.0.trim_start_matches('#');
        let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(0) as f32 / 255.0;
        let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(0) as f32 / 255.0;
        let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(0) as f32 / 255.0;
        if s.len() >= 8 {
            let a = u8::from_str_radix(&s[6..8], 16).unwrap_or(255) as f32 / 255.0;
            Color::srgba(r, g, b, a)
        } else {
            Color::srgb(r, g, b)
        }
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

/// 9-slice texture configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct SliceConfig {
    pub texture: String,
    pub border: f32,
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
    pub slot_texture: Option<SliceConfig>,
    /// Font size for slot number labels (1, 2, 3…).
    #[serde(default = "default_label_font_size")]
    pub label_font_size: f32,
}

fn default_label_font_size() -> f32 {
    20.0
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

/// Chat panel configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatConfig {
    pub max_messages: usize,
    pub visible_lines: usize,
    pub fade_delay_secs: f32,
    pub fade_duration_secs: f32,
    pub font: String,
    pub font_size: f32,
    pub width: f32,
    pub height: f32,
    pub system_color: HexColor,
    pub dialog_color: HexColor,
    pub command_color: HexColor,
    pub player_color: HexColor,
    pub input_bg_color: HexColor,
    pub active_bg_color: HexColor,
}

/// Root UI theme loaded from RON.
#[derive(Asset, TypePath, Debug, Clone, Deserialize, Resource)]
pub struct UiTheme {
    pub base_path: String,
    pub font_size: f32,
    pub colors: UiColors,
    pub hotbar: HotbarConfig,
    pub inventory_screen: InventoryScreenConfig,
    pub tooltip: TooltipConfig,
    pub panel_texture: Option<SliceConfig>,
    pub chat: ChatConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_color_rgb() {
        let color: Color = HexColor("#FF8000".into()).into();
        let Color::Srgba(c) = color else {
            panic!("expected Srgba");
        };
        assert!((c.red - 1.0).abs() < 0.01);
        assert!((c.green - 0.502).abs() < 0.01);
        assert!((c.blue - 0.0).abs() < 0.01);
        assert!((c.alpha - 1.0).abs() < 0.01);
    }

    #[test]
    fn hex_color_rgba() {
        let color: Color = HexColor("#FF800080".into()).into();
        let Color::Srgba(c) = color else {
            panic!("expected Srgba");
        };
        assert!((c.red - 1.0).abs() < 0.01);
        assert!((c.alpha - 0.502).abs() < 0.01);
    }
}
