//! Crafting panel UI — recipe list (left) + detail/progress panel (right).
//!
//! Spawned/despawned reactively based on `OpenStation` and `HandCraftOpen` resources.
//! Supports both station-based crafting and hand-crafting.
//! Uses the unified window system for dragging, close button and ESC-close.

use bevy::picking::prelude::*;
use bevy::prelude::*;

use crate::crafting::{
    ActiveCraft, CraftingStation, HandCraftState, RecipeRegistry, UnlockedRecipes,
};
use crate::interaction::interactable::{HandCraftOpen, OpenStation};
use crate::inventory::Inventory;
use crate::item::ItemRegistry;
use crate::player::Player;
use crate::registry::AppState;

use super::theme::UiTheme;
use super::window::{self, GameWindow, WindowConfig};

// ── Resources ──

/// Tracks which recipe is selected in the crafting panel.
#[derive(Resource, Default)]
pub struct CraftingUiState {
    pub selected_recipe_id: Option<String>,
}

// ── Marker components ──

/// Root entity for the entire crafting panel.
#[derive(Component)]
pub struct CraftingPanelRoot;

/// Container holding the recipe list buttons.
#[derive(Component)]
pub struct RecipeListContainer;

/// A clickable recipe button in the list.
#[derive(Component)]
pub struct RecipeButton {
    pub recipe_id: String,
}

/// Right-side detail panel.
#[derive(Component)]
pub struct DetailPanel;

/// Title text in the detail panel showing selected recipe name.
#[derive(Component)]
pub struct DetailTitle;

/// Container for ingredient rows in the detail panel.
#[derive(Component)]
pub struct IngredientList;

/// The fill portion of the progress bar.
#[derive(Component)]
pub struct ProgressBarFill;

/// The craft button.
#[derive(Component)]
pub struct CraftButton;

// ── Plugin ──

pub struct CraftingUiPlugin;

impl Plugin for CraftingUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CraftingUiState>().add_systems(
            Update,
            (
                // Phase 1: spawn / despawn the panel root (deferred commands).
                manage_crafting_panel,
                // Flush so the panel entities are available to later systems.
                ApplyDeferred,
                // Phase 2: populate / refresh panel contents + handle input.
                (
                    update_recipe_list,
                    update_detail_panel,
                    handle_craft_button_click,
                    handle_recipe_button_click,
                    update_progress_bar,
                ),
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    }
}

// ── Constants ──

const PANEL_WIDTH: f32 = 500.0;
const PANEL_HEIGHT: f32 = 350.0;
const PANEL_PADDING: f32 = 12.0;
const RECIPE_LIST_WIDTH: f32 = 180.0;
const PROGRESS_BAR_HEIGHT: f32 = 16.0;

// ── Systems ──

/// Spawn or despawn the crafting panel based on `OpenStation` and `HandCraftOpen`.
fn manage_crafting_panel(
    mut commands: Commands,
    open_station: Res<OpenStation>,
    hand_craft_open: Res<HandCraftOpen>,
    panel_query: Query<Entity, With<CraftingPanelRoot>>,
    station_query: Query<&CraftingStation>,
    theme: Res<UiTheme>,
    mut ui_state: ResMut<CraftingUiState>,
    asset_server: Res<AssetServer>,
) {
    let should_be_open = open_station.0.is_some() || hand_craft_open.0;
    let panel_exists = !panel_query.is_empty();

    if should_be_open && !panel_exists {
        // Determine title
        let title = if let Some(station_entity) = open_station.0 {
            if let Ok(station) = station_query.get(station_entity) {
                format_station_name(&station.station_id)
            } else {
                "Crafting Station".to_string()
            }
        } else {
            "Hand Crafting".to_string()
        };

        ui_state.selected_recipe_id = None;
        spawn_crafting_panel(&mut commands, &theme, &title, &asset_server);
    } else if !should_be_open && panel_exists {
        for entity in &panel_query {
            commands.entity(entity).despawn();
        }
        ui_state.selected_recipe_id = None;
    }
}

