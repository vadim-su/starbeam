use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContexts};

use crate::fluid::cell::FluidId;
use crate::fluid::registry::FluidRegistry;
use crate::fluid::simulation::FluidSimConfig;
use crate::fluid::sph_particle::ParticleStore;
use crate::fluid::sph_simulation::SphConfig;
use crate::fluid::systems::ActiveFluidChunks;
use crate::fluid::wave::WaveState;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::{tile_to_chunk, tile_to_local, world_to_tile, WorldMap};

/// Debug visualization mode for fluid rendering.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum FluidDebugMode {
    /// Normal rendering (no debug overlay).
    #[default]
    Off,
    /// Heat-map of mass per cell (black=0 → green=1 → red=pressurized).
    Mass,
    /// Show which cells are surface cells (bright) vs interior (dim).
    Surface,
    /// Colour-code by fluid type (each FluidId gets a distinct hue).
    FluidType,
    /// Depth darkening visualised as grayscale.
    Depth,
}

impl FluidDebugMode {
    /// Integer sent to the shader uniform.
    pub fn as_u32(self) -> u32 {
        match self {
            Self::Off => 0,
            Self::Mass => 1,
            Self::Surface => 2,
            Self::FluidType => 3,
            Self::Depth => 4,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Mass => "Mass",
            Self::Surface => "Surface",
            Self::FluidType => "Fluid Type",
            Self::Depth => "Depth",
        }
    }

    const ALL: [Self; 5] = [Self::Off, Self::Mass, Self::Surface, Self::FluidType, Self::Depth];
}

/// Resource controlling fluid debug overlay.
#[derive(Resource)]
pub struct FluidDebugState {
    /// Whether the debug panel + overlay are visible.
    pub visible: bool,
    /// Which visualization mode is active.
    pub mode: FluidDebugMode,
    /// Show grid lines between cells in debug mode.
    pub show_grid: bool,
}

impl Default for FluidDebugState {
    fn default() -> Self {
        Self {
            visible: false,
            mode: FluidDebugMode::Mass,
            show_grid: true,
        }
    }
}

/// Toggle fluid debug overlay on F8. Adjust sim speed with +/-.
pub fn toggle_fluid_debug(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<FluidDebugState>,
    mut sim_config: ResMut<FluidSimConfig>,
) {
    if keyboard.just_pressed(KeyCode::F8) {
        state.visible = !state.visible;
    }
    // +/= speed up, - slow down, 0 reset to default
    if keyboard.just_pressed(KeyCode::Equal) || keyboard.just_pressed(KeyCode::NumpadAdd) {
        sim_config.tick_rate = (sim_config.tick_rate * 2.0).min(240.0);
        info!("Fluid tick rate: {} Hz", sim_config.tick_rate);
    }
    if keyboard.just_pressed(KeyCode::Minus) || keyboard.just_pressed(KeyCode::NumpadSubtract) {
        sim_config.tick_rate = (sim_config.tick_rate / 2.0).max(1.0);
        info!("Fluid tick rate: {} Hz", sim_config.tick_rate);
    }
    if keyboard.just_pressed(KeyCode::Digit0) {
        sim_config.tick_rate = 60.0;
        info!("Fluid tick rate reset: {} Hz", sim_config.tick_rate);
    }
}

