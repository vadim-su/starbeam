use std::collections::HashMap;

use bevy::prelude::*;
use serde::Deserialize;

/// Compact tile identifier. Index into TileRegistry.defs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TileId(pub u16);

impl TileId {
    pub const AIR: TileId = TileId(0);
}

fn default_light_opacity() -> u8 {
    15
}

/// Properties of a single tile type, deserialized from RON.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // Fields reserved for future gameplay systems
pub struct TileDef {
    pub id: String,
    pub autotile: Option<String>,
    pub solid: bool,
    pub hardness: f32,
    pub friction: f32,
    pub viscosity: f32,
    pub damage_on_contact: f32,
    #[serde(default)]
    pub effects: Vec<String>,
    #[serde(default)]
    pub light_emission: [u8; 3],
    #[serde(default = "default_light_opacity")]
    pub light_opacity: u8,
}

/// Registry of all tile definitions. Inserted as a Resource after asset loading.
#[derive(Resource)]
pub struct TileRegistry {
    pub(crate) defs: Vec<TileDef>,
    name_to_id: HashMap<String, TileId>,
}

impl TileRegistry {
    /// Build registry from a list of TileDefs. Order = TileId index.
    pub fn from_defs(defs: Vec<TileDef>) -> Self {
        let name_to_id = defs
            .iter()
            .enumerate()
            .map(|(i, d)| (d.id.clone(), TileId(i as u16)))
            .collect();
        Self { defs, name_to_id }
    }

    pub fn get(&self, id: TileId) -> &TileDef {
        &self.defs[id.0 as usize]
    }

    pub fn is_solid(&self, id: TileId) -> bool {
        self.defs[id.0 as usize].solid
    }

    pub fn autotile_name(&self, id: TileId) -> Option<&str> {
        self.defs[id.0 as usize].autotile.as_deref()
    }

    #[allow(dead_code)] // Used by lighting propagation system (Task 5)
    pub fn light_emission(&self, id: TileId) -> [u8; 3] {
        self.defs[id.0 as usize].light_emission
    }

    #[allow(dead_code)] // Used by lighting propagation system (Task 5)
    pub fn light_opacity(&self, id: TileId) -> u8 {
        self.defs[id.0 as usize].light_opacity
    }

    pub fn by_name(&self, name: &str) -> TileId {
        *self
            .name_to_id
            .get(name)
            .unwrap_or_else(|| panic!("Unknown tile: {name}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry() -> TileRegistry {
        TileRegistry::from_defs(vec![
            TileDef {
                id: "air".into(),
                autotile: None,
                solid: false,
                hardness: 0.0,
                friction: 0.0,
                viscosity: 0.0,
                damage_on_contact: 0.0,
                effects: vec![],
                light_emission: [0, 0, 0],
                light_opacity: 0,
            },
            TileDef {
                id: "grass".into(),
                autotile: Some("grass".into()),
                solid: true,
                hardness: 1.0,
                friction: 0.8,
                viscosity: 0.0,
                damage_on_contact: 0.0,
                effects: vec![],
                light_emission: [0, 0, 0],
                light_opacity: 15,
            },
            TileDef {
                id: "dirt".into(),
                autotile: Some("dirt".into()),
                solid: true,
                hardness: 2.0,
                friction: 0.7,
                viscosity: 0.0,
                damage_on_contact: 0.0,
                effects: vec![],
                light_emission: [0, 0, 0],
                light_opacity: 15,
            },
            TileDef {
                id: "stone".into(),
                autotile: Some("stone".into()),
                solid: true,
                hardness: 5.0,
                friction: 0.6,
                viscosity: 0.0,
                damage_on_contact: 0.0,
                effects: vec![],
                light_emission: [0, 0, 0],
                light_opacity: 15,
            },
        ])
    }

    #[test]
    fn air_is_always_id_zero() {
        let reg = test_registry();
        assert_eq!(reg.by_name("air"), TileId::AIR);
        assert_eq!(TileId::AIR, TileId(0));
    }

    #[test]
    fn lookup_by_name() {
        let reg = test_registry();
        assert_eq!(reg.by_name("grass"), TileId(1));
        assert_eq!(reg.by_name("dirt"), TileId(2));
        assert_eq!(reg.by_name("stone"), TileId(3));
    }

    #[test]
    fn solid_check() {
        let reg = test_registry();
        assert!(!reg.is_solid(TileId::AIR));
        assert!(reg.is_solid(TileId(1)));
        assert!(reg.is_solid(TileId(3)));
    }

    #[test]
    fn autotile_name() {
        let reg = test_registry();
        assert_eq!(reg.autotile_name(TileId::AIR), None);
        assert_eq!(reg.autotile_name(TileId(1)), Some("grass"));
        assert_eq!(reg.autotile_name(TileId(3)), Some("stone"));
    }

    #[test]
    fn get_returns_full_def() {
        let reg = test_registry();
        let stone = reg.get(TileId(3));
        assert_eq!(stone.id, "stone");
        assert_eq!(stone.hardness, 5.0);
        assert_eq!(stone.friction, 0.6);
    }

    #[test]
    fn light_properties() {
        let reg = test_registry();
        assert_eq!(reg.light_emission(TileId::AIR), [0, 0, 0]);
        assert_eq!(reg.light_opacity(TileId::AIR), 0);
        assert_eq!(reg.light_opacity(TileId(1)), 15); // grass
        assert_eq!(reg.light_opacity(TileId(3)), 15); // stone
    }

    #[test]
    #[should_panic]
    fn by_name_panics_on_unknown() {
        let reg = test_registry();
        reg.by_name("lava");
    }
}
