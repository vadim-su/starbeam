#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TileType {
    #[default]
    Air,
    Grass,
    Dirt,
    Stone,
}

impl TileType {
    /// Atlas texture index for this tile type, or `None` for Air.
    pub fn texture_index(self) -> Option<u32> {
        match self {
            TileType::Air => None,
            TileType::Grass => Some(0),
            TileType::Dirt => Some(1),
            TileType::Stone => Some(2),
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
    fn air_has_no_texture_index() {
        assert!(TileType::Air.texture_index().is_none());
    }

    #[test]
    fn solid_tiles_have_texture_indices() {
        assert_eq!(TileType::Grass.texture_index(), Some(0));
        assert_eq!(TileType::Dirt.texture_index(), Some(1));
        assert_eq!(TileType::Stone.texture_index(), Some(2));
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