/// Update the recipe list when the panel is visible.
fn update_recipe_list(
    mut commands: Commands,
    open_station: Res<OpenStation>,
    hand_craft_open: Res<HandCraftOpen>,
    recipe_registry: Res<RecipeRegistry>,
    player_query: Query<(Ref<Inventory>, &UnlockedRecipes), With<Player>>,
    station_query: Query<&CraftingStation>,
    list_query: Query<(Entity, Option<&Children>), With<RecipeListContainer>>,
    ui_state: Res<CraftingUiState>,
    theme: Res<UiTheme>,
) {
    // Don't touch children if the panel is about to be despawned — the root
    // despawn already handles recursive cleanup and issuing duplicate despawn
    // commands on the same children produces "entity is invalid" warnings.
    let should_be_open = open_station.0.is_some() || hand_craft_open.0;
    if !should_be_open {
        return;
    }

    let Ok((list_entity, children)) = list_query.single() else {
        return;
    };

    let Ok((inventory_ref, unlocked)) = player_query.single() else {
        return;
    };

    // Update when resources change, inventory changes, OR container is empty (just spawned).
    let is_empty = children.is_none_or(|c| c.is_empty());
    if !is_empty
        && !open_station.is_changed()
        && !hand_craft_open.is_changed()
        && !ui_state.is_changed()
        && !inventory_ref.is_changed()
    {
        return;
    }

    let inventory: &Inventory = &*inventory_ref;

    // Determine station filter
    let station_id: Option<String> = if let Some(station_entity) = open_station.0 {
        station_query
            .get(station_entity)
            .ok()
            .map(|s| s.station_id.clone())
    } else if hand_craft_open.0 {
        None
    } else {
        return;
    };

    let recipes: Vec<&crate::crafting::Recipe> = recipe_registry
        .for_station(station_id.as_deref())
        .into_iter()
        .filter(|r| r.unlocked_by.is_unlocked(&unlocked.blueprints))
        .collect();
    let craftable: Vec<&str> = recipe_registry
        .craftable_recipes(station_id.as_deref(), inventory, &unlocked.blueprints)
        .iter()
        .map(|r| r.id.as_str())
        .collect();

    let colors = &theme.colors;
    let text_color = Color::from(colors.text.clone());
    let text_dim = Color::from(colors.text_dim.clone());
    let bg_medium = Color::from(colors.bg_medium.clone());
    let selected_color = Color::from(colors.selected.clone());
    let border_color = Color::from(colors.border.clone());

    // Clear existing children
    if let Some(children) = children {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }

    // Rebuild children
    commands.entity(list_entity).with_children(|parent| {
        for recipe in &recipes {
            let is_craftable = craftable.contains(&recipe.id.as_str());
            let is_selected = ui_state
                .selected_recipe_id
                .as_ref()
                .is_some_and(|id| id == &recipe.id);

            let label = format!("{} x{}", recipe.result.item_id, recipe.result.count);

            let btn_bg = if is_selected {
                Color::from(colors.border.clone())
            } else {
                bg_medium
            };
            let btn_text = if is_craftable { text_color } else { text_dim };
            let btn_border = if is_selected {
                selected_color
            } else {
                border_color
            };

            parent
                .spawn((
                    RecipeButton {
                        recipe_id: recipe.id.clone(),
                    },
                    Button,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(28.0),
                        padding: UiRect::horizontal(Val::Px(6.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(btn_bg),
                    BorderColor::all(btn_border),
                    Pickable {
                        should_block_lower: true,
                        is_hoverable: true,
                    },
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new(label),
                        TextFont {
                            font_size: 11.0,
                            ..default()
                        },
                        TextColor(btn_text),
                        Pickable::IGNORE,
                    ));
                });
        }
    });
}

