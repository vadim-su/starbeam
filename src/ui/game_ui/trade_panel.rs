//! Trading panel UI — shows trade offers from a nearby trader.
//!
//! Spawned/despawned reactively based on `OpenTrader` resource.
//! Uses the unified window system for dragging, close button and ESC-close.

use bevy::picking::prelude::*;
use bevy::prelude::*;

use crate::inventory::components::BagTarget;
use crate::inventory::Inventory;
use crate::item::definition::ItemType;
use crate::item::ItemRegistry;
use crate::player::Player;
use crate::registry::AppState;
use crate::trader::{OpenTrader, TradeOffers};

use super::theme::UiTheme;
use super::window::{self, GameWindow, WindowConfig};

// ── Marker components ──

/// Root entity for the entire trade panel.
#[derive(Component)]
pub struct TradePanelRoot;

/// Container holding the list of trade offer rows.
#[derive(Component)]
pub struct TradeOfferList;

/// A clickable "Trade" button linked to a specific offer index.
#[derive(Component)]
pub struct TradeButton {
    pub offer_index: usize,
}

// ── Plugin ──

pub struct TradeUiPlugin;

impl Plugin for TradeUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                // Phase 1: spawn / despawn the panel root (deferred commands).
                manage_trade_panel,
                // Flush so the panel entities are available to later systems.
                ApplyDeferred,
                // Phase 2: populate / refresh panel contents + handle input.
                (update_trade_offers, handle_trade_button),
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    }
}

// ── Constants ──

const PANEL_WIDTH: f32 = 420.0;
const PANEL_HEIGHT: f32 = 320.0;
const PANEL_PADDING: f32 = 12.0;

// ── Systems ──

/// Spawn or despawn the trade panel based on `OpenTrader`.
fn manage_trade_panel(
    mut commands: Commands,
    open_trader: Res<OpenTrader>,
    panel_query: Query<Entity, With<TradePanelRoot>>,
    theme: Res<UiTheme>,
    asset_server: Res<AssetServer>,
) {
    let should_be_open = open_trader.0.is_some();
    let panel_exists = !panel_query.is_empty();

    if should_be_open && !panel_exists {
        spawn_trade_panel(&mut commands, &theme, &asset_server);
    } else if !should_be_open && panel_exists {
        for entity in &panel_query {
            commands.entity(entity).despawn();
        }
    }
}

