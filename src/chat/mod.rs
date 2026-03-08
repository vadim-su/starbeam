use bevy::prelude::*;

use crate::registry::AppState;
use crate::ui::game_ui::UiTheme;

/// Category of a chat message, determines prefix and color.
#[derive(Debug, Clone)]
pub enum MessageCategory {
    System,
    Dialog { speaker: String },
    PlayerCommand,
    PlayerChat,
}

/// A single chat message.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub text: String,
    pub category: MessageCategory,
    pub timestamp: f64,
}

/// Event fired when the player enters a `/command`.
#[derive(Message, Debug, Clone)]
pub struct ChatCommandEvent {
    pub command: String,
    pub args: Vec<String>,
}

/// Global chat state resource.
#[derive(Resource)]
pub struct ChatState {
    pub messages: Vec<ChatMessage>,
    pub is_active: bool,
    pub input_buffer: String,
    pub scroll_offset: i32,
    pub max_messages: usize,
}

impl ChatState {
    pub fn new(max_messages: usize) -> Self {
        Self {
            messages: Vec::new(),
            is_active: false,
            input_buffer: String::new(),
            scroll_offset: 0,
            max_messages,
        }
    }

    /// Add a system message.
    pub fn send_system(&mut self, text: &str, time: f64) {
        self.push(text.to_string(), MessageCategory::System, time);
    }

    /// Add an NPC dialog message.
    pub fn send_dialog(&mut self, speaker: &str, text: &str, time: f64) {
        self.push(
            text.to_string(),
            MessageCategory::Dialog {
                speaker: speaker.to_string(),
            },
            time,
        );
    }

    pub fn push(&mut self, text: String, category: MessageCategory, timestamp: f64) {
        self.messages.push(ChatMessage {
            text,
            category,
            timestamp,
        });
        if self.messages.len() > self.max_messages {
            self.messages.remove(0);
        }
        // Reset scroll to bottom on new message
        self.scroll_offset = 0;
    }
}

fn init_chat_state(mut commands: Commands, theme: Res<UiTheme>) {
    commands.insert_resource(ChatState::new(theme.chat.max_messages));
}

pub struct ChatPlugin;

impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ChatCommandEvent>()
            .add_systems(OnEnter(AppState::InGame), init_chat_state);
    }
}
