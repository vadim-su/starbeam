use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContexts};

use crate::liquid::data::{LiquidCell, LiquidId};
use crate::liquid::registry::LiquidRegistry;
use crate::liquid::render::DirtyLiquidChunks;
use crate::liquid::system::LiquidSimState;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::{tile_to_chunk, tile_to_local, world_to_tile, WorldMap};

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

/// Current debug liquid type to spawn. Cycles with F6.
#[derive(Resource)]
pub struct DebugLiquidType {
    pub current: u8, // 1=water, 2=lava, 3=oil
}

impl Default for DebugLiquidType {
    fn default() -> Self {
        Self { current: 1 }
    }
}

/// Liquid debug panel state (F8).
#[derive(Resource, Default)]
pub struct LiquidDebugState {
    pub visible: bool,
}

// ---------------------------------------------------------------------------
// F5/F6 spawn keys
// ---------------------------------------------------------------------------

/// F5: Spawn liquid at cursor position.
/// F6: Cycle liquid type (water -> lava -> oil -> water).
pub fn debug_liquid_keys(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut debug_type: ResMut<DebugLiquidType>,
    config: Res<ActiveWorld>,
    mut world_map: ResMut<WorldMap>,
    mut dirty_liquid: ResMut<DirtyLiquidChunks>,
    mut liquid_sim: ResMut<LiquidSimState>,
    liquid_registry: Res<LiquidRegistry>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
) {
    // F6: cycle type
    if keyboard.just_pressed(KeyCode::F6) {
        let max_types = liquid_registry.defs.len() as u8;
        if max_types == 0 {
            return;
        }
        debug_type.current = if debug_type.current >= max_types {
            1
        } else {
            debug_type.current + 1
        };
        let name = liquid_registry
            .get(LiquidId(debug_type.current))
            .map(|d| d.name.as_str())
            .unwrap_or("???");
        info!("Debug liquid type: {} ({})", name, debug_type.current);
    }

    // F5: spawn liquid at cursor (3x3 area, full fill)
    if keyboard.just_pressed(KeyCode::F5) {
        let Ok(window) = windows.single() else { return };
        let Ok((camera, camera_gt)) = camera_query.single() else {
            return;
        };
        let Some(cursor_pos) = window.cursor_position() else {
            return;
        };
        let Ok(world_pos) = camera.viewport_to_world_2d(camera_gt, cursor_pos) else {
            return;
        };

        let (center_tx, center_ty) = world_to_tile(world_pos.x, world_pos.y, config.tile_size);

        let liquid_type = LiquidId(debug_type.current);
        let name = liquid_registry
            .get(liquid_type)
            .map(|d| d.name.as_str())
            .unwrap_or("???");

        let mut placed = 0;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let tx = center_tx + dx;
                let ty = center_ty + dy;

                if ty < 0 || ty >= config.height_tiles {
                    continue;
                }

                let wtx = config.wrap_tile_x(tx);
                let (cx, cy) = tile_to_chunk(wtx, ty, config.chunk_size);

                // Skip solid tiles.
                if let Some(chunk) = world_map.chunk(cx, cy) {
                    let (lx, ly) = tile_to_local(wtx, ty, config.chunk_size);
                    let tile_id = chunk.fg.get(lx, ly, config.chunk_size);
                    if tile_id != crate::registry::tile::TileId::AIR {
                        continue;
                    }
                }

                // Set liquid.
                if let Some(chunk) = world_map.chunk_mut(cx, cy) {
                    let (lx, ly) = tile_to_local(wtx, ty, config.chunk_size);
                    chunk.liquid.set(
                        lx,
                        ly,
                        LiquidCell {
                            liquid_type,
                            level: 1.0,
                        },
                        config.chunk_size,
                    );
                    dirty_liquid.0.insert((cx, cy));
                    liquid_sim.sleep.mark_changed(tx, ty);
                    placed += 1;
                }
            }
        }

        info!(
            "Spawned {} tiles of '{}' at ({}, {}). Active sleep tiles: {}",
            placed,
            name,
            center_tx,
            center_ty,
            liquid_sim.sleep.active_count(),
        );
    }
}

// ---------------------------------------------------------------------------
// F8 toggle
// ---------------------------------------------------------------------------

pub fn toggle_liquid_debug(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<LiquidDebugState>,
) {
    if keyboard.just_pressed(KeyCode::F8) {
        state.visible = !state.visible;
    }
}