/// Update the trade offer list when the panel is visible.
fn update_trade_offers(
    mut commands: Commands,
    open_trader: Res<OpenTrader>,
    offers_query: Query<&TradeOffers>,
    player_query: Query<Ref<Inventory>, With<Player>>,
    list_query: Query<(Entity, Option<&Children>), With<TradeOfferList>>,
    theme: Res<UiTheme>,
) {
    let Some(trader_entity) = open_trader.0 else {
        return;
    };

    let Ok((list_entity, children)) = list_query.single() else {
        return;
    };

    let Ok(inventory_ref) = player_query.single() else {
        return;
    };

    // Update when resources change, inventory changes, OR container is empty (just spawned).
    let is_empty = children.is_none_or(|c| c.is_empty());
    if !is_empty && !open_trader.is_changed() && !inventory_ref.is_changed() {
        return;
    }

    let Ok(trade_offers) = offers_query.get(trader_entity) else {
        return;
    };

    let inventory: &Inventory = &*inventory_ref;
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

    // Rebuild offer rows
    commands.entity(list_entity).with_children(|parent| {
        for (idx, offer) in trade_offers.offers.iter().enumerate() {
            // Check if player can afford this trade
            let can_afford = offer
                .cost
                .iter()
                .all(|(item_id, count)| inventory.count_item(item_id) >= *count as u32);

            // Build cost text
            let cost_text: Vec<String> = offer
                .cost
                .iter()
                .map(|(item_id, count)| format!("{}x {}", count, item_id))
                .collect();
            let cost_str = cost_text.join(" + ");

            // Build result text
            let result_str = format!("{}x {}", offer.result.1, offer.result.0);

            // Row container
            parent
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::SpaceBetween,
                        padding: UiRect::all(Val::Px(6.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        margin: UiRect::bottom(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(bg_medium),
                    BorderColor::all(border_color),
                    Pickable::IGNORE,
                ))
                .with_children(|row| {
                    // Offer description: "cost → result"
                    let offer_color = if can_afford { text_color } else { text_dim };

                    row.spawn((
                        Text::new(format!("{} -> {}", cost_str, result_str)),
                        TextFont {
                            font_size: 11.0,
                            ..default()
                        },
                        TextColor(offer_color),
                        Node {
                            flex_shrink: 1.0,
                            margin: UiRect::right(Val::Px(8.0)),
                            ..default()
                        },
                        Pickable::IGNORE,
                    ));

                    // Trade button
                    let btn_bg = if can_afford {
                        Color::srgb(0.2, 0.5, 0.2)
                    } else {
                        bg_medium
                    };
                    let btn_text_color = if can_afford { text_color } else { text_dim };

                    row.spawn((
                        TradeButton { offer_index: idx },
                        Button,
                        Node {
                            width: Val::Px(60.0),
                            height: Val::Px(24.0),
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
                        btn.spawn((
                            Text::new("Trade"),
                            TextFont {
                                font_size: 11.0,
                                ..default()
                            },
                            TextColor(btn_text_color),
                            Pickable::IGNORE,
                        ));
                    });
                });
        }
    });
}

/// Handle clicks on trade buttons — consume cost items, add result item.
fn handle_trade_button(
    trade_btn_query: Query<(&Interaction, &TradeButton), Changed<Interaction>>,
    open_trader: Res<OpenTrader>,
    offers_query: Query<&TradeOffers>,
    mut player_query: Query<&mut Inventory, With<Player>>,
    item_registry: Res<ItemRegistry>,
) {
    let Some(trader_entity) = open_trader.0 else {
        return;
    };

    let Ok(trade_offers) = offers_query.get(trader_entity) else {
        return;
    };

    let Ok(mut inventory) = player_query.single_mut() else {
        return;
    };

    for (interaction, trade_btn) in &trade_btn_query {
        if *interaction != Interaction::Pressed {
            continue;
        }

        let Some(offer) = trade_offers.offers.get(trade_btn.offer_index) else {
            continue;
        };

        // Verify player can afford
        let can_afford = offer
            .cost
            .iter()
            .all(|(item_id, count)| inventory.count_item(item_id) >= *count as u32);

        if !can_afford {
            continue;
        }

        // Consume cost items
        for (item_id, count) in &offer.cost {
            inventory.remove_item(item_id, *count);
        }

        // Add result item
        let (result_id, result_count) = &offer.result;
        let (target, max_stack) = item_registry
            .by_name(result_id)
            .map(|id| {
                let def = item_registry.get(id);
                let target = match def.item_type {
                    ItemType::Block | ItemType::Material => BagTarget::Material,
                    _ => BagTarget::Main,
                };
                (target, def.max_stack)
            })
            .unwrap_or((BagTarget::Main, 99));

        inventory.try_add_item(result_id, *result_count, max_stack, target);
    }
}

// ── Spawn helpers ──

/// Spawn the trade panel UI hierarchy using the unified window frame.
fn spawn_trade_panel(commands: &mut Commands, theme: &UiTheme, asset_server: &AssetServer) {
    let colors = &theme.colors;
    let text_dim = Color::from(colors.text_dim.clone());

    // Spawn unified window frame.
    let entities = window::spawn_window_frame(
        commands,
        theme,
        &WindowConfig {
            title: "Trader",
            width: PANEL_WIDTH,
            height: PANEL_HEIGHT,
            padding: PANEL_PADDING,
        },
        GameWindow::Trading,
        asset_server,
    );

    // Mark the root so existing systems can find it.
    commands.entity(entities.root).insert(TradePanelRoot);

    // Configure the body layout.
    commands.entity(entities.body).insert(Node {
        flex_direction: FlexDirection::Column,
        flex_grow: 1.0,
        width: Val::Percent(100.0),
        overflow: Overflow::clip_y(),
        ..default()
    });

    // ── Body contents ──
    commands.entity(entities.body).with_children(|body| {
        // Header text
        body.spawn((
            Text::new("Available Trades:"),
            TextFont {
                font_size: 12.0,
                ..default()
            },
            TextColor(text_dim),
            Node {
                margin: UiRect::bottom(Val::Px(8.0)),
                ..default()
            },
            Pickable::IGNORE,
        ));

        // Scrollable offer list container
        body.spawn((
            TradeOfferList,
            Node {
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                ..default()
            },
            Pickable::IGNORE,
        ));
    });
}
