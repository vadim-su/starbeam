use bevy::asset::RenderAssetUsages;
use bevy::math::Vec2;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;

use crate::fluid::cell::FluidId;
use crate::fluid::registry::FluidRegistry;
use crate::fluid::sph_particle::ParticleStore;

/// Z-depth for fluid particles (between tiles z=0 and entities).
pub const FLUID_Z: f32 = 0.5;

/// Build a mesh of quads for all particles within a given world-space AABB.
/// Each particle becomes a screen-aligned quad (2 triangles, 6 vertices).
/// Returns None if no particles are in the region.
pub fn build_particle_mesh(
    particles: &ParticleStore,
    chunk_world_min: Vec2,
    chunk_world_max: Vec2,
    particle_radius: f32,
    registry: &FluidRegistry,
) -> Option<Mesh> {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();

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

        let def = registry.get(fid);
        let color = [
            def.color[0] as f32 / 255.0,
            def.color[1] as f32 / 255.0,
            def.color[2] as f32 / 255.0,
            def.color[3] as f32 / 255.0,
        ];

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
        let mesh = build_particle_mesh(&store, Vec2::ZERO, Vec2::splat(100.0), 4.0, &reg);
        assert!(mesh.is_none());
    }

    #[test]
    fn single_particle_creates_6_vertices() {
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(50.0, 50.0), FluidId(1), 1.0));
        let reg = test_registry();
        let mesh = build_particle_mesh(&store, Vec2::ZERO, Vec2::splat(100.0), 4.0, &reg);
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
        let mesh = build_particle_mesh(&store, Vec2::ZERO, Vec2::splat(100.0), 4.0, &reg);
        assert!(mesh.is_none());
    }

    #[test]
    fn none_fluid_id_excluded() {
        let mut store = ParticleStore::new();
        store.add(Particle::new(Vec2::new(50.0, 50.0), FluidId::NONE, 1.0));
        let reg = test_registry();
        let mesh = build_particle_mesh(&store, Vec2::ZERO, Vec2::splat(100.0), 4.0, &reg);
        assert!(mesh.is_none());
    }
}
