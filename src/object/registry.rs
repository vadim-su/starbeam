use std::collections::HashMap;

use bevy::prelude::*;

use super::definition::{ObjectDef, ObjectId};

#[derive(Resource, Debug)]
pub struct ObjectRegistry {
    defs: Vec<ObjectDef>,
    name_to_id: HashMap<String, ObjectId>,
}

impl ObjectRegistry {
    pub fn from_defs(mut defs: Vec<ObjectDef>) -> Self {
        for def in &mut defs {
            def.validate();
        }
        let name_to_id = defs
            .iter()
            .enumerate()
            .map(|(i, d)| (d.id.clone(), ObjectId(i as u16)))
            .collect();
        Self { defs, name_to_id }
    }

    pub fn get(&self, id: ObjectId) -> &ObjectDef {
        &self.defs[id.0 as usize]
    }

    pub fn try_get(&self, id: ObjectId) -> Option<&ObjectDef> {
        self.defs.get(id.0 as usize)
    }

    pub fn by_name(&self, name: &str) -> Option<ObjectId> {
        self.name_to_id.get(name).copied()
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{ObjectType, PlacementRule};

    fn test_registry() -> ObjectRegistry {
        ObjectRegistry::from_defs(vec![
            ObjectDef {
                id: "torch".into(),
                display_name: "Torch".into(),
                size: (1, 1),
                sprite: "objects/torch.png".into(),
                solid_mask: vec![false],
                placement: PlacementRule::Wall,
                light_emission: [240, 180, 80],
                object_type: ObjectType::LightSource,
                drops: vec![],
                sprite_columns: 1,
                sprite_rows: 1,
                sprite_fps: 0.0,
                flicker_speed: 0.0,
                flicker_strength: 0.0,
                flicker_min: 1.0,
                auto_item: None,
                background: false,
            },
            ObjectDef {
                id: "chest".into(),
                display_name: "Wooden Chest".into(),
                size: (2, 1),
                sprite: "objects/chest.png".into(),
                solid_mask: vec![true, true],
                placement: PlacementRule::Floor,
                light_emission: [0, 0, 0],
                object_type: ObjectType::Container { slots: 16 },
                drops: vec![],
                sprite_columns: 1,
                sprite_rows: 1,
                sprite_fps: 0.0,
                flicker_speed: 0.0,
                flicker_strength: 0.0,
                flicker_min: 1.0,
                auto_item: None,
                background: false,
            },
        ])
    }

    #[test]
    fn lookup_by_name() {
        let reg = test_registry();
        assert_eq!(reg.by_name("torch"), Some(ObjectId(0)));
        assert_eq!(reg.by_name("chest"), Some(ObjectId(1)));
    }

    #[test]
    fn by_name_returns_none_for_unknown() {
        let reg = test_registry();
        assert_eq!(reg.by_name("nonexistent"), None);
    }

    #[test]
    fn get_returns_def() {
        let reg = test_registry();
        let torch = reg.get(ObjectId(0));
        assert_eq!(torch.id, "torch");
        assert_eq!(torch.size, (1, 1));
    }

    #[test]
    fn registry_len() {
        let reg = test_registry();
        assert_eq!(reg.len(), 2);
        assert!(!reg.is_empty());
    }
}