/// Draw the fluid debug egui panel.
#[allow(clippy::too_many_arguments)]
pub fn draw_fluid_debug_panel(
    mut contexts: EguiContexts,
    mut state: ResMut<FluidDebugState>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    world_map: Res<WorldMap>,
    active_world: Res<ActiveWorld>,
    fluid_registry: Res<FluidRegistry>,
    active_fluids: Res<ActiveFluidChunks>,
    sim_config: Res<FluidSimConfig>,
    wave_state: Res<WaveState>,
    particles: Res<ParticleStore>,
    sph_config: Res<SphConfig>,
) -> Result {
    if !state.visible {
        return Ok(());
    }

    let ctx = contexts.ctx_mut()?;

    let panel_frame = egui::Frame::NONE
        .fill(egui::Color32::from_rgba_unmultiplied(20, 20, 30, 220))
        .inner_margin(egui::Margin::same(8))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)));

    egui::SidePanel::left("fluid_debug_panel")
        .default_width(300.0)
        .resizable(false)
        .frame(panel_frame)
        .show(ctx, |ui| {
            ui.heading("Fluid Debug (F8)");
            ui.separator();

            // --- Visualization mode selector ---
            egui::CollapsingHeader::new(egui::RichText::new("Visualization").strong())
                .default_open(true)
                .show(ui, |ui| {
                    for mode in FluidDebugMode::ALL {
                        ui.radio_value(&mut state.mode, mode, mode.label());
                    }
                    ui.checkbox(&mut state.show_grid, "Show grid lines");
                });

            // --- Simulation config ---
            egui::CollapsingHeader::new(egui::RichText::new("Simulation").strong())
                .default_open(true)
                .show(ui, |ui| {
                    egui::Grid::new("sim_grid")
                        .num_columns(2)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Tick rate:");
                            ui.colored_label(
                                if sim_config.tick_rate != 60.0 { egui::Color32::YELLOW } else { egui::Color32::LIGHT_GREEN },
                                format!("{} Hz (+/-/0)", sim_config.tick_rate),
                            );
                            ui.end_row();

                            ui.label("Active chunks:");
                            ui.colored_label(
                                egui::Color32::LIGHT_GREEN,
                                format!("{}", active_fluids.chunks.len()),
                            );
                            ui.end_row();

                            ui.label("Wave buffers:");
                            ui.monospace(format!("{}", wave_state.buffers.len()));
                            ui.end_row();

                            ui.label("SPH particles:");
                            ui.colored_label(
                                egui::Color32::LIGHT_BLUE,
                                format!("{}", particles.len()),
                            );
                            ui.end_row();

                            ui.label("SPH smoothing r:");
                            ui.monospace(format!("{:.1}", sph_config.smoothing_radius));
                            ui.end_row();
                        });
                });

            // --- Active chunks list ---
            egui::CollapsingHeader::new(egui::RichText::new("Active Chunks").strong())
                .default_open(false)
                .show(ui, |ui| {
                    let mut chunks: Vec<_> = active_fluids.chunks.iter().copied().collect();
                    chunks.sort();
                    for (cx, cy) in &chunks {
                        let calm = active_fluids.calm_ticks.get(&(*cx, *cy)).copied().unwrap_or(0);
                        let color = if calm > 30 {
                            egui::Color32::YELLOW
                        } else {
                            egui::Color32::LIGHT_GREEN
                        };
                        ui.colored_label(
                            color,
                            format!("({cx}, {cy})  calm: {calm}"),
                        );
                    }
                    if chunks.is_empty() {
                        ui.label("(none)");
                    }
                });

            // --- Cell under cursor ---
            egui::CollapsingHeader::new(egui::RichText::new("Cell at Cursor").strong())
                .default_open(true)
                .show(ui, |ui| {
                    let cursor_info = (|| {
                        let window = windows.single().ok()?;
                        let cursor_pos = window.cursor_position()?;
                        let (cam, cam_gt) = camera.single().ok()?;
                        cam.viewport_to_world_2d(cam_gt, cursor_pos).ok()
                    })();

                    if let Some(world_pos) = cursor_info {
                        let tile_size = active_world.tile_size;
                        let chunk_size = active_world.chunk_size;
                        let (tx, ty) = world_to_tile(world_pos.x, world_pos.y, tile_size);
                        let wrapped_tx = active_world.wrap_tile_x(tx);
                        let (cx, cy) = tile_to_chunk(wrapped_tx, ty, chunk_size);
                        let (lx, ly) = tile_to_local(wrapped_tx, ty, chunk_size);

                        let cell_info = world_map
                            .chunk(cx, cy)
                            .map(|chunk| {
                                let idx = (ly * chunk_size + lx) as usize;
                                if idx < chunk.fluids.len() {
                                    chunk.fluids[idx]
                                } else {
                                    crate::fluid::cell::FluidCell::EMPTY
                                }
                            });

                        egui::Grid::new("cell_grid")
                            .num_columns(2)
                            .spacing([20.0, 4.0])
                            .show(ui, |ui| {
                                ui.label("Tile:");
                                ui.monospace(format!("{wrapped_tx}, {ty}"));
                                ui.end_row();

                                ui.label("Local:");
                                ui.monospace(format!("{lx}, {ly}"));
                                ui.end_row();

                                ui.label("Chunk:");
                                ui.monospace(format!("{cx}, {cy}"));
                                ui.end_row();

                                let is_active = active_fluids.chunks.contains(&(cx, cy));
                                ui.label("Chunk active:");
                                ui.colored_label(
                                    if is_active { egui::Color32::LIGHT_GREEN } else { egui::Color32::LIGHT_RED },
                                    if is_active { "yes" } else { "no" },
                                );
                                ui.end_row();

                                if let Some(cell) = cell_info {
                                    if cell.is_empty() {
                                        ui.label("Fluid:");
                                        ui.colored_label(egui::Color32::GRAY, "(empty)");
                                        ui.end_row();
                                    } else {
                                        let has_secondary = !cell.secondary.is_empty();

                                        // --- Primary slot ---
                                        if !cell.primary.is_empty() {
                                            let def = fluid_registry.get(cell.primary.fluid_id);
                                            let label = if has_secondary { "Primary:" } else { "Fluid:" };
                                            ui.label(label);
                                            ui.colored_label(
                                                egui::Color32::from_rgb(def.color[0], def.color[1], def.color[2]),
                                                &def.id,
                                            );
                                            ui.end_row();

                                            ui.label("  FluidId:");
                                            ui.monospace(format!("{}", cell.primary.fluid_id.0));
                                            ui.end_row();

                                            ui.label("  Mass:");
                                            let mass_color = if cell.primary.mass > 1.0 {
                                                egui::Color32::RED
                                            } else if cell.primary.mass > 0.9 {
                                                egui::Color32::LIGHT_GREEN
                                            } else if cell.primary.mass > 0.1 {
                                                egui::Color32::YELLOW
                                            } else {
                                                egui::Color32::LIGHT_RED
                                            };
                                            ui.colored_label(mass_color, format!("{:.4}", cell.primary.mass));
                                            ui.end_row();

                                            ui.label("  Fill:");
                                            let fill = cell.primary.mass.min(1.0);
                                            ui.add(egui::ProgressBar::new(fill).text(format!("{:.0}%", fill * 100.0)));
                                            ui.end_row();

                                            ui.label("  Density:");
                                            ui.monospace(format!("{:.0}", def.density));
                                            ui.end_row();
                                        }

                                        // --- Secondary slot ---
                                        if has_secondary {
                                            let def2 = fluid_registry.get(cell.secondary.fluid_id);
                                            ui.label("Secondary:");
                                            ui.colored_label(
                                                egui::Color32::from_rgb(def2.color[0], def2.color[1], def2.color[2]),
                                                &def2.id,
                                            );
                                            ui.end_row();

                                            ui.label("  FluidId:");
                                            ui.monospace(format!("{}", cell.secondary.fluid_id.0));
                                            ui.end_row();

                                            ui.label("  Mass:");
                                            let mass_color = if cell.secondary.mass > 1.0 {
                                                egui::Color32::RED
                                            } else if cell.secondary.mass > 0.9 {
                                                egui::Color32::LIGHT_GREEN
                                            } else if cell.secondary.mass > 0.1 {
                                                egui::Color32::YELLOW
                                            } else {
                                                egui::Color32::LIGHT_RED
                                            };
                                            ui.colored_label(mass_color, format!("{:.4}", cell.secondary.mass));
                                            ui.end_row();

                                            ui.label("  Fill:");
                                            let fill = cell.secondary.mass.min(1.0);
                                            ui.add(egui::ProgressBar::new(fill).text(format!("{:.0}%", fill * 100.0)));
                                            ui.end_row();

                                            ui.label("  Density:");
                                            ui.monospace(format!("{:.0}", def2.density));
                                            ui.end_row();

                                            ui.label("Total:");
                                            ui.monospace(format!("{:.4}", cell.total_mass()));
                                            ui.end_row();
                                        }
                                    }
                                } else {
                                    ui.label("Fluid:");
                                    ui.colored_label(egui::Color32::DARK_GRAY, "(chunk not loaded)");
                                    ui.end_row();
                                }
                            });

                        // --- Neighbours ---
                        if let Some(cell) = cell_info {
                            if !cell.is_empty() {
                                ui.separator();
                                ui.label(egui::RichText::new("Neighbours:").strong());
                                let offsets = [("Up", 0i32, 1i32), ("Down", 0, -1), ("Left", -1, 0), ("Right", 1, 0)];
                                for (name, dx, dy) in offsets {
                                    let ntx = active_world.wrap_tile_x(wrapped_tx + dx);
                                    let nty = ty + dy;
                                    let (ncx, ncy) = tile_to_chunk(ntx, nty, chunk_size);
                                    let (nlx, nly) = tile_to_local(ntx, nty, chunk_size);
                                    let ncell = world_map.chunk(ncx, ncy).map(|c| {
                                        let nidx = (nly * chunk_size + nlx) as usize;
                                        if nidx < c.fluids.len() { c.fluids[nidx] } else { crate::fluid::cell::FluidCell::EMPTY }
                                    });
                                    let text = match ncell {
                                        Some(nc) if nc.is_empty() => format!("{name}: empty"),
                                        Some(nc) => {
                                            let mut parts = Vec::new();
                                            if !nc.primary.is_empty() {
                                                let nd = fluid_registry.get(nc.primary.fluid_id);
                                                parts.push(format!("{} m={:.3}", nd.id, nc.primary.mass));
                                            }
                                            if !nc.secondary.is_empty() {
                                                let nd = fluid_registry.get(nc.secondary.fluid_id);
                                                parts.push(format!("{} m={:.3}", nd.id, nc.secondary.mass));
                                            }
                                            format!("{name}: {}", parts.join(" + "))
                                        }
                                        None => format!("{name}: (unloaded)"),
                                    };
                                    ui.monospace(&text);
                                }
                            }
                        }
                    } else {
                        ui.label("— (cursor outside)");
                    }
                });

            // --- Total mass stats ---
            egui::CollapsingHeader::new(egui::RichText::new("Mass Stats").strong())
                .default_open(false)
                .show(ui, |ui| {
                    let mut total_mass: f32 = 0.0;
                    let mut total_cells: u32 = 0;
                    let mut by_type: std::collections::HashMap<FluidId, (f32, u32)> =
                        std::collections::HashMap::new();

                    for &(cx, cy) in &active_fluids.chunks {
                        if let Some(chunk) = world_map.chunk(cx, cy) {
                            for cell in &chunk.fluids {
                                if !cell.is_empty() {
                                    total_cells += 1;
                                }
                                if !cell.primary.is_empty() {
                                    total_mass += cell.primary.mass;
                                    let entry = by_type.entry(cell.primary.fluid_id).or_insert((0.0, 0));
                                    entry.0 += cell.primary.mass;
                                    entry.1 += 1;
                                }
                                if !cell.secondary.is_empty() {
                                    total_mass += cell.secondary.mass;
                                    let entry = by_type.entry(cell.secondary.fluid_id).or_insert((0.0, 0));
                                    entry.0 += cell.secondary.mass;
                                    entry.1 += 1;
                                }
                            }
                        }
                    }

                    egui::Grid::new("mass_grid")
                        .num_columns(2)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Total mass:");
                            ui.monospace(format!("{total_mass:.2}"));
                            ui.end_row();

                            ui.label("Total cells:");
                            ui.monospace(format!("{total_cells}"));
                            ui.end_row();
                        });

                    if !by_type.is_empty() {
                        ui.separator();
                        let mut entries: Vec<_> = by_type.into_iter().collect();
                        entries.sort_by_key(|(fid, _)| fid.0);
                        for (fid, (mass, count)) in entries {
                            let def = fluid_registry.get(fid);
                            ui.colored_label(
                                egui::Color32::from_rgb(def.color[0], def.color[1], def.color[2]),
                                format!("{}: {count} cells, mass={mass:.2}", def.id),
                            );
                        }
                    }

                    ui.separator();
                    ui.label(egui::RichText::new("SPH Particles").strong());
                    egui::Grid::new("sph_grid")
                        .num_columns(2)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Count:");
                            ui.monospace(format!("{}", particles.len()));
                            ui.end_row();

                            if !particles.is_empty() {
                                let avg_density: f32 = particles.densities.iter().sum::<f32>() / particles.len() as f32;
                                let avg_pressure: f32 = particles.pressures.iter().sum::<f32>() / particles.len() as f32;
                                let avg_speed: f32 = particles.velocities.iter().map(|v| v.length()).sum::<f32>() / particles.len() as f32;

                                ui.label("Avg density:");
                                ui.monospace(format!("{avg_density:.4}"));
                                ui.end_row();

                                ui.label("Avg pressure:");
                                ui.monospace(format!("{avg_pressure:.2}"));
                                ui.end_row();

                                ui.label("Avg speed:");
                                ui.monospace(format!("{avg_speed:.1} px/s"));
                                ui.end_row();
                            }
                        });
                });
        });

    Ok(())
}
