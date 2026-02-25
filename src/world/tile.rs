use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TileType {
    #[default]
    Air,
    Grass,
    Dirt,
    Stone,
}

impl TileType {
    pub fn color(self) -> Option<Color> {
        match self {
            TileType::Air => None,
            TileType::Grass => Some(Color::srgb(0.2, 0.7, 0.2)),
            TileType::Dirt => Some(Color::srgb(0.55, 0.35, 0.15)),
            TileType::Stone => Some(Color::srgb(0.5, 0.5, 0.5)),
        }
    }

    pub fn is_solid(self) -> bool {
        !matches!(self, TileType::Air)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn air_has_no_color() {
        assert!(TileType::Air.color().is_none());
    }

    #[test]
    fn solid_tiles_have_colors() {
        assert!(TileType::Grass.color().is_some());
        assert!(TileType::Dirt.color().is_some());
        assert!(TileType::Stone.color().is_some());
    }

    #[test]
    fn air_is_not_solid() {
        assert!(!TileType::Air.is_solid());
    }

    #[test]
    fn non_air_is_solid() {
        assert!(TileType::Grass.is_solid());
        assert!(TileType::Dirt.is_solid());
        assert!(TileType::Stone.is_solid());
    }
}
