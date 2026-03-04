use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContexts};

use crate::fluid::cell::FluidId;
use crate::fluid::registry::FluidRegistry;
use crate::fluid::sph_particle::ParticleStore;
use crate::fluid::sph_simulation::SphConfig;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::{tile_to_chunk, tile_to_local, world_to_tile, WorldMap};

/// Debug visualization mode for fluid rendering.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum FluidDebugMode {
    /// Normal rendering (no debug overlay).
    #[default]
    Off,
    /// Heat-map of mass per cell (black=0 -> green=1 -> red=pressurized).
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
    /// Override particle visual radius. 0.0 means auto (smoothing_radius * 0.5).
    pub particle_visual_radius: f32,
    /// Whether caustics are enabled in the shader.
    pub enable_caustics: bool,
    /// Whether shimmer is enabled in the shader.
    pub enable_shimmer: bool,
}

impl Default for FluidDebugState {
    fn default() -> Self {
        Self {
            visible: false,
            mode: FluidDebugMode::Mass,
            show_grid: true,
            particle_visual_radius: 0.0,
            enable_caustics: true,
            enable_shimmer: true,
        }
    }
}

/// Toggle fluid debug overlay on F8.
pub fn toggle_fluid_debug(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<FluidDebugState>,
) {
    if keyboard.just_pressed(KeyCode::F8) {
        state.visible = !state.visible;
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
    particles: Res<ParticleStore>,
    mut sph_config: ResMut<SphConfig>,
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

            // --- SPH Particle stats ---
            egui::CollapsingHeader::new(egui::RichText::new("SPH Particles").strong())
                .default_open(true)
                .show(ui, |ui| {
                    egui::Grid::new("sph_grid")
                        .num_columns(2)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Count:");
                            ui.colored_label(
                                egui::Color32::LIGHT_BLUE,
                                format!("{}", particles.len()),
                            );
                            ui.end_row();

                            ui.label("Smoothing r:");
                            ui.monospace(format!("{:.1}", sph_config.smoothing_radius));
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

                                // Per-fluid-type breakdown
                                let mut by_type: std::collections::HashMap<FluidId, u32> =
                                    std::collections::HashMap::new();
                                for fid in &particles.fluid_ids {
                                    if *fid != FluidId::NONE {
                                        *by_type.entry(*fid).or_insert(0) += 1;
                                    }
                                }
                                if !by_type.is_empty() {
                                    ui.end_row();
                                    let mut entries: Vec<_> = by_type.into_iter().collect();
                                    entries.sort_by_key(|(fid, _)| fid.0);
                                    for (fid, count) in entries {
                                        let def = fluid_registry.get(fid);
                                        ui.label("  Type:");
                                        ui.colored_label(
                                            egui::Color32::from_rgb(def.color[0], def.color[1], def.color[2]),
                                            format!("{}: {count}", def.id),
                                        );
                                        ui.end_row();
                                    }
                                }
                            }
                        });
                });

            // --- Cell at cursor (still useful for tile/chunk inspection) ---
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

                                if let Some(cell) = cell_info {
                                    if cell.is_empty() {
                                        ui.label("CA Cell:");
                                        ui.colored_label(egui::Color32::GRAY, "(empty)");
                                        ui.end_row();
                                    } else {
                                        if !cell.primary.is_empty() {
                                            let def = fluid_registry.get(cell.primary.fluid_id);
                                            ui.label("CA Cell:");
                                            ui.colored_label(
                                                egui::Color32::from_rgb(def.color[0], def.color[1], def.color[2]),
                                                format!("{} m={:.3}", def.id, cell.primary.mass),
                                            );
                                            ui.end_row();
                                        }
                                    }
                                }

                                // SPH particles near cursor
                                let near_radius = sph_config.smoothing_radius;
                                let mut nearby_count = 0u32;
                                for i in 0..particles.len() {
                                    if world_pos.distance(particles.positions[i]) < near_radius {
                                        nearby_count += 1;
                                    }
                                }
                                ui.label("SPH nearby:");
                                ui.colored_label(
                                    if nearby_count > 0 { egui::Color32::LIGHT_BLUE } else { egui::Color32::GRAY },
                                    format!("{nearby_count}"),
                                );
                                ui.end_row();
                            });
                    } else {
                        ui.label("-- (cursor outside)");
                    }
                });

            // --- SPH Settings ---
            egui::CollapsingHeader::new(egui::RichText::new("SPH Settings").strong())
                .default_open(false)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Rendering").underline());
                    ui.add(
                        egui::Slider::new(&mut state.particle_visual_radius, 0.0..=32.0)
                            .text("Particle visual radius")
                            .custom_formatter(|v, _| {
                                if v == 0.0 {
                                    "auto".to_string()
                                } else {
                                    format!("{v:.1}")
                                }
                            }),
                    );
                    ui.checkbox(&mut state.enable_caustics, "Enable caustics");
                    ui.checkbox(&mut state.enable_shimmer, "Enable shimmer");

                    ui.separator();
                    ui.label(egui::RichText::new("Simulation").underline());
                    ui.add(
                        egui::Slider::new(&mut sph_config.smoothing_radius, 4.0..=64.0)
                            .text("Smoothing radius"),
                    );
                    ui.add(
                        egui::Slider::new(&mut sph_config.stiffness, 1.0..=500.0)
                            .text("Stiffness"),
                    );
                    ui.add(
                        egui::Slider::new(&mut sph_config.viscosity, 0.0..=5.0)
                            .text("Viscosity"),
                    );
                    ui.add(
                        egui::Slider::new(&mut sph_config.gravity.y, -500.0..=0.0)
                            .text("Gravity Y"),
                    );
                    ui.add(
                        egui::Slider::new(&mut sph_config.particle_mass, 0.1..=10.0)
                            .text("Particle mass"),
                    );
                });
        });

    Ok(())
}