// ---------------------------------------------------------------------------
// Liquid debug panel (egui)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn draw_liquid_debug_panel(
    mut contexts: EguiContexts,
    state: Res<LiquidDebugState>,
    debug_type: Res<DebugLiquidType>,
    liquid_registry: Res<LiquidRegistry>,
    liquid_sim: Res<LiquidSimState>,
    world_map: Res<WorldMap>,
    config: Res<ActiveWorld>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
) -> Result {
    if !state.visible {
        return Ok(());
    }

    let ctx = contexts.ctx_mut()?;

    let panel_frame = egui::Frame::NONE
        .fill(egui::Color32::from_rgba_unmultiplied(20, 30, 50, 220))
        .inner_margin(egui::Margin::same(8))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 80, 120)));

    egui::SidePanel::left("liquid_debug_panel")
        .default_width(300.0)
        .resizable(false)
        .frame(panel_frame)
        .show(ctx, |ui| {
            ui.heading("Liquid Debug (F8)");
            ui.separator();

            // --- Simulation Stats ---
            egui::CollapsingHeader::new(egui::RichText::new("Simulation").strong())
                .default_open(true)
                .show(ui, |ui| {
                    egui::Grid::new("liq_sim_grid")
                        .num_columns(2)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Active tiles:");
                            ui.colored_label(
                                egui::Color32::LIGHT_GREEN,
                                format!("{}", liquid_sim.sleep.active_count()),
                            );
                            ui.end_row();

                            ui.label("Accumulator:");
                            ui.monospace(format!("{:.3}s", liquid_sim.accumulator));
                            ui.end_row();

                            ui.label("Spawn type:");
                            let type_name = liquid_registry
                                .get(LiquidId(debug_type.current))
                                .map(|d| d.name.as_str())
                                .unwrap_or("???");
                            ui.colored_label(
                                liquid_color_egui(&liquid_registry, LiquidId(debug_type.current)),
                                format!("{} (F6 to cycle)", type_name),
                            );
                            ui.end_row();
                        });
                });

            // --- Registry ---
            egui::CollapsingHeader::new(egui::RichText::new("Liquid Types").strong())
                .default_open(true)
                .show(ui, |ui| {
                    for (i, def) in liquid_registry.defs.iter().enumerate() {
                        let id = LiquidId((i + 1) as u8);
                        let color = liquid_color_egui(&liquid_registry, id);
                        ui.colored_label(color, egui::RichText::new(&def.name).strong());
                        egui::Grid::new(format!("liq_type_{i}"))
                            .num_columns(2)
                            .spacing([20.0, 2.0])
                            .show(ui, |ui| {
                                ui.label("  ID:");
                                ui.monospace(format!("{}", id.0));
                                ui.end_row();
                                ui.label("  Density:");
                                ui.monospace(format!("{:.1}", def.density));
                                ui.end_row();
                                ui.label("  Viscosity:");
                                ui.monospace(format!("{:.1}", def.viscosity));
                                ui.end_row();
                                ui.label("  Damage:");
                                ui.monospace(format!("{:.1}", def.damage_on_contact));
                                ui.end_row();
                                ui.label("  Swim speed:");
                                ui.monospace(format!("{:.2}", def.swim_speed_factor));
                                ui.end_row();
                                ui.label("  Light emit:");
                                ui.monospace(format!(
                                    "({}, {}, {})",
                                    def.light_emission[0],
                                    def.light_emission[1],
                                    def.light_emission[2]
                                ));
                                ui.end_row();
                                ui.label("  Opacity:");
                                ui.monospace(format!("{}", def.light_opacity));
                                ui.end_row();
                                ui.label("  Reactions:");
                                if def.reactions.is_empty() {
                                    ui.label("none");
                                } else {
                                    let names: Vec<&str> =
                                        def.reactions.iter().map(|r| r.other.as_str()).collect();
                                    ui.label(names.join(", "));
                                }
                                ui.end_row();
                            });
                        ui.add_space(4.0);
                    }
                });

            // --- Cursor Cell ---
            egui::CollapsingHeader::new(egui::RichText::new("Cursor Cell").strong())
                .default_open(true)
                .show(ui, |ui| {
                    let cursor_cell =
                        get_cursor_liquid(&windows, &camera_query, &world_map, &config);

                    match cursor_cell {
                        Some((tx, ty, cell)) => {
                            egui::Grid::new("liq_cursor_grid")
                                .num_columns(2)
                                .spacing([20.0, 4.0])
                                .show(ui, |ui| {
                                    ui.label("Tile:");
                                    ui.monospace(format!("{}, {}", tx, ty));
                                    ui.end_row();

                                    if cell.is_empty() {
                                        ui.label("Liquid:");
                                        ui.colored_label(egui::Color32::GRAY, "empty");
                                        ui.end_row();
                                    } else {
                                        let name = liquid_registry
                                            .get(cell.liquid_type)
                                            .map(|d| d.name.as_str())
                                            .unwrap_or("???");
                                        let color =
                                            liquid_color_egui(&liquid_registry, cell.liquid_type);

                                        ui.label("Type:");
                                        ui.colored_label(color, name);
                                        ui.end_row();

                                        ui.label("ID:");
                                        ui.monospace(format!("{}", cell.liquid_type.0));
                                        ui.end_row();

                                        ui.label("Level:");
                                        let bar_color =
                                            liquid_color_egui32(&liquid_registry, cell.liquid_type);
                                        ui.horizontal(|ui| {
                                            ui.monospace(format!("{:.3}", cell.level));
                                            let bar =
                                                egui::ProgressBar::new(cell.level).fill(bar_color);
                                            ui.add_sized([80.0, 14.0], bar);
                                        });
                                        ui.end_row();

                                        if let Some(def) = liquid_registry.get(cell.liquid_type) {
                                            ui.label("Density:");
                                            ui.monospace(format!("{:.1}", def.density));
                                            ui.end_row();

                                            ui.label("Viscosity:");
                                            ui.monospace(format!("{:.1}", def.viscosity));
                                            ui.end_row();
                                        }

                                        // Show neighbors
                                        ui.label("Neighbors:");
                                        ui.end_row();
                                        for (label, dx, dy) in [
                                            ("  Up", 0, 1),
                                            ("  Down", 0, -1),
                                            ("  Left", -1, 0),
                                            ("  Right", 1, 0),
                                        ] {
                                            let nx = tx + dx;
                                            let ny = ty + dy;
                                            let n = get_liquid_at(&world_map, &config, nx, ny);
                                            ui.label(label);
                                            if n.is_empty() {
                                                ui.colored_label(egui::Color32::DARK_GRAY, "empty");
                                            } else {
                                                let n_name = liquid_registry
                                                    .get(n.liquid_type)
                                                    .map(|d| d.name.as_str())
                                                    .unwrap_or("?");
                                                let n_color = liquid_color_egui(
                                                    &liquid_registry,
                                                    n.liquid_type,
                                                );
                                                ui.colored_label(
                                                    n_color,
                                                    format!("{} {:.2}", n_name, n.level),
                                                );
                                            }
                                            ui.end_row();
                                        }
                                    }
                                });
                        }
                        None => {
                            ui.label("(cursor outside)");
                        }
                    }
                });
        });

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn get_cursor_liquid(
    windows: &Query<&Window, With<PrimaryWindow>>,
    camera_query: &Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    world_map: &WorldMap,
    config: &ActiveWorld,
) -> Option<(i32, i32, LiquidCell)> {
    let window = windows.single().ok()?;
    let cursor_pos = window.cursor_position()?;
    let (camera, camera_gt) = camera_query.single().ok()?;
    let world_pos = camera.viewport_to_world_2d(camera_gt, cursor_pos).ok()?;
    let (tx, ty) = world_to_tile(world_pos.x, world_pos.y, config.tile_size);
    let cell = get_liquid_at(world_map, config, tx, ty);
    Some((tx, ty, cell))
}

