use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::data::LiquidId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidReaction {
    pub other: String,
    pub produce_tile: Option<String>,
    pub consume_both: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidDef {
    pub name: String,
    pub density: f32,
    pub viscosity: f32,
    pub color: [f32; 4],
    pub damage_on_contact: f32,
    pub light_emission: [u8; 3],
    pub light_opacity: u8,
    pub swim_speed_factor: f32,
    #[serde(default)]
    pub reactions: Vec<LiquidReaction>,
}

#[derive(Resource, Default)]
pub struct LiquidRegistry {
    pub defs: Vec<LiquidDef>,
    name_to_id: HashMap<String, LiquidId>,
    reaction_cache: HashMap<(u8, u8), usize>,
}

impl LiquidRegistry {
    pub fn from_defs(defs: Vec<LiquidDef>) -> Self {
        let mut name_to_id = HashMap::new();
        for (i, def) in defs.iter().enumerate() {
            name_to_id.insert(def.name.clone(), LiquidId((i + 1) as u8));
        }

        let mut reaction_cache = HashMap::new();
        for (i, def) in defs.iter().enumerate() {
            let a = (i + 1) as u8;
            for (ri, reaction) in def.reactions.iter().enumerate() {
                if let Some(&b_id) = name_to_id.get(&reaction.other) {
                    reaction_cache.insert((a, b_id.0), ri);
                }
            }
        }

        Self {
            defs,
            name_to_id,
            reaction_cache,
        }
    }

    pub fn get(&self, id: LiquidId) -> Option<&LiquidDef> {
        if id.is_none() {
            return None;
        }
        self.defs.get((id.0 - 1) as usize)
    }

    pub fn by_name(&self, name: &str) -> LiquidId {
        self.name_to_id.get(name).copied().unwrap_or(LiquidId::NONE)
    }

    pub fn density(&self, id: LiquidId) -> f32 {
        self.get(id).map(|d| d.density).unwrap_or(0.0)
    }

    pub fn viscosity(&self, id: LiquidId) -> f32 {
        self.get(id).map(|d| d.viscosity).unwrap_or(1.0)
    }

    pub fn get_reaction(&self, a: LiquidId, b: LiquidId) -> Option<&LiquidReaction> {
        let idx = self.reaction_cache.get(&(a.0, b.0))?;
        let def = self.get(a)?;
        def.reactions.get(*idx)
    }
}
