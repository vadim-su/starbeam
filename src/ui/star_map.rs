//! Star-map panel — shows the current system's bodies and allows warping.
//!
//! Toggle with **F4**. Displays the star, all orbiting bodies with their type
//! and size, highlights the current planet, and provides "Warp" buttons.
//!
//! When `AutopilotMode` is active (opened via autopilot console), shows fuel
//! costs and "Navigate" buttons instead of "Warp" buttons.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::cosmos::address::CelestialAddress;
use crate::cosmos::current::CurrentSystem;
use crate::cosmos::fuel;
use crate::cosmos::ship_location::{ShipLocation, ShipManifest};
use crate::cosmos::warp::WarpToBody;
use crate::registry::world::ActiveWorld;
use bevy::ecs::message::MessageWriter;

/// Tracks star-map panel visibility.
#[derive(Resource, Default)]
pub struct StarMapState {
    pub visible: bool,
}

/// When true, the star map shows autopilot controls (fuel costs, Navigate
/// buttons) instead of instant Warp buttons.
#[derive(Resource, Default)]
pub struct AutopilotMode(pub bool);

/// Message requesting autopilot navigation to a specific orbit.
/// Handled by `handle_navigate` in `ship_location.rs`.
#[derive(Message, Debug)]
pub struct NavigateToBody {
    pub orbit: u32,
}

/// Toggles star-map panel on F4 press.
pub fn toggle_star_map(keyboard: Res<ButtonInput<KeyCode>>, mut state: ResMut<StarMapState>) {
    if keyboard.just_pressed(KeyCode::F4) {
        state.visible = !state.visible;
    }
}

