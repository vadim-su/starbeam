use bevy::prelude::*;

/// Top-level system ordering sets for the game loop.
///
/// Configured as a chain: Input → Physics → WorldUpdate → Camera → Parallax → Ui.
/// Individual plugins place their systems into the appropriate set.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameSet {
    Input,
    Physics,
    WorldUpdate,
    Camera,
    Parallax,
    Ui,
}
