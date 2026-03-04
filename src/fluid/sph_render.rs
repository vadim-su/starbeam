use bevy::asset::RenderAssetUsages;
use bevy::math::Vec2;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;

use crate::fluid::cell::FluidId;
use crate::fluid::debug_overlay::FluidDebugMode;
use crate::fluid::registry::FluidRegistry;
use crate::fluid::sph_particle::ParticleStore;

/// Z-depth for fluid particles (between tiles z=0 and entities).
pub const FLUID_Z: f32 = 0.5;

/// Debug coloring context passed to the mesh builder.
pub struct DebugColorCtx {
    pub mode: FluidDebugMode,
    /// (min, max) density across all particles, for normalization.
    pub density_range: (f32, f32),
    /// (min, max) pressure across all particles.
    pub pressure_range: (f32, f32),
    /// max speed across all particles.
    pub max_speed: f32,
}

impl DebugColorCtx {
    /// Compute ranges from particle store.
    pub fn from_store(store: &ParticleStore, mode: FluidDebugMode) -> Self {
        if store.is_empty() || mode == FluidDebugMode::Off {
            return Self {
                mode,
                density_range: (0.0, 1.0),
                pressure_range: (0.0, 1.0),
                max_speed: 1.0,
            };
        }
        let mut min_d = f32::MAX;
        let mut max_d = f32::MIN;
        let mut min_p = f32::MAX;
        let mut max_p = f32::MIN;
        let mut max_s: f32 = 0.0;
        for i in 0..store.len() {
            let d = store.densities[i];
            let p = store.pressures[i];
            let s = store.velocities[i].length();
            if d < min_d { min_d = d; }
            if d > max_d { max_d = d; }
            if p < min_p { min_p = p; }
            if p > max_p { max_p = p; }
            if s > max_s { max_s = s; }
        }
        if (max_d - min_d).abs() < 1e-10 { max_d = min_d + 1.0; }
        if (max_p - min_p).abs() < 1e-10 { max_p = min_p + 1.0; }
        if max_s < 1e-6 { max_s = 1.0; }
        Self {
            mode,
            density_range: (min_d, max_d),
            pressure_range: (min_p, max_p),
            max_speed: max_s,
        }
    }

    /// Get debug color for particle i, or None if using default fluid color.
    fn color_for(&self, store: &ParticleStore, i: usize) -> Option<[f32; 4]> {
        match self.mode {
            FluidDebugMode::Off => None,
            FluidDebugMode::Mass => {
                let t = normalize(store.densities[i], self.density_range.0, self.density_range.1);
                Some(heatmap(t))
            }
            FluidDebugMode::Surface => {
                // Pressure-based: negative=blue, zero=green, positive=red
                let t = normalize(store.pressures[i], self.pressure_range.0, self.pressure_range.1);
                Some(heatmap(t))
            }
            FluidDebugMode::FluidType => None, // Use default fluid colors
            FluidDebugMode::Depth => {
                // Speed-based coloring
                let speed = store.velocities[i].length();
                let t = (speed / self.max_speed).clamp(0.0, 1.0);
                Some(speed_color(t))
            }
        }
    }
}

fn normalize(v: f32, min: f32, max: f32) -> f32 {
    ((v - min) / (max - min)).clamp(0.0, 1.0)
}

/// Blue -> Cyan -> Green -> Yellow -> Red heatmap
fn heatmap(t: f32) -> [f32; 4] {
    let (r, g, b) = if t < 0.25 {
        let s = t / 0.25;
        (0.0, s, 1.0) // blue -> cyan
    } else if t < 0.5 {
        let s = (t - 0.25) / 0.25;
        (0.0, 1.0, 1.0 - s) // cyan -> green
    } else if t < 0.75 {
        let s = (t - 0.5) / 0.25;
        (s, 1.0, 0.0) // green -> yellow
    } else {
        let s = (t - 0.75) / 0.25;
        (1.0, 1.0 - s, 0.0) // yellow -> red
    };
    [r, g, b, 0.9]
}

/// Slow=blue, fast=white
fn speed_color(t: f32) -> [f32; 4] {
    let r = t;
    let g = t * 0.5;
    let b = 1.0 - t * 0.5;
    [r, g, b, 0.9]
}

