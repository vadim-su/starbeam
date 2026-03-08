use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::MouseWheel;
use bevy::input::ButtonState;
use bevy::picking::prelude::*;
use bevy::prelude::*;

use crate::chat::{ChatCommandEvent, ChatState, MessageCategory};


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

/// Handles keyboard input for the chat: opening, typing, submitting, and closing.
pub fn chat_input_system(
    mut chat_state: ResMut<ChatState>,
    mut keyboard_events: MessageReader<KeyboardInput>,
    mut input_query: Query<
        (&mut Text, &mut Visibility, &mut BackgroundColor),
        With<ChatInputLine>,
    >,
    mut bg_query: Query<&mut BackgroundColor, (With<ChatBackground>, Without<ChatInputLine>)>,
    theme: Res<UiTheme>,
    time: Res<Time>,
    mut cmd_events: MessageWriter<ChatCommandEvent>,
) {
    let events: Vec<KeyboardInput> = keyboard_events.read().cloned().collect();

    for event in &events {
        if event.state != ButtonState::Pressed {
            continue;
        }

        if !chat_state.is_active {
            // Enter opens chat
            if event.key_code == KeyCode::Enter {
                chat_state.is_active = true;

                for (mut _text, mut vis, mut bg) in &mut input_query {
                    *vis = Visibility::Visible;
                    let input_bg: Color = theme.chat.input_bg_color.clone().into();
                    *bg = BackgroundColor(input_bg);
                }
                for mut bg in &mut bg_query {
                    let active_bg: Color = theme.chat.active_bg_color.clone().into();
                    *bg = BackgroundColor(active_bg);
                }
            }
            continue;
        }

        // Chat is active
        match event.key_code {
            KeyCode::Enter => {
                let buffer = chat_state.input_buffer.trim().to_string();
                let now = time.elapsed_secs_f64();

                if !buffer.is_empty() {
                    if buffer.starts_with('/') {
                        // Parse command
                        let parts: Vec<&str> = buffer[1..].split_whitespace().collect();
                        if let Some((&cmd, args)) = parts.split_first() {
                            cmd_events.write(ChatCommandEvent {
                                command: cmd.to_string(),
                                args: args.iter().map(|s| s.to_string()).collect(),
                            });
                        }
                        chat_state.push(
                            buffer.clone(),
                            MessageCategory::PlayerCommand,
                            now,
                        );
                    } else {
                        chat_state.push(buffer.clone(), MessageCategory::PlayerChat, now);
                    }
                }

                // Deactivate
                deactivate_chat(
                    &mut chat_state,
                    &mut input_query,
                    &mut bg_query,
                );
            }
            KeyCode::Escape => {
                deactivate_chat(
                    &mut chat_state,
                    &mut input_query,
                    &mut bg_query,
                );
            }
            KeyCode::Backspace => {
                chat_state.input_buffer.pop();
            }
            KeyCode::Space => {
                chat_state.input_buffer.push(' ');
            }
            _ => {
                if let Key::Character(ref ch) = event.logical_key {
                    chat_state.input_buffer.push_str(ch.as_str());
                }
            }
        }
    }

    // Update input line text
    if chat_state.is_active {
        for (mut text, _, _) in &mut input_query {
            **text = format!("> {}_", chat_state.input_buffer);
        }
    }
}

fn deactivate_chat(
    chat_state: &mut ResMut<ChatState>,
    input_query: &mut Query<
        (&mut Text, &mut Visibility, &mut BackgroundColor),
        With<ChatInputLine>,
    >,
    bg_query: &mut Query<&mut BackgroundColor, (With<ChatBackground>, Without<ChatInputLine>)>,
) {
    chat_state.is_active = false;
    chat_state.input_buffer.clear();

    for (mut text, mut vis, mut bg) in input_query.iter_mut() {
        **text = String::new();
        *vis = Visibility::Hidden;
        *bg = BackgroundColor(Color::NONE);
    }
    for mut bg in bg_query.iter_mut() {
        *bg = BackgroundColor(Color::NONE);
    }
}

/// Handles mouse wheel scrolling when chat is active.
pub fn chat_scroll_system(
    mut chat_state: ResMut<ChatState>,
    mut scroll_events: MessageReader<MouseWheel>,
) {
    if !chat_state.is_active {
        return;
    }

    for event in scroll_events.read() {
        let delta = event.y.signum() as i32;
        chat_state.scroll_offset = (chat_state.scroll_offset + delta)
            .max(0)
            .min(chat_state.messages.len().saturating_sub(1) as i32);
    }
}

/// Renders chat messages each frame, applying fade for inactive mode.
pub fn chat_render_messages(
    mut commands: Commands,
    existing: Query<Entity, With<ChatMessageText>>,
    messages_area: Query<Entity, With<ChatMessagesArea>>,
    chat_state: Res<ChatState>,
    theme: Res<UiTheme>,
    time: Res<Time>,
) {
    // Despawn all existing message text entities
    for entity in &existing {
        commands.entity(entity).despawn();
    }

    let Ok(area_entity) = messages_area.single() else {
        return;
    };

    let chat = &theme.chat;
    let now = time.elapsed_secs_f64();

    // Determine which messages to show
    let messages = &chat_state.messages;
    let slice = if chat_state.is_active {
        // Show all (respecting scroll_offset)
        let end = (messages.len() as i32 - chat_state.scroll_offset).max(0) as usize;
        &messages[..end]
    } else {
        // Show only last visible_lines messages
        let start = messages.len().saturating_sub(chat.visible_lines);
        &messages[start..]
    };

    for (i, msg) in slice.iter().enumerate() {
        // Build prefix
        let prefix = match &msg.category {
            MessageCategory::System => "[System] ".to_string(),
            MessageCategory::Dialog { speaker } => format!("[{}] ", speaker),
            MessageCategory::PlayerCommand => "[Cmd] ".to_string(),
            MessageCategory::PlayerChat => "[You] ".to_string(),
        };

        // Get color from theme
        let base_color: Color = match &msg.category {
            MessageCategory::System => chat.system_color.clone().into(),
            MessageCategory::Dialog { .. } => chat.dialog_color.clone().into(),
            MessageCategory::PlayerCommand => chat.command_color.clone().into(),
            MessageCategory::PlayerChat => chat.player_color.clone().into(),
        };

        // Calculate alpha
        let alpha = if chat_state.is_active {
            1.0_f32
        } else {
            let age = (now - msg.timestamp) as f32;
            if age < chat.fade_delay_secs {
                1.0
            } else {
                (1.0 - (age - chat.fade_delay_secs) / chat.fade_duration_secs).clamp(0.0, 1.0)
            }
        };

        // Skip fully faded messages in inactive mode
        if !chat_state.is_active && alpha <= 0.0 {
            continue;
        }

        let display_text = format!("{}{}", prefix, msg.text);
        let color = base_color.with_alpha(alpha);

        let child = commands
            .spawn((
                ChatMessageText { index: i },
                Text::new(display_text),
                TextFont {
                    font_size: chat.font_size,
                    ..default()
                },
                TextColor(color),
                Pickable::IGNORE,
            ))
            .id();

        commands.entity(area_entity).add_child(child);
    }
}
