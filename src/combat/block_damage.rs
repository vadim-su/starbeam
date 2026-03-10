use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Resource, Default, Debug)]
pub struct BlockDamageMap {
    pub damage: HashMap<(i32, i32), BlockDamageState>,
}

#[derive(Debug)]
pub struct BlockDamageState {
    pub accumulated: f32,
    pub regen_timer: f32,
}

const REGEN_DELAY: f32 = 2.0;
const REGEN_RATE: f32 = 0.5;

pub fn tick_block_damage_regen(time: Res<Time>, mut damage_map: ResMut<BlockDamageMap>) {
    let dt = time.delta_secs();
    damage_map.damage.retain(|_pos, state| {
        state.regen_timer += dt;
        if state.regen_timer >= REGEN_DELAY {
            state.accumulated -= REGEN_RATE * dt;
        }
        state.accumulated > 0.0
    });
}