/// Update the detail panel when a recipe is selected.
fn update_detail_panel(
    mut commands: Commands,
    ui_state: Res<CraftingUiState>,
    recipe_registry: Res<RecipeRegistry>,
    item_registry: Res<ItemRegistry>,
    player_query: Query<(Ref<Inventory>, Option<&HandCraftState>), With<Player>>,
    open_station: Res<OpenStation>,
    hand_craft_open: Res<HandCraftOpen>,
    station_query: Query<&CraftingStation>,
    detail_query: Query<(Entity, Option<&Children>), With<DetailPanel>>,
    theme: Res<UiTheme>,
) {
    // Don't touch children if the panel is about to be despawned (see update_recipe_list).
    let should_be_open = open_station.0.is_some() || hand_craft_open.0;
    if !should_be_open {
        return;
    }

    let Ok((inventory_ref, hand_craft_state)) = player_query.single() else {
        return;
    };

    if !ui_state.is_changed() && !open_station.is_changed() && !inventory_ref.is_changed() {
        return;
    }

    let Ok((detail_entity, children)) = detail_query.single() else {
        return;
    };

    let colors = &theme.colors;
    let text_color = Color::from(colors.text.clone());
    let text_dim = Color::from(colors.text_dim.clone());
    let bg_medium = Color::from(colors.bg_medium.clone());
    let border_color = Color::from(colors.border.clone());

    // Clear existing children
    if let Some(children) = children {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }

    let Some(ref recipe_id) = ui_state.selected_recipe_id else {
        // No recipe selected — show placeholder
        commands.entity(detail_entity).with_children(|parent| {
            parent.spawn((
                Text::new("Select a recipe"),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(text_dim),
                Pickable::IGNORE,
            ));
        });
        return;
    };

    let Some(recipe) = recipe_registry.get(recipe_id) else {
        return;
    };

    let inventory: &Inventory = &*inventory_ref;

    // Check if currently crafting
    let active_craft: Option<&ActiveCraft> = if let Some(station_entity) = open_station.0 {
        station_query
            .get(station_entity)
            .ok()
            .and_then(|s| s.active_craft.as_ref())
    } else {
        hand_craft_state.and_then(|h| h.active_craft.as_ref())
    };

    let is_crafting = active_craft.is_some();

    // Check if all ingredients are available
    let can_craft = !is_crafting
        && recipe
            .ingredients
            .iter()
            .all(|ing| inventory.count_item(&ing.item_id) >= ing.count as u32);

    // Get display name for result
    let result_display = item_registry
        .by_name(&recipe.result.item_id)
        .map(|id| item_registry.get(id).display_name.clone())
        .unwrap_or_else(|| recipe.result.item_id.clone());

    commands.entity(detail_entity).with_children(|parent| {
        // ── Result title ──
        parent.spawn((
            DetailTitle,
            Text::new(format!("{} x{}", result_display, recipe.result.count)),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(text_color),
            Node {
                margin: UiRect::bottom(Val::Px(8.0)),
                ..default()
            },
            Pickable::IGNORE,
        ));

        // ── Ingredients header ──
        parent.spawn((
            Text::new("Ingredients:"),
            TextFont {
                font_size: 11.0,
                ..default()
            },
            TextColor(text_dim),
            Node {
                margin: UiRect::bottom(Val::Px(4.0)),
                ..default()
            },
            Pickable::IGNORE,
        ));

        // ── Ingredient list ──
        parent
            .spawn((
                IngredientList,
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    margin: UiRect::bottom(Val::Px(8.0)),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|ing_parent| {
                for ingredient in &recipe.ingredients {
                    let have = inventory.count_item(&ingredient.item_id);
                    let need = ingredient.count as u32;
                    let enough = have >= need;

                    let ing_display = item_registry
                        .by_name(&ingredient.item_id)
                        .map(|id| item_registry.get(id).display_name.clone())
                        .unwrap_or_else(|| ingredient.item_id.clone());

                    let color = if enough {
                        Color::srgb(0.4, 0.9, 0.4) // green
                    } else {
                        Color::srgb(0.9, 0.4, 0.4) // red
                    };

                    let symbol = if enough { "+" } else { "-" };

                    ing_parent.spawn((
                        Text::new(format!(
                            " {} {}x {} ({}/{})",
                            symbol, ingredient.count, ing_display, have, need
                        )),
                        TextFont {
                            font_size: 11.0,
                            ..default()
                        },
                        TextColor(color),
                        Pickable::IGNORE,
                    ));
                }
            });

        // ── Progress bar ──
        let progress = active_craft.map(|c| c.progress()).unwrap_or(0.0);

        parent
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(PROGRESS_BAR_HEIGHT),
                    margin: UiRect::bottom(Val::Px(8.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(bg_medium),
                BorderColor::all(border_color),
                Pickable::IGNORE,
            ))
            .with_children(|bar_parent| {
                bar_parent.spawn((
                    ProgressBarFill,
                    Node {
                        width: Val::Percent(progress * 100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.3, 0.7, 0.3)),
                    Pickable::IGNORE,
                ));
            });

        // ── Craft button ──
        let btn_bg = if can_craft {
            Color::srgb(0.2, 0.5, 0.2)
        } else {
            Color::from(colors.bg_medium.clone())
        };
        let btn_text_color = if can_craft { text_color } else { text_dim };

        parent
            .spawn((
                CraftButton,
                Button,
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(30.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(btn_bg),
                BorderColor::all(border_color),
                Pickable {
                    should_block_lower: true,
                    is_hoverable: true,
                },
            ))
            .with_children(|btn| {
                let label = if is_crafting { "Crafting..." } else { "Craft" };
                btn.spawn((
                    Text::new(label),
                    TextFont {
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(btn_text_color),
                    Pickable::IGNORE,
                ));
            });
    });
}

/// Handle clicks on recipe buttons to select a recipe.
fn handle_recipe_button_click(
    interactions: Query<(&Interaction, &RecipeButton), Changed<Interaction>>,
    mut ui_state: ResMut<CraftingUiState>,
) {
    for (interaction, recipe_btn) in &interactions {
        if *interaction == Interaction::Pressed {
            ui_state.selected_recipe_id = Some(recipe_btn.recipe_id.clone());
        }
    }
}

/// Handle craft button click — consume ingredients and start crafting.
fn handle_craft_button_click(
    craft_btn_query: Query<&Interaction, (Changed<Interaction>, With<CraftButton>)>,
    ui_state: Res<CraftingUiState>,
    recipe_registry: Res<RecipeRegistry>,
    open_station: Res<OpenStation>,
    mut player_query: Query<(&mut Inventory, &mut HandCraftState), With<Player>>,
    mut station_query: Query<&mut CraftingStation>,
) {
    let Ok(interaction) = craft_btn_query.single() else {
        return;
    };

    if *interaction != Interaction::Pressed {
        return;
    }

    let Some(ref recipe_id) = ui_state.selected_recipe_id else {
        return;
    };

    let Some(recipe) = recipe_registry.get(recipe_id) else {
        return;
    };

    let Ok((mut inventory, mut hand_craft)) = player_query.single_mut() else {
        return;
    };

    // Check if station/hand is already crafting
    if let Some(station_entity) = open_station.0 {
        if let Ok(station) = station_query.get(station_entity) {
            if station.active_craft.is_some() {
                return; // Already crafting
            }
        }
    } else if hand_craft.active_craft.is_some() {
        return; // Already crafting
    }

    // Verify ingredients
    let has_all = recipe
        .ingredients
        .iter()
        .all(|ing| inventory.count_item(&ing.item_id) >= ing.count as u32);

    if !has_all {
        return;
    }

    // Consume ingredients
    for ingredient in &recipe.ingredients {
        inventory.remove_item(&ingredient.item_id, ingredient.count);
    }

    // Start crafting
    let active_craft = ActiveCraft::new(recipe);

    if let Some(station_entity) = open_station.0 {
        if let Ok(mut station) = station_query.get_mut(station_entity) {
            station.active_craft = Some(active_craft);
        }
    } else {
        hand_craft.active_craft = Some(active_craft);
    }
}

/// Update progress bar fill width each frame.
fn update_progress_bar(
    open_station: Res<OpenStation>,
    station_query: Query<&CraftingStation>,
    player_query: Query<&HandCraftState, With<Player>>,
    mut fill_query: Query<&mut Node, With<ProgressBarFill>>,
) {
    let Ok(mut fill_node) = fill_query.single_mut() else {
        return;
    };

    let progress = if let Some(station_entity) = open_station.0 {
        station_query
            .get(station_entity)
            .ok()
            .and_then(|s| s.active_craft.as_ref())
            .map(|c| c.progress())
            .unwrap_or(0.0)
    } else {
        player_query
            .single()
            .ok()
            .and_then(|h| h.active_craft.as_ref())
            .map(|c| c.progress())
            .unwrap_or(0.0)
    };

    fill_node.width = Val::Percent(progress * 100.0);
}

// ── Spawn helpers ──

/// Format a station_id into a display name (e.g. "workbench" -> "Workbench").
fn format_station_name(station_id: &str) -> String {
    let mut chars = station_id.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Spawn the crafting panel UI hierarchy using the unified window frame.
fn spawn_crafting_panel(commands: &mut Commands, theme: &UiTheme, title: &str, asset_server: &AssetServer) {
    let colors = &theme.colors;
    let bg_medium = Color::from(colors.bg_medium.clone());
    let border_color = Color::from(colors.border.clone());
    let text_dim = Color::from(colors.text_dim.clone());

    // Spawn unified window frame.
    let entities = window::spawn_window_frame(
        commands,
        theme,
        &WindowConfig {
            title,
            width: PANEL_WIDTH,
            height: PANEL_HEIGHT,
            padding: PANEL_PADDING,
        },
        GameWindow::Crafting,
        asset_server,
    );

    // Mark the root so existing systems can find it.
    commands.entity(entities.root).insert(CraftingPanelRoot);

    // Configure the body layout.
    commands.entity(entities.body).insert(Node {
        flex_direction: FlexDirection::Row,
        column_gap: Val::Px(8.0),
        flex_grow: 1.0,
        width: Val::Percent(100.0),
        ..default()
    });

    // ── Body contents ──
    commands.entity(entities.body).with_children(|body| {
        // ── Left: Recipe list ──
        body.spawn((
            Node {
                width: Val::Px(RECIPE_LIST_WIDTH),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                border: UiRect::all(Val::Px(1.0)),
                padding: UiRect::all(Val::Px(4.0)),
                overflow: Overflow::clip_y(),
                ..default()
            },
            BackgroundColor(bg_medium),
            BorderColor::all(border_color),
            Pickable::IGNORE,
        ))
        .with_children(|list_wrapper| {
            // Header
            list_wrapper.spawn((
                Text::new("Recipes"),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(text_dim),
                Node {
                    margin: UiRect::bottom(Val::Px(4.0)),
                    ..default()
                },
                Pickable::IGNORE,
            ));

            // Scrollable recipe list container
            list_wrapper.spawn((
                RecipeListContainer,
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    width: Val::Percent(100.0),
                    flex_grow: 1.0,
                    ..default()
                },
                Pickable::IGNORE,
            ));
        });

        // ── Right: Detail panel ──
        body.spawn((
            DetailPanel,
            Node {
                flex_grow: 1.0,
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                border: UiRect::all(Val::Px(1.0)),
                padding: UiRect::all(Val::Px(8.0)),
                overflow: Overflow::clip_y(),
                ..default()
            },
            BackgroundColor(bg_medium),
            BorderColor::all(border_color),
            Pickable::IGNORE,
        ))
        .with_children(|detail| {
            // Placeholder text
            detail.spawn((
                Text::new("Select a recipe"),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(text_dim),
                Pickable::IGNORE,
            ));
        });
    });
}