fn get_liquid_at(world_map: &WorldMap, config: &ActiveWorld, tx: i32, ty: i32) -> LiquidCell {
    if ty < 0 || ty >= config.height_tiles {
        return LiquidCell::EMPTY;
    }
    let wtx = config.wrap_tile_x(tx);
    let (cx, cy) = tile_to_chunk(wtx, ty, config.chunk_size);
    let (lx, ly) = tile_to_local(wtx, ty, config.chunk_size);
    match world_map.chunk(cx, cy) {
        Some(chunk) => chunk.liquid.get(lx, ly, config.chunk_size),
        None => LiquidCell::EMPTY,
    }
}

fn liquid_color_egui(registry: &LiquidRegistry, id: LiquidId) -> egui::Color32 {
    liquid_color_egui32(registry, id)
}

fn liquid_color_egui32(registry: &LiquidRegistry, id: LiquidId) -> egui::Color32 {
    if let Some(def) = registry.get(id) {
        let r = (def.color[0] * 255.0) as u8;
        let g = (def.color[1] * 255.0) as u8;
        let b = (def.color[2] * 255.0) as u8;
        // Brighten for visibility on dark panel
        let r = r.saturating_add(60);
        let g = g.saturating_add(60);
        let b = b.saturating_add(60);
        egui::Color32::from_rgb(r, g, b)
    } else {
        egui::Color32::GRAY
    }
}
