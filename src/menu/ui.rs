use bevy::ecs::message::MessageWriter;
use bevy::prelude::*;

use super::MenuEntity;
use crate::registry::AppState;

/// Marker for the "New Game" button.
#[derive(Component)]
pub struct NewGameButton;

/// Marker for the "Exit" button.
#[derive(Component)]
pub struct ExitButton;

/// Colors from the Starbeam website CSS variables.
pub mod colors {
    use bevy::prelude::*;

    // --accent: #5cb8ff
    pub const ACCENT: Color = Color::srgb(0.361, 0.722, 1.0);
    // --accent-warm: #ff8c42
    pub const ACCENT_WARM: Color = Color::srgb(1.0, 0.549, 0.259);
    // --bg-deep: #06060e  (used for primary button text)
    pub const BG_DEEP: Color = Color::srgb(0.024, 0.024, 0.055);
    // --text: #e8e8f0
    pub const TEXT: Color = Color::srgb(0.910, 0.910, 0.941);
    // --text-dim: #8888aa
    pub const TEXT_DIM: Color = Color::srgb(0.533, 0.533, 0.667);

    // Primary button: background = --accent, hover = brighter, pressed = darker
    pub const BTN_PRIMARY: Color = ACCENT;
    pub const BTN_PRIMARY_HOVER: Color = Color::srgb(0.420, 0.770, 1.0);
    pub const BTN_PRIMARY_PRESSED: Color = Color::srgb(0.300, 0.640, 0.900);

    // Secondary button: border rgba(255,255,255,0.15), hover border = --accent
    pub const BTN_SECONDARY_BORDER: Color = Color::srgba(1.0, 1.0, 1.0, 0.15);
    pub const BTN_SECONDARY_HOVER_BG: Color = Color::srgba(0.361, 0.722, 1.0, 0.05);
    pub const BTN_SECONDARY_HOVER_BORDER: Color = ACCENT;
}

/// Spawn the complete menu UI layout.
pub fn spawn_menu_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/Silkscreen-Regular.ttf");
    let font_bold = asset_server.load("fonts/Silkscreen-Bold.ttf");

    // Root container — full screen, column layout, centered
    commands
        .spawn((
            MenuEntity,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::NONE),
            GlobalZIndex(1),
        ))
        .with_children(|parent| {
            // --- Title row: "STAR" + "BEAM" ---
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    margin: UiRect::bottom(Val::Px(60.0)),
                    ..default()
                })
                .with_children(|row| {
                    // "STAR" — accent blue #5cb8ff
                    row.spawn((
                        Text::new("STAR"),
                        TextFont {
                            font: font_bold.clone(),
                            font_size: 80.0,
                            ..default()
                        },
                        TextColor(colors::ACCENT),
                    ));
                    // "BEAM" — accent warm #ff8c42
                    row.spawn((
                        Text::new("BEAM"),
                        TextFont {
                            font: font_bold.clone(),
                            font_size: 80.0,
                            ..default()
                        },
                        TextColor(colors::ACCENT_WARM),
                    ));
                });

            // --- Buttons column (stacked like the screenshot) ---
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(16.0),
                    ..default()
                })
                .with_children(|col| {
                    // "NEW GAME" — primary filled button
                    // Website: bg = --accent, color = --bg-deep, font-family pixel, uppercase
                    col.spawn((
                        NewGameButton,
                        Button,
                        Node {
                            width: Val::Px(280.0),
                            height: Val::Px(56.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(colors::BTN_PRIMARY),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("NEW GAME"),
                            TextFont {
                                font: font.clone(),
                                font_size: 16.0,
                                ..default()
                            },
                            TextColor(colors::BG_DEEP),
                        ));
                    });

                    // "EXIT" — secondary outlined button
                    // Website: bg transparent, border 1px rgba(255,255,255,0.15), color --text
                    col.spawn((
                        ExitButton,
                        Button,
                        Node {
                            width: Val::Px(280.0),
                            height: Val::Px(56.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                        BorderColor::all(colors::BTN_SECONDARY_BORDER),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("EXIT"),
                            TextFont {
                                font: font.clone(),
                                font_size: 16.0,
                                ..default()
                            },
                            TextColor(colors::TEXT),
                        ));
                    });
                });
        });
}

/// Handle New Game button interaction.
pub fn handle_new_game_button(
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<NewGameButton>),
    >,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for (interaction, mut bg) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                *bg = BackgroundColor(colors::BTN_PRIMARY_PRESSED);
                next_state.set(AppState::Loading);
            }
            Interaction::Hovered => {
                *bg = BackgroundColor(colors::BTN_PRIMARY_HOVER);
            }
            Interaction::None => {
                *bg = BackgroundColor(colors::BTN_PRIMARY);
            }
        }
    }
}

/// Handle Exit button interaction.
/// Website hover: border-color -> --accent, bg -> rgba(92,184,255,0.05)
pub fn handle_exit_button(
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor, &mut BorderColor),
        (Changed<Interaction>, With<ExitButton>),
    >,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, mut bg, mut border) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                exit.write(AppExit::Success);
            }
            Interaction::Hovered => {
                *bg = BackgroundColor(colors::BTN_SECONDARY_HOVER_BG);
                *border = BorderColor::all(colors::BTN_SECONDARY_HOVER_BORDER);
            }
            Interaction::None => {
                *bg = BackgroundColor(Color::NONE);
                *border = BorderColor::all(colors::BTN_SECONDARY_BORDER);
            }
        }
    }
}