/// Draws the star-map panel.
#[allow(clippy::too_many_arguments)]
pub fn draw_star_map(
    mut contexts: EguiContexts,
    mut state: ResMut<StarMapState>,
    mut autopilot_mode: ResMut<AutopilotMode>,
    current_system: Option<Res<CurrentSystem>>,
    active_world: Option<Res<ActiveWorld>>,
    ship_manifest: Option<Res<ShipManifest>>,
    mut warp_events: MessageWriter<WarpToBody>,
    mut navigate_events: MessageWriter<NavigateToBody>,
) -> Result {
    if !state.visible {
        return Ok(());
    }
    let Some(current_system) = current_system else {
        return Ok(());
    };
    let Some(active_world) = active_world else {
        return Ok(());
    };

    let ctx = contexts.ctx_mut()?;

    let is_autopilot = autopilot_mode.0;
    let is_on_ship = matches!(active_world.address, CelestialAddress::Ship { .. });

    // Read active ship data from manifest
    let active_ship = ship_manifest.as_ref().and_then(|m| m.active());

    // Determine the "current orbit" — for ships, use ShipLocation from manifest.
    let current_orbit = if is_on_ship {
        match active_ship.map(|s| &s.location) {
            Some(ShipLocation::Orbit(addr)) => addr.orbit().unwrap_or(0),
            Some(ShipLocation::InTransit { .. }) => 0,
            None => 0,
        }
    } else {
        active_world.address.orbit().unwrap_or(0)
    };

    let in_transit = matches!(
        active_ship.map(|s| &s.location),
        Some(ShipLocation::InTransit { .. })
    );

    let panel_frame = egui::Frame::NONE
        .fill(egui::Color32::from_rgba_unmultiplied(15, 15, 30, 220))
        .inner_margin(egui::Margin::same(12))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(80)));

    let title = if is_autopilot {
        "Autopilot Navigation"
    } else {
        "Star System"
    };

    egui::Window::new(title)
        .default_width(340.0)
        .resizable(false)
        .collapsible(true)
        .frame(panel_frame)
        .show(ctx, |ui| {
            let system = &current_system.system;

            // --- Star info ---
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("★ {}", system.star.type_id))
                        .color(egui::Color32::from_rgb(255, 220, 100))
                        .strong()
                        .size(16.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Luminosity:");
                ui.monospace(format!("{:.2}", system.star.luminosity));
                ui.separator();
                ui.label("Orbits:");
                ui.monospace(format!("{}", system.star.orbit_count));
            });

            // --- Fuel display (autopilot mode) ---
            if is_autopilot {
                ui.separator();
                if let Some(ref ship) = active_ship {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Fuel:")
                                .color(egui::Color32::from_rgb(255, 180, 60))
                                .strong(),
                        );
                        let fuel_pct = ship.fuel.current / ship.fuel.capacity;
                        let fuel_color = if fuel_pct > 0.5 {
                            egui::Color32::from_rgb(80, 255, 80)
                        } else if fuel_pct > 0.2 {
                            egui::Color32::from_rgb(255, 200, 60)
                        } else {
                            egui::Color32::from_rgb(255, 80, 80)
                        };
                        ui.label(
                            egui::RichText::new(format!(
                                "{:.0} / {:.0}",
                                ship.fuel.current, ship.fuel.capacity
                            ))
                            .color(fuel_color)
                            .monospace(),
                        );
                    });
                }

                if in_transit {
                    if let Some(ShipLocation::InTransit { progress, to, .. }) =
                        active_ship.map(|s| &s.location)
                    {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "In transit to orbit {} — {:.0}%",
                                    to.orbit().unwrap_or(0),
                                    progress * 100.0
                                ))
                                .color(egui::Color32::from_rgb(100, 180, 255)),
                            );
                        });
                    }
                }
            }

            ui.separator();
            ui.label(egui::RichText::new("Planets").strong().size(14.0));
            ui.add_space(4.0);

            // --- Body list ---
            for body in &system.bodies {
                let body_orbit = body.address.orbit().unwrap_or(0);
                let is_current = body_orbit == current_orbit && !in_transit;

                let type_color = match body.planet_type_id.as_str() {
                    "garden" => egui::Color32::from_rgb(100, 200, 100),
                    "barren" => egui::Color32::from_rgb(180, 150, 120),
                    _ => egui::Color32::from_rgb(160, 160, 200),
                };

                ui.horizontal(|ui| {
                    // Orbit number
                    ui.label(
                        egui::RichText::new(format!("#{}", body_orbit))
                            .monospace()
                            .color(egui::Color32::from_gray(140)),
                    );

                    // Planet type
                    let label = egui::RichText::new(&body.planet_type_id)
                        .color(type_color)
                        .strong();
                    ui.label(label);

                    // Size
                    ui.label(
                        egui::RichText::new(format!("{}x{}", body.width_tiles, body.height_tiles))
                            .monospace()
                            .color(egui::Color32::from_gray(120)),
                    );

                    // Right-side: current marker or action button
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if is_current {
                            if is_autopilot {
                                ui.label(
                                    egui::RichText::new("ORBITING")
                                        .color(egui::Color32::from_rgb(80, 255, 80))
                                        .strong(),
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new("● HERE")
                                        .color(egui::Color32::from_rgb(80, 255, 80))
                                        .strong(),
                                );
                            }
                        } else if in_transit {
                            ui.label(
                                egui::RichText::new("—").color(egui::Color32::from_gray(80)),
                            );
                        } else if is_autopilot {
                            // Autopilot mode: show fuel cost and Navigate button
                            let cost = fuel::fuel_cost(current_orbit, body_orbit);
                            let has_fuel = active_ship
                                .map(|s| s.fuel.current >= cost)
                                .unwrap_or(false);

                            let cost_text = format!("{:.0}F", cost);
                            ui.label(
                                egui::RichText::new(&cost_text).monospace().color(
                                    if has_fuel {
                                        egui::Color32::from_rgb(255, 200, 60)
                                    } else {
                                        egui::Color32::from_rgb(255, 80, 80)
                                    },
                                ),
                            );

                            if has_fuel {
                                if ui
                                    .button(
                                        egui::RichText::new("Navigate")
                                            .color(egui::Color32::from_rgb(100, 180, 255)),
                                    )
                                    .clicked()
                                {
                                    navigate_events.write(NavigateToBody { orbit: body_orbit });
                                    state.visible = false;
                                    autopilot_mode.0 = false;
                                }
                            } else {
                                ui.add_enabled(
                                    false,
                                    egui::Button::new(
                                        egui::RichText::new("Navigate")
                                            .color(egui::Color32::from_gray(100)),
                                    ),
                                );
                            }
                        } else if is_on_ship {
                            // On ship but not autopilot mode (F4) — no warp
                            ui.label(
                                egui::RichText::new("use autopilot")
                                    .color(egui::Color32::from_gray(80))
                                    .small(),
                            );
                        } else {
                            // Normal mode: Warp button
                            if ui
                                .button(
                                    egui::RichText::new("Warp")
                                        .color(egui::Color32::from_rgb(100, 180, 255)),
                                )
                                .clicked()
                            {
                                warp_events.write(WarpToBody { orbit: body_orbit });
                            }
                        }
                    });
                });

                // Day/night info (compact)
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    let dn = &body.day_night;
                    ui.label(
                        egui::RichText::new(format!(
                            "cycle: {:.0}s  day:{:.0}% night:{:.0}%",
                            dn.cycle_duration_secs,
                            dn.day_ratio * 100.0,
                            dn.night_ratio * 100.0,
                        ))
                        .small()
                        .color(egui::Color32::from_gray(100)),
                    );
                });

                ui.add_space(2.0);
            }
        });

    Ok(())
}
