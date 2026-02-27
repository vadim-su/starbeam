use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContexts};

use crate::parallax::transition::CurrentBiome;
use crate::player::{Grounded, Player, Velocity};
use crate::registry::biome::BiomeRegistry;
use crate::registry::tile::TileId;
use crate::registry::tile::TileRegistry;
use crate::registry::world::WorldConfig;
use crate::registry::BiomeParallaxConfigs;
use crate::world::chunk::{tile_to_chunk, tile_to_local, world_to_tile, LoadedChunks, WorldMap};

/// Tracks debug panel visibility.
#[derive(Resource, Default)]
pub struct DebugUiState {
    pub visible: bool,
}

/// Toggles debug panel visibility on F3 press.
pub fn toggle_debug_panel(keyboard: Res<ButtonInput<KeyCode>>, mut state: ResMut<DebugUiState>) {
    if keyboard.just_pressed(KeyCode::F3) {
        state.visible = !state.visible;
    }
}

/// Draws the debug inspector panel using egui.
#[allow(clippy::too_many_arguments)]
pub fn draw_debug_panel(
    mut contexts: EguiContexts,
    state: Res<DebugUiState>,
    // Player
    player_query: Query<(&Transform, &Velocity, &Grounded), With<Player>>,
    // Cursor
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    // World
    world_map: Res<WorldMap>,
    world_config: Res<WorldConfig>,
    tile_registry: Res<TileRegistry>,
    loaded_chunks: Res<LoadedChunks>,
    // Performance
    diagnostics: Res<DiagnosticsStore>,
    entities: Query<Entity>,
    // Parallax
    biome_registry: Res<BiomeRegistry>,
    biome_parallax: Option<Res<BiomeParallaxConfigs>>,
    current_biome: Option<Res<CurrentBiome>>,
) -> Result {
    if !state.visible {
        return Ok(());
    }

    let ctx = contexts.ctx_mut()?;

    let panel_frame = egui::Frame::NONE
        .fill(egui::Color32::from_rgba_unmultiplied(20, 20, 30, 200))
        .inner_margin(egui::Margin::same(8))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)));

    egui::SidePanel::right("debug_panel")
        .default_width(280.0)
        .resizable(false)
        .frame(panel_frame)
        .show(ctx, |ui| {
            ui.heading("Debug Panel");
            ui.separator();

            // --- Performance ---
            egui::CollapsingHeader::new(egui::RichText::new("Performance").strong())
                .default_open(true)
                .show(ui, |ui| {
                    egui::Grid::new("perf_grid")
                        .num_columns(2)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("FPS:");
                            let fps_text = diagnostics
                                .get(&FrameTimeDiagnosticsPlugin::FPS)
                                .and_then(|d| d.smoothed())
                                .map(|v| format!("{v:.1}"))
                                .unwrap_or_else(|| "...".to_string());
                            ui.colored_label(egui::Color32::LIGHT_GREEN, &fps_text);
                            ui.end_row();

                            ui.label("Frame time:");
                            let ft_text = diagnostics
                                .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
                                .and_then(|d| d.smoothed())
                                .map(|v| format!("{v:.1}ms"))
                                .unwrap_or_else(|| "...".to_string());
                            ui.label(&ft_text);
                            ui.end_row();

                            ui.label("Entities:");
                            ui.label(format!("{}", entities.iter().count()));
                            ui.end_row();
                        });
                });

            // --- Player ---
            egui::CollapsingHeader::new(egui::RichText::new("Player").strong())
                .default_open(true)
                .show(ui, |ui| {
                    if let Ok((transform, velocity, grounded)) = player_query.single() {
                        let px = transform.translation.x;
                        let py = transform.translation.y;
                        let (tx, ty) = world_to_tile(px, py, world_config.tile_size);
                        let (cx, cy) = tile_to_chunk(tx, ty, world_config.chunk_size);

                        egui::Grid::new("player_grid")
                            .num_columns(2)
                            .spacing([20.0, 4.0])
                            .show(ui, |ui| {
                                ui.label("Position:");
                                ui.monospace(format!("{px:.1}, {py:.1}"));
                                ui.end_row();

                                ui.label("Tile:");
                                ui.monospace(format!("{tx}, {ty}"));
                                ui.end_row();

                                ui.label("Velocity:");
                                ui.monospace(format!("{:.1}, {:.1}", velocity.x, velocity.y));
                                ui.end_row();

                                ui.label("Grounded:");
                                ui.label(if grounded.0 { "true" } else { "false" });
                                ui.end_row();

                                ui.label("Chunk:");
                                ui.monospace(format!("{cx}, {cy}"));
                                ui.end_row();
                            });
                    } else {
                        ui.label("No player entity");
                    }
                });

            // --- Cursor ---
            egui::CollapsingHeader::new(egui::RichText::new("Cursor").strong())
                .default_open(true)
                .show(ui, |ui| {
                    let cursor_info = (|| {
                        let window = windows.single().ok()?;
                        let cursor_pos = window.cursor_position()?;
                        let (camera, camera_gt) = camera_query.single().ok()?;
                        let world_pos = camera.viewport_to_world_2d(camera_gt, cursor_pos).ok()?;
                        Some(world_pos)
                    })();

                    if let Some(world_pos) = cursor_info {
                        let (tx, ty) =
                            world_to_tile(world_pos.x, world_pos.y, world_config.tile_size);
                        let wrapped_tx = world_config.wrap_tile_x(tx);
                        let (cx, cy) = tile_to_chunk(wrapped_tx, ty, world_config.chunk_size);

                        // Get tile info (read-only, no chunk generation)
                        let tile_info = if ty < 0 {
                            Some(tile_registry.by_name("stone"))
                        } else if ty >= world_config.height_tiles {
                            Some(TileId::AIR)
                        } else {
                            let (lx, ly) = tile_to_local(wrapped_tx, ty, world_config.chunk_size);
                            world_map
                                .chunk(cx, cy)
                                .map(|chunk| chunk.get(lx, ly, world_config.chunk_size))
                        };

                        if let Some(tile_id) = tile_info {
                            let tile_def = tile_registry.get(tile_id);

                            egui::Grid::new("cursor_grid")
                                .num_columns(2)
                                .spacing([20.0, 4.0])
                                .show(ui, |ui| {
                                    ui.label("World:");
                                    ui.monospace(format!("{:.1}, {:.1}", world_pos.x, world_pos.y));
                                    ui.end_row();

                                    ui.label("Tile:");
                                    ui.monospace(format!("{tx}, {ty}"));
                                    ui.end_row();

                                    ui.label("Block:");
                                    ui.colored_label(
                                        if tile_def.solid {
                                            egui::Color32::LIGHT_BLUE
                                        } else {
                                            egui::Color32::GRAY
                                        },
                                        &tile_def.id,
                                    );
                                    ui.end_row();

                                    ui.label("Solid:");
                                    ui.label(if tile_def.solid { "true" } else { "false" });
                                    ui.end_row();

                                    ui.label("Chunk:");
                                    ui.monospace(format!("{cx}, {cy}"));
                                    ui.end_row();
                                });
                        } else {
                            egui::Grid::new("cursor_grid")
                                .num_columns(2)
                                .spacing([20.0, 4.0])
                                .show(ui, |ui| {
                                    ui.label("World:");
                                    ui.monospace(format!("{:.1}, {:.1}", world_pos.x, world_pos.y));
                                    ui.end_row();

                                    ui.label("Tile:");
                                    ui.monospace(format!("{tx}, {ty}"));
                                    ui.end_row();

                                    ui.label("Block:");
                                    ui.colored_label(
                                        egui::Color32::DARK_GRAY,
                                        "(chunk not loaded)",
                                    );
                                    ui.end_row();

                                    ui.label("Chunk:");
                                    ui.monospace(format!("{cx}, {cy}"));
                                    ui.end_row();
                                });
                        }
                    } else {
                        ui.label("— (cursor outside)");
                    }
                });

            // --- World ---
            egui::CollapsingHeader::new(egui::RichText::new("World").strong())
                .default_open(true)
                .show(ui, |ui| {
                    egui::Grid::new("world_grid")
                        .num_columns(2)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Seed:");
                            ui.monospace(format!("{}", world_config.seed));
                            ui.end_row();

                            ui.label("Size:");
                            ui.monospace(format!(
                                "{} × {} tiles",
                                world_config.width_tiles, world_config.height_tiles
                            ));
                            ui.end_row();

                            ui.label("Loaded chunks:");
                            ui.label(format!("{}", loaded_chunks.map.len()));
                            ui.end_row();
                        });
                });

            // --- Parallax ---
            if let (Some(biome_parallax), Some(current_biome)) = (&biome_parallax, &current_biome) {
                egui::CollapsingHeader::new(egui::RichText::new("Parallax").strong())
                    .default_open(false)
                    .show(ui, |ui| {
                        let biome_name = biome_registry.name_of(current_biome.biome_id);
                        ui.label(format!("Biome: {}", biome_name));
                        if let Some(config) = biome_parallax.configs.get(&current_biome.biome_id) {
                            ui.label(format!("{} layers", config.layers.len()));
                            for (i, layer_def) in config.layers.iter().enumerate() {
                                ui.separator();
                                egui::Grid::new(format!("parallax_layer_{i}"))
                                    .num_columns(2)
                                    .spacing([20.0, 4.0])
                                    .show(ui, |ui| {
                                        ui.label("Name:");
                                        ui.monospace(&layer_def.name);
                                        ui.end_row();

                                        ui.label("Speed:");
                                        ui.monospace(format!(
                                            "{:.2}, {:.2}",
                                            layer_def.speed_x, layer_def.speed_y
                                        ));
                                        ui.end_row();

                                        ui.label("Repeat:");
                                        ui.monospace(format!(
                                            "x={}, y={}",
                                            layer_def.repeat_x, layer_def.repeat_y
                                        ));
                                        ui.end_row();
                                    });
                            }
                        } else {
                            ui.label("No parallax config for this biome");
                        }
                    });
            }
        });

    Ok(())
}
