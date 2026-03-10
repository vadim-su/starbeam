use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::combat::Health;
use crate::player::Player;

/// Draw the health bar HUD for the player.
pub fn draw_health_hud(
    mut contexts: EguiContexts,
    query: Query<&Health, With<Player>>,
) -> Result {
    let Ok(health) = query.single() else {
        return Ok(());
    };

    let ctx = contexts.ctx_mut()?;

    let ratio = health.current / health.max;
    let bar_color = if ratio > 0.5 {
        egui::Color32::from_rgb(50, 200, 70) // green
    } else if ratio > 0.25 {
        egui::Color32::from_rgb(230, 200, 50) // yellow
    } else {
        egui::Color32::from_rgb(220, 50, 50) // red
    };

    egui::Area::new(egui::Id::new("health_hud"))
        .fixed_pos(egui::pos2(10.0, 32.0))
        .interactable(false)
        .show(ctx, |ui| {
            let bar_width = 140.0;
            let bar_height = 16.0;

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("HP")
                        .color(egui::Color32::WHITE)
                        .size(14.0),
                );

                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(bar_width, bar_height), egui::Sense::hover());

                let painter = ui.painter();

                // Background
                painter.rect_filled(
                    rect,
                    3.0,
                    egui::Color32::from_rgba_unmultiplied(20, 20, 30, 180),
                );

                // Filled portion
                if ratio > 0.0 {
                    let filled_rect = egui::Rect::from_min_size(
                        rect.min,
                        egui::vec2(bar_width * ratio, bar_height),
                    );
                    painter.rect_filled(filled_rect, 3.0, bar_color);
                }

                // Border
                painter.rect_stroke(
                    rect,
                    3.0,
                    egui::Stroke::new(1.0, egui::Color32::from_gray(120)),
                    egui::StrokeKind::Outside,
                );

                // Text overlay
                let text = format!("HP {:.0}/{:.0}", health.current, health.max);
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    &text,
                    egui::FontId::proportional(11.0),
                    egui::Color32::WHITE,
                );
            });
        });

    Ok(())
}
