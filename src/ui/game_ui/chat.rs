use bevy::picking::prelude::*;
use bevy::prelude::*;

use super::theme::UiTheme;

/// Marker for the chat root container.
#[derive(Component)]
pub struct ChatRoot;

/// Marker for the messages scroll area.
#[derive(Component)]
pub struct ChatMessagesArea;

/// Marker for the input text field.
#[derive(Component)]
pub struct ChatInputLine;

/// Marker for a single chat message text entity.
#[derive(Component)]
pub struct ChatMessageText {
    pub index: usize,
}

/// Marker for the chat background (changes opacity based on active state).
#[derive(Component)]
pub struct ChatBackground;

pub fn spawn_chat(commands: &mut Commands, theme: &UiTheme) {
    let chat = &theme.chat;

    commands
        .spawn((
            ChatRoot,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(8.0),
                bottom: Val::Px(8.0),
                width: Val::Px(chat.width),
                height: Val::Px(chat.height),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexEnd,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|parent| {
            // Background
            parent.spawn((
                ChatBackground,
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
                Pickable::IGNORE,
            ));

            // Messages area (scrollable, fills available space)
            parent.spawn((
                ChatMessagesArea,
                Node {
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::FlexEnd,
                    flex_grow: 1.0,
                    overflow: Overflow::clip_y(),
                    padding: UiRect::all(Val::Px(4.0)),
                    ..default()
                },
                Pickable::IGNORE,
            ));

            // Input line (hidden by default)
            parent.spawn((
                ChatInputLine,
                Text::new(""),
                TextFont {
                    font_size: chat.font_size,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::NONE),
                Visibility::Hidden,
                Pickable::IGNORE,
            ));
        });
}