/// Build a mesh of quads for all particles within a given world-space AABB.
/// Each particle becomes a screen-aligned quad (2 triangles, 6 vertices).
/// Returns None if no particles are in the region.
pub fn build_particle_mesh(
    particles: &ParticleStore,
    chunk_world_min: Vec2,
    chunk_world_max: Vec2,
    particle_radius: f32,
    registry: &FluidRegistry,
    debug_ctx: Option<&DebugColorCtx>,
) -> Option<Mesh> {
    // Pre-allocate assuming ~25% of particles fall in this chunk region.
    // Each particle produces 6 vertices (2 triangles).
    let estimated_verts = (particles.len() / 4).max(64) * 6;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(estimated_verts);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(estimated_verts);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(estimated_verts);

    let margin = particle_radius * 2.0;
    let min = chunk_world_min - Vec2::splat(margin);
    let max = chunk_world_max + Vec2::splat(margin);

    for i in 0..particles.len() {
        let pos = particles.positions[i];
        if pos.x < min.x || pos.x > max.x || pos.y < min.y || pos.y > max.y {
            continue;
        }

        let fid = particles.fluid_ids[i];
        if fid == FluidId::NONE {
            continue;
        }

        // Determine color: debug override or fluid default
        let color = if let Some(ctx) = debug_ctx {
            ctx.color_for(particles, i).unwrap_or_else(|| {
                let def = registry.get(fid);
                [
                    def.color[0] as f32 / 255.0,
                    def.color[1] as f32 / 255.0,
                    def.color[2] as f32 / 255.0,
                    def.color[3] as f32 / 255.0,
                ]
            })
        } else {
            let def = registry.get(fid);
            [
                def.color[0] as f32 / 255.0,
                def.color[1] as f32 / 255.0,
                def.color[2] as f32 / 255.0,
                def.color[3] as f32 / 255.0,
            ]
        };

        let r = particle_radius;
        // Quad corners: bottom-left, bottom-right, top-right, top-left
        let corners = [
            ([-r, -r], [0.0, 0.0]),
            ([r, -r], [1.0, 0.0]),
            ([r, r], [1.0, 1.0]),
            ([-r, r], [0.0, 1.0]),
        ];
        // Two triangles: 0-1-2, 0-2-3
        let indices = [0usize, 1, 2, 0, 2, 3];

        for &idx in &indices {
            let (offset, uv) = corners[idx];
            positions.push([pos.x + offset[0], pos.y + offset[1], FLUID_Z]);
            colors.push(color);
            uvs.push(uv);
        }
    }

    if positions.is_empty() {
        return None;
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);

    Some(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::cell::FluidId;
    use crate::fluid::registry::{FluidDef, FluidRegistry};
    use crate::fluid::sph_particle::{Particle, ParticleStore};

    fn test_registry() -> FluidRegistry {
        FluidRegistry::from_defs(vec![FluidDef {
            id: "water".to_string(),
            density: 1000.0,
            viscosity: 0.1,
            max_compress: 0.02,
            is_gas: false,
            color: [64, 128, 255, 180],
            damage_on_contact: 0.0,
            light_emission: [0, 0, 0],
            effects: vec![],
            wave_amplitude: 1.0,
            wave_speed: 1.0,
            light_absorption: 0.3,
        }])
    }

    #[test]
    fn empty_store_returns_none() {
        let store = ParticleStore::new();
        let reg = test_registry();
        let mesh = build_particle_mesh(&store, Vec2::ZERO, Vec2::splat(100.0), 4.0, &reg, None);
        assert!(mesh.is_none());
    }

    #[test]
    fn single_particle_creates_6_vertices() {
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(50.0, 50.0), FluidId(1), 1.0));
        let reg = test_registry();
        let mesh = build_particle_mesh(&store, Vec2::ZERO, Vec2::splat(100.0), 4.0, &reg, None);
        assert!(mesh.is_some());
        let mesh = mesh.unwrap();
        // 1 particle = 6 vertices (2 triangles)
        assert_eq!(mesh.count_vertices(), 6);
    }

    #[test]
    fn particle_outside_region_excluded() {
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(200.0, 200.0), FluidId(1), 1.0));
        let reg = test_registry();
        let mesh = build_particle_mesh(&store, Vec2::ZERO, Vec2::splat(100.0), 4.0, &reg, None);
        assert!(mesh.is_none());
    }

    #[test]
    fn none_fluid_id_excluded() {
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(50.0, 50.0), FluidId::NONE, 1.0));
        let reg = test_registry();
        let mesh = build_particle_mesh(&store, Vec2::ZERO, Vec2::splat(100.0), 4.0, &reg, None);
        assert!(mesh.is_none());
    }

    #[test]
    fn debug_mass_mode_colors_particles() {
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(50.0, 50.0), FluidId(1), 1.0));
        store.densities[0] = 0.5;
        let reg = test_registry();
        let ctx = DebugColorCtx::from_store(&store, FluidDebugMode::Mass);
        let mesh = build_particle_mesh(&store, Vec2::ZERO, Vec2::splat(100.0), 4.0, &reg, Some(&ctx));
        assert!(mesh.is_some());
    }
}
