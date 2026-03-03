use std::collections::HashMap;

use bevy::prelude::*;
use serde::Deserialize;

use super::cell::FluidId;

fn default_max_compress() -> f32 {
    0.02
}

fn default_viscosity() -> f32 {
    0.1
}

fn default_density() -> f32 {
    1000.0
}

fn default_color() -> [u8; 4] {
    [128, 128, 255, 180]
}

fn default_wave_amplitude() -> f32 {
    1.0
}

fn default_wave_speed() -> f32 {
    1.0
}

fn default_light_absorption() -> f32 {
    0.0
}

/// Properties of a single fluid/gas type, deserialized from RON.
#[derive(Debug, Clone, Deserialize)]
pub struct FluidDef {
    pub id: String,
    #[serde(default = "default_density")]
    pub density: f32,
    #[serde(default = "default_viscosity")]
    pub viscosity: f32,
    #[serde(default = "default_max_compress")]
    pub max_compress: f32,
    #[serde(default)]
    pub is_gas: bool,
    #[serde(default = "default_color")]
    pub color: [u8; 4],
    #[serde(default)]
    pub damage_on_contact: f32,
    #[serde(default)]
    pub light_emission: [u8; 3],
    #[serde(default)]
    pub effects: Vec<String>,
    /// Multiplier for the shader ripple amplitude (default 1.0).
    #[serde(default = "default_wave_amplitude")]
    pub wave_amplitude: f32,
    /// Multiplier for the shader ripple speed (default 1.0).
    #[serde(default = "default_wave_speed")]
    pub wave_speed: f32,
    /// How much this fluid blocks light (0.0 = transparent, 1.0 = opaque).
    /// Used by RC lighting to attenuate light through fluid.
    #[serde(default = "default_light_absorption")]
    pub light_absorption: f32,
}

/// Runtime registry of all fluid types. Index 0 is reserved for NONE.
#[derive(Resource, Debug)]
pub struct FluidRegistry {
    pub(crate) defs: Vec<FluidDef>,
    name_to_id: HashMap<String, FluidId>,
}

impl FluidRegistry {
    /// Build registry from a list of definitions.
    /// Index 0 is reserved (NONE), so defs start at index 1.
    pub fn from_defs(defs: Vec<FluidDef>) -> Self {
        let mut name_to_id = HashMap::new();
        for (i, def) in defs.iter().enumerate() {
            let fid = FluidId((i + 1) as u8);
            name_to_id.insert(def.id.clone(), fid);
        }
        Self { defs, name_to_id }
    }

    /// Get definition by FluidId. Panics if id is NONE or out of range.
    pub fn get(&self, id: FluidId) -> &FluidDef {
        assert!(id != FluidId::NONE, "Cannot get def for FluidId::NONE");
        &self.defs[(id.0 - 1) as usize]
    }

    /// Look up FluidId by string name. Panics if not found.
    pub fn by_name(&self, name: &str) -> FluidId {
        *self
            .name_to_id
            .get(name)
            .unwrap_or_else(|| panic!("Unknown fluid: {name}"))
    }

    /// Look up FluidId by string name, returns None if not found.
    pub fn try_by_name(&self, name: &str) -> Option<FluidId> {
        self.name_to_id.get(name).copied()
    }

    /// Number of registered fluid types (excluding NONE).
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

    fn test_defs() -> Vec<FluidDef> {
        vec![
            FluidDef {
                id: "water".to_string(),
                density: 1000.0,
                viscosity: 0.1,
                max_compress: 0.02,
                is_gas: false,
                color: [64, 128, 255, 180],
                damage_on_contact: 0.0,
                light_emission: [0, 0, 0],
                effects: vec![],
                wave_amplitude: 1.0,
                wave_speed: 1.0,
                light_absorption: 0.3,
            },
            FluidDef {
                id: "lava".to_string(),
                density: 3000.0,
                viscosity: 0.6,
                max_compress: 0.01,
                is_gas: false,
                color: [255, 80, 20, 220],
                damage_on_contact: 10.0,
                light_emission: [255, 100, 20],
                effects: vec![],
                wave_amplitude: 0.4,
                wave_speed: 0.3,
                light_absorption: 0.8,
            },
            FluidDef {
                id: "steam".to_string(),
                density: 0.6,
                viscosity: 0.05,
                max_compress: 0.01,
                is_gas: true,
                color: [200, 200, 200, 100],
                damage_on_contact: 0.0,
                light_emission: [0, 0, 0],
                effects: vec![],
                wave_amplitude: 0.6,
                wave_speed: 1.5,
                light_absorption: 0.05,
            },
        ]
    }

    #[test]
    fn registry_from_defs() {
        let reg = FluidRegistry::from_defs(test_defs());
        assert_eq!(reg.len(), 3);
    }

    #[test]
    fn registry_by_name() {
        let reg = FluidRegistry::from_defs(test_defs());
        let water_id = reg.by_name("water");
        assert_eq!(water_id, FluidId(1));
        let lava_id = reg.by_name("lava");
        assert_eq!(lava_id, FluidId(2));
        let steam_id = reg.by_name("steam");
        assert_eq!(steam_id, FluidId(3));
    }

    #[test]
    fn registry_get_def() {
        let reg = FluidRegistry::from_defs(test_defs());
        let water = reg.get(FluidId(1));
        assert_eq!(water.id, "water");
        assert!(!water.is_gas);

        let steam = reg.get(FluidId(3));
        assert_eq!(steam.id, "steam");
        assert!(steam.is_gas);
    }

    #[test]
    fn registry_try_by_name_returns_none_for_unknown() {
        let reg = FluidRegistry::from_defs(test_defs());
        assert!(reg.try_by_name("unknown").is_none());
    }

    #[test]
    #[should_panic(expected = "Cannot get def for FluidId::NONE")]
    fn registry_get_none_panics() {
        let reg = FluidRegistry::from_defs(test_defs());
        reg.get(FluidId::NONE);
    }
}
