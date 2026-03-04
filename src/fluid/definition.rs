use std::collections::HashMap;

use bevy::prelude::*;
use serde::Deserialize;

use super::cell::FluidId;

fn default_flow_rate() -> u8 {
    6
}

fn default_viscosity() -> f32 {
    1.0
}

fn default_density() -> f32 {
    1.0
}

fn default_light_opacity() -> u8 {
    2
}

/// Properties of a single fluid type, deserialized from RON.
#[derive(Debug, Clone, Deserialize)]
pub struct FluidDef {
    pub id: String,
    #[serde(default = "default_viscosity")]
    pub viscosity: f32,
    #[serde(default = "default_density")]
    pub density: f32,
    #[serde(default)]
    pub color: [u8; 3],
    #[serde(default = "default_light_opacity")]
    pub light_opacity: u8,
    #[serde(default)]
    pub damage_on_contact: f32,
    #[serde(default = "default_flow_rate")]
    pub flow_rate: u8,
    #[serde(default)]
    pub light_emission: [u8; 3],
}

/// Registry of all fluid definitions. Inserted as a Resource after asset loading.
#[derive(Resource)]
pub struct FluidRegistry {
    pub(crate) defs: Vec<FluidDef>,
    name_to_id: HashMap<String, FluidId>,
}

impl FluidRegistry {
    /// Build registry from a list of FluidDefs. Order = FluidId index.
    /// Index 0 is reserved for "none" (no fluid), so defs start at index 1.
    pub fn from_defs(defs: Vec<FluidDef>) -> Self {
        let name_to_id = defs
            .iter()
            .enumerate()
            .map(|(i, d)| (d.id.clone(), FluidId((i + 1) as u8)))
            .collect();
        Self { defs, name_to_id }
    }

    pub fn get(&self, id: FluidId) -> &FluidDef {
        assert!(
            id.0 > 0 && (id.0 as usize) <= self.defs.len(),
            "Invalid FluidId: {}",
            id.0
        );
        &self.defs[(id.0 - 1) as usize]
    }

    pub fn by_name(&self, name: &str) -> FluidId {
        *self
            .name_to_id
            .get(name)
            .unwrap_or_else(|| panic!("Unknown fluid: {name}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry() -> FluidRegistry {
        FluidRegistry::from_defs(vec![
            FluidDef {
                id: "water".into(),
                viscosity: 1.0,
                density: 1.0,
                color: [64, 128, 255],
                light_opacity: 2,
                damage_on_contact: 0.0,
                flow_rate: 6,
                light_emission: [0, 0, 0],
            },
            FluidDef {
                id: "lava".into(),
                viscosity: 4.0,
                density: 3.0,
                color: [255, 100, 20],
                light_opacity: 0,
                damage_on_contact: 40.0,
                flow_rate: 2,
                light_emission: [200, 100, 20],
            },
        ])
    }

    #[test]
    fn lookup_by_name() {
        let reg = test_registry();
        assert_eq!(reg.by_name("water"), FluidId(1));
        assert_eq!(reg.by_name("lava"), FluidId(2));
    }

    #[test]
    fn get_returns_def() {
        let reg = test_registry();
        let water = reg.get(FluidId(1));
        assert_eq!(water.id, "water");
        assert_eq!(water.flow_rate, 6);
    }

    #[test]
    #[should_panic]
    fn get_none_panics() {
        let reg = test_registry();
        reg.get(FluidId::NONE);
    }

    #[test]
    #[should_panic]
    fn by_name_panics_on_unknown() {
        let reg = test_registry();
        reg.by_name("oil");
    }
}
