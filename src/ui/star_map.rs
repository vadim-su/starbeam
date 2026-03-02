//! Star-map panel — shows the current system's bodies and allows warping.
//!
//! Toggle with **F4**. Displays the star, all orbiting bodies with their type
//! and size, highlights the current planet, and provides "Warp" buttons.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::cosmos::current::CurrentSystem;
use crate::cosmos::warp::WarpToBody;
use crate::registry::world::ActiveWorld;
use bevy::ecs::message::MessageWriter;

/// Tracks star-map panel visibility.
#[derive(Resource, Default)]
pub struct StarMapState {
    pub visible: bool,
}

/// Toggles star-map panel on F4 press.
pub fn toggle_star_map(keyboard: Res<ButtonInput<KeyCode>>, mut state: ResMut<StarMapState>) {
    if keyboard.just_pressed(KeyCode::F4) {
        state.visible = !state.visible;
    }
}

/// Draws the star-map panel.
pub fn draw_star_map(
    mut contexts: EguiContexts,
    state: Res<StarMapState>,
    current_system: Option<Res<CurrentSystem>>,
    active_world: Option<Res<ActiveWorld>>,
    mut warp_events: MessageWriter<WarpToBody>,
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

    let panel_frame = egui::Frame::NONE
        .fill(egui::Color32::from_rgba_unmultiplied(15, 15, 30, 220))
        .inner_margin(egui::Margin::same(12))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(80)));

    egui::Window::new("⭐ Star System")
        .default_width(320.0)
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

            ui.separator();
            ui.label(egui::RichText::new("Planets").strong().size(14.0));
            ui.add_space(4.0);

            // --- Body list ---
            let current_orbit = active_world.address.orbit().unwrap_or(0);

            for body in &system.bodies {
                let is_current = body.address.orbit() == Some(current_orbit);

                let type_color = match body.planet_type_id.as_str() {
                    "garden" => egui::Color32::from_rgb(100, 200, 100),
                    "barren" => egui::Color32::from_rgb(180, 150, 120),
                    _ => egui::Color32::from_rgb(160, 160, 200),
                };

                ui.horizontal(|ui| {
                    // Orbit number
                    ui.label(
                        egui::RichText::new(format!("#{}", body.address.orbit().unwrap_or(0)))
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
                        egui::RichText::new(format!("{}×{}", body.width_tiles, body.height_tiles))
                            .monospace()
                            .color(egui::Color32::from_gray(120)),
                    );

                    // Current marker or Warp button
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if is_current {
                            ui.label(
                                egui::RichText::new("● HERE")
                                    .color(egui::Color32::from_rgb(80, 255, 80))
                                    .strong(),
                            );
                        } else if ui
                            .button(
                                egui::RichText::new("Warp")
                                    .color(egui::Color32::from_rgb(100, 180, 255)),
                            )
                            .clicked()
                        {
                            warp_events.write(WarpToBody {
                                orbit: body.address.orbit().unwrap_or(0),
                            });
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
