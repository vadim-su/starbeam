use bevy::prelude::*;

/// Which body part this child entity represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PartType {
    BackArm,
    Body,
    Head,
    FrontArm,
}

impl PartType {
    /// Z-offset for render ordering. Higher = in front.
    pub fn z_offset(self) -> f32 {
        match self {
            PartType::BackArm => -0.02,
            PartType::Body => -0.01,
            PartType::Head => 0.0,
            PartType::FrontArm => 0.01,
        }
    }

    /// All part types in spawn order.
    pub const ALL: [PartType; 4] = [
        PartType::BackArm,
        PartType::Body,
        PartType::Head,
        PartType::FrontArm,
    ];
}

/// Marker component on each body-part child entity.
#[derive(Component)]
pub struct CharacterPart(pub PartType);

/// Marker + state for arms that can aim toward the cursor.
#[derive(Component)]
pub struct ArmAiming {
    /// Whether aiming is currently active (item in hotbar slot).
    pub active: bool,
}
