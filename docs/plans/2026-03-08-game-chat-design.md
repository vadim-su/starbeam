# Game Chat Design

## Overview

In-game chat panel in the bottom-right corner for displaying system messages, NPC dialog, and player commands. Supports transparent idle mode with fading messages and active input mode with scrolling.

## Data Layer (`src/chat/mod.rs`)

### Resources & Components

- `ChatState` resource: `messages: Vec<ChatMessage>`, `is_active: bool`, `input_buffer: String`, `scroll_offset: usize`
- `ChatMessage`: `text: String`, `category: MessageCategory`, `timestamp: f64`
- `MessageCategory` enum: `System`, `Dialog { speaker: String }`, `PlayerCommand`, `PlayerChat`
- `ChatCommandEvent` event: `command: String`, `args: Vec<String>`

### API

```rust
chat_state.send_system("You picked up a stone");
chat_state.send_dialog("Merchant", "Hello, traveler!");
```

## UI Layer (`src/ui/game_ui/chat.rs`)

- Container anchored bottom-right
- Message area with scroll support (Bevy ScrollPosition)
- Input line (visible only in active mode)
- Prefix formatting: `[System]`, `[NPC Name]`, `[You]`

## Configuration (`ui.theme.ron`)

```ron
chat: (
    max_messages: 100,
    visible_lines: 5,
    fade_delay_secs: 5.0,
    fade_duration_secs: 1.0,
    font_size: 14.0,
    width: 400.0,
    height: 200.0,
    system_color: "#aaaaaa",
    dialog_color: "#ffcc00",
    command_color: "#88ff88",
    player_color: "#ffffff",
    input_bg_color: "#000000aa",
    active_bg_color: "#00000088",
)
```

## Modes

### Inactive (default)
- Transparent background
- Last 5 messages visible, fade after 5 seconds
- No input capture

### Active (Enter key)
- Semi-transparent background, all messages visible with scrolling
- Input line active, all keyboard input captured by chat
- Enter sends message (if starts with `/` → parse command, emit `ChatCommandEvent`), Esc closes
- Player movement and interaction keys blocked

## Input Blocking

When chat is active, add `chat_state.is_active` guard to movement/interaction systems (same pattern as inventory window checks).

## Message Flow

1. Any system calls `chat_state.send_*()` methods
2. Message added to `messages` vec with timestamp
3. UI system detects change, spawns/updates message text entities
4. In inactive mode: fade system adjusts opacity based on elapsed time
5. In active mode: all messages visible, scroll offset controls viewport
6. Commands starting with `/` parsed into `ChatCommandEvent` for other systems to handle
