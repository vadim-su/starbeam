use bevy::prelude::*;

use super::Health;
use crate::player::Player;

#[derive(Message, Debug)]
pub struct PlayerDeathEvent;

pub fn detect_player_death(
    query: Query<&Health, With<Player>>,
    mut writer: bevy::ecs::message::MessageWriter<PlayerDeathEvent>,
) {
    for health in &query {
        if health.is_dead() {
            writer.write(PlayerDeathEvent);
        }
    }
}

pub fn handle_player_death(
    mut reader: bevy::ecs::message::MessageReader<PlayerDeathEvent>,
    mut query: Query<&mut Health, With<Player>>,
) {
    for _event in reader.read() {
        for mut health in &mut query {
            health.current = health.max;
        }
        warn!("Player died! Respawning...");
    }
}
