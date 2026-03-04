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
    /// Heat-map of density per particle (blue=low -> red=high).
    Mass,
    /// Heat-map of pressure per particle (blue=low -> red=high).
    Surface,
    /// Colour-code by fluid type (each FluidId gets a distinct hue).
    FluidType,
    /// Heat-map of speed per particle (blue=slow -> white=fast).
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
            Self::Mass => "Density heatmap",
            Self::Surface => "Pressure heatmap",
            Self::FluidType => "Fluid Type",
            Self::Depth => "Speed heatmap",
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
    mut particles: ResMut<ParticleStore>,
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
        .default_width(320.0)
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
                                let n = particles.len() as f32;

                                // Density stats
                                let mut min_d = f32::MAX;
                                let mut max_d = f32::MIN;
                                let mut sum_d: f32 = 0.0;
                                for &d in &particles.densities {
                                    sum_d += d;
                                    if d < min_d { min_d = d; }
                                    if d > max_d { max_d = d; }
                                }
                                let avg_d = sum_d / n;

                                ui.label("Density (avg):");
                                ui.monospace(format!("{avg_d:.4}"));
                                ui.end_row();

                                ui.label("Density (min/max):");
                                ui.monospace(format!("{min_d:.4} / {max_d:.4}"));
                                ui.end_row();

                                // Pressure stats
                                let mut min_p = f32::MAX;
                                let mut max_p = f32::MIN;
                                let mut sum_p: f32 = 0.0;
                                for &p in &particles.pressures {
                                    sum_p += p;
                                    if p < min_p { min_p = p; }
                                    if p > max_p { max_p = p; }
                                }
                                let avg_p = sum_p / n;

                                ui.label("Pressure (avg):");
                                ui.monospace(format!("{avg_p:.1}"));
                                ui.end_row();

                                ui.label("Pressure (min/max):");
                                ui.monospace(format!("{min_p:.1} / {max_p:.1}"));
                                ui.end_row();

                                // Speed stats
                                let mut min_s = f32::MAX;
                                let mut max_s: f32 = 0.0;
                                let mut sum_s: f32 = 0.0;
                                for v in &particles.velocities {
                                    let s = v.length();
                                    sum_s += s;
                                    if s < min_s { min_s = s; }
                                    if s > max_s { max_s = s; }
                                }
                                let avg_s = sum_s / n;

                                ui.label("Speed (avg):");
                                ui.monospace(format!("{avg_s:.1} px/s"));
                                ui.end_row();

                                ui.label("Speed (max):");
                                ui.monospace(format!("{max_s:.1} px/s"));
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

                                // Diagnostic info
                                ui.end_row();
                                ui.label("rest_density:");
                                let rest_d = sph_config.rest_density;
                                let ratio = if rest_d > 1e-6 { avg_d / rest_d } else { 0.0 };
                                let color = if ratio > 2.0 {
                                    egui::Color32::RED
                                } else if ratio > 1.5 {
                                    egui::Color32::YELLOW
                                } else {
                                    egui::Color32::LIGHT_GREEN
                                };
                                ui.colored_label(color, format!("{rest_d:.4} (avg/rest = {ratio:.1}x)"));
                                ui.end_row();
                            }
                        });

                    // Action buttons
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Clear all particles").clicked() {
                            particles.clear();
                        }
                    });
                });

            // --- Particle at cursor ---
            egui::CollapsingHeader::new(egui::RichText::new("Particle at Cursor").strong())
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
                                ui.label("World pos:");
                                ui.monospace(format!("{:.0}, {:.0}", world_pos.x, world_pos.y));
                                ui.end_row();

                                ui.label("Tile:");
                                ui.monospace(format!("{wrapped_tx}, {ty}"));
                                ui.end_row();

                                ui.label("Chunk:");
                                ui.monospace(format!("{cx}, {cy} [{lx},{ly}]"));
                                ui.end_row();

                                if let Some(cell) = cell_info {
                                    if !cell.is_empty() && !cell.primary.is_empty() {
                                        let def = fluid_registry.get(cell.primary.fluid_id);
                                        ui.label("CA Cell:");
                                        ui.colored_label(
                                            egui::Color32::from_rgb(def.color[0], def.color[1], def.color[2]),
                                            format!("{} m={:.3}", def.id, cell.primary.mass),
                                        );
                                        ui.end_row();
                                    }
                                }

                                // Find nearest SPH particle
                                let search_radius = sph_config.smoothing_radius;
                                let mut nearest_idx: Option<usize> = None;
                                let mut nearest_dist = f32::MAX;
                                let mut nearby_count = 0u32;
                                for i in 0..particles.len() {
                                    let d = world_pos.distance(particles.positions[i]);
                                    if d < search_radius {
                                        nearby_count += 1;
                                        if d < nearest_dist {
                                            nearest_dist = d;
                                            nearest_idx = Some(i);
                                        }
                                    }
                                }

                                ui.label("SPH nearby:");
                                ui.colored_label(
                                    if nearby_count > 0 { egui::Color32::LIGHT_BLUE } else { egui::Color32::GRAY },
                                    format!("{nearby_count}"),
                                );
                                ui.end_row();

                                // Show nearest particle details
                                if let Some(idx) = nearest_idx {
                                    ui.label("Nearest particle:");
                                    ui.monospace(format!("#{idx} (d={nearest_dist:.1})"));
                                    ui.end_row();

                                    ui.label("  density:");
                                    let d = particles.densities[idx];
                                    let rest = sph_config.rest_density;
                                    let ratio = if rest > 1e-6 { d / rest } else { 0.0 };
                                    let dcolor = if ratio > 2.0 { egui::Color32::RED }
                                        else if ratio > 1.2 { egui::Color32::YELLOW }
                                        else { egui::Color32::LIGHT_GREEN };
                                    ui.colored_label(dcolor, format!("{d:.4} ({ratio:.1}x rest)"));
                                    ui.end_row();

                                    ui.label("  pressure:");
                                    let p = particles.pressures[idx];
                                    let pcolor = if p > 100.0 { egui::Color32::RED }
                                        else if p < -100.0 { egui::Color32::LIGHT_BLUE }
                                        else { egui::Color32::LIGHT_GREEN };
                                    ui.colored_label(pcolor, format!("{p:.1}"));
                                    ui.end_row();

                                    ui.label("  velocity:");
                                    let v = particles.velocities[idx];
                                    ui.monospace(format!("({:.1}, {:.1}) |{:.1}|", v.x, v.y, v.length()));
                                    ui.end_row();

                                    ui.label("  force:");
                                    let f = particles.forces[idx];
                                    ui.monospace(format!("({:.1}, {:.1})", f.x, f.y));
                                    ui.end_row();

                                    ui.label("  mass:");
                                    ui.monospace(format!("{:.2}", particles.masses[idx]));
                                    ui.end_row();
                                }
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
                    ui.label(egui::RichText::new("PBF Simulation").underline());
                    ui.add(
                        egui::Slider::new(&mut sph_config.smoothing_radius, 4.0..=64.0)
                            .text("Smoothing radius (h)"),
                    );
                    ui.add(
                        egui::Slider::new(&mut sph_config.rest_density, 0.001..=0.1)
                            .text("Rest density (ρ₀)"),
                    );
                    ui.add(
                        egui::Slider::new(&mut sph_config.solver_iterations, 1..=10)
                            .text("Solver iterations"),
                    );
                    ui.add(
                        egui::Slider::new(&mut sph_config.epsilon, 0.001..=10.0)
                            .logarithmic(true)
                            .text("Epsilon (CFM)"),
                    );
                    ui.add(
                        egui::Slider::new(&mut sph_config.viscosity, 0.0..=0.5)
                            .text("XSPH viscosity"),
                    );
                    ui.add(
                        egui::Slider::new(&mut sph_config.gravity.y, -500.0..=0.0)
                            .text("Gravity Y"),
                    );
                    ui.add(
                        egui::Slider::new(&mut sph_config.particle_mass, 0.1..=10.0)
                            .text("Particle mass"),
                    );
                    ui.add(
                        egui::Slider::new(&mut sph_config.surface_tension_k, 0.0..=1.0)
                            .text("Surface tension (k)"),
                    );

                    ui.separator();
                    ui.label(egui::RichText::new("Diagnostics").underline());
                    // Show computed kernel values for tuning
                    let h = sph_config.smoothing_radius;
                    let p0 = crate::fluid::sph_kernels::poly6(0.0, h);
                    let p_half = crate::fluid::sph_kernels::poly6(h * 0.5, h);
                    ui.monospace(format!("poly6(0, {h:.0}) = {p0:.6}"));
                    ui.monospace(format!("poly6({:.0}, {h:.0}) = {p_half:.6}", h * 0.5));
                    ui.monospace(format!("self-density = {:.6}", p0 * sph_config.particle_mass));
                });
        });

    Ok(())
}
