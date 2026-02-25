use bevy::prelude::*;

use crate::player::Player;
use crate::registry::world::WorldConfig;

#[derive(Component)]
pub struct DebugHudText;

pub fn spawn_debug_hud(mut commands: Commands) {
    commands.spawn((
        DebugHudText,
        Text::new("X: 0.0 Y: 0.0 (tile 0, 0)"),
        TextFont {
            font_size: 18.0,
            ..default()
        },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.8)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(10.0),
            top: Val::Px(10.0),
            ..default()
        },
    ));
}

pub fn update_debug_hud(
    player_query: Query<&Transform, With<Player>>,
    mut text_query: Query<&mut Text, With<DebugHudText>>,
    world_config: Res<WorldConfig>,
) {
    let Ok(player_tf) = player_query.single() else {
        return;
    };
    let Ok(mut text) = text_query.single_mut() else {
        return;
    };

    let px = player_tf.translation.x;
    let py = player_tf.translation.y;
    let tx = (px / world_config.tile_size).floor() as i32;
    let ty = (py / world_config.tile_size).floor() as i32;

    **text = format!("X: {px:.0} Y: {py:.0} (tile {tx}, {ty})");
}
