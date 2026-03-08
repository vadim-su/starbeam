use bevy::prelude::*;

/// Which body part this child entity represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PartType {
    BackArm,
    Legs,
    Body,
    Head,
    FrontArm,
}

impl PartType {
    /// Z-offset for render ordering. Higher = in front.
    pub fn z_offset(self) -> f32 {
        match self {
            PartType::BackArm => -0.03,
            PartType::Legs => -0.02,
            PartType::Body => -0.01,
            PartType::Head => 0.0,
            PartType::FrontArm => 0.01,
        }
    }

    /// All part types in spawn order.
    pub const ALL: [PartType; 5] = [
        PartType::BackArm,
        PartType::Legs,
        PartType::Body,
        PartType::Head,
        PartType::FrontArm,
    ];

    /// Whether this part type is an arm.
    pub fn is_arm(self) -> bool {
        matches!(self, PartType::FrontArm | PartType::BackArm)
    }
}

/// Marker component on each body-part child entity.
#[derive(Component)]
pub struct CharacterPart(pub PartType);

/// Marker + state for arms that can aim toward the cursor.
#[derive(Component)]
pub struct ArmAiming {
    /// Whether aiming is currently active (item in hotbar slot).
    pub active: bool,
    /// Pivot point for rotation in pixels relative to sprite center.
    pub pivot: Vec2,
    /// Default rotation angle (radians) when not aiming.
    pub default_angle: f32,
}
