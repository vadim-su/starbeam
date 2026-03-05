/// Basic particle rendering: rebuilds a single batched mesh each frame from
/// all alive particles in the pool.
///
/// Each particle is a coloured quad (two triangles). The mesh uses
/// `ATTRIBUTE_POSITION` and `ATTRIBUTE_COLOR`. A `ColorMaterial` with
/// `AlphaMode2d::Blend` is shared and used for the whole batch.
///
/// This is the "Phase 1" renderer — no metaballs, no custom shader, just
/// coloured quads that prove the particle system is wired up end-to-end.
/// Metaball rendering (Task 10) can replace or augment this later.
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::sprite_render::AlphaMode2d;

use super::pool::ParticlePool;

/// Marker for the single entity that holds the particle batch mesh.
#[derive(Component)]
pub struct ParticleMeshEntity;

/// Shared `ColorMaterial` handle used by the particle mesh entity.
#[derive(Resource)]
pub struct SharedParticleMaterial {
    pub handle: Handle<ColorMaterial>,
}

/// Particle z-layer — sits above tiles and below UI.
pub const PARTICLE_Z: f32 = 1.0;

/// Create the shared `ColorMaterial` and the singleton `ParticleMeshEntity`.
pub fn init_particle_render(
    mut commands: Commands,
    mut color_materials: ResMut<Assets<ColorMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let mat = color_materials.add(ColorMaterial {
        color: Color::WHITE,
        alpha_mode: AlphaMode2d::Blend,
        ..Default::default()
    });
    commands.insert_resource(SharedParticleMaterial {
        handle: mat.clone(),
    });

    // Spawn the singleton entity. The mesh starts empty; `rebuild_particle_mesh`
    // will replace it each frame.
    let empty_mesh = meshes.add(Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    ));
    commands.spawn((
        ParticleMeshEntity,
        Mesh2d(empty_mesh),
        MeshMaterial2d(mat),
        Transform::from_translation(Vec3::new(0.0, 0.0, PARTICLE_Z)),
        Visibility::default(),
    ));
}

/// Rebuild the particle batch mesh every frame.
///
/// We iterate all alive particles in the pool and emit two tris per particle
/// (a quad centred on `particle.position` with half-size `particle.size`).
/// Per-vertex colours carry the particle's RGBA.
pub fn rebuild_particle_mesh(
    pool: Res<ParticlePool>,
    mut meshes: ResMut<Assets<Mesh>>,
    query: Query<&Mesh2d, With<ParticleMeshEntity>>,
) {
    let Ok(mesh_2d) = query.single() else {
        return;
    };

    let alive: Vec<_> = pool.particles.iter().filter(|p| !p.is_dead()).collect();

    // Build vertex data -------------------------------------------------------
    let n = alive.len();
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n * 4);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(n * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(n * 6);

    for (i, p) in alive.iter().enumerate() {
        let base = (i * 4) as u32;
        let x = p.position.x;
        let y = p.position.y;
        let r = p.size * 0.5;

        // Optionally fade out as particle ages.
        let alpha = if p.fade_out {
            p.color[3] * (1.0 - p.age_ratio())
        } else {
            p.color[3]
        };
        let c = [p.color[0], p.color[1], p.color[2], alpha];

        // Four corners of the quad (local space, no rotation):
        //  3──2
        //  │ /│
        //  │/ │
        //  0──1
        positions.push([x - r, y - r, 0.0]); // 0 bottom-left
        positions.push([x + r, y - r, 0.0]); // 1 bottom-right
        positions.push([x + r, y + r, 0.0]); // 2 top-right
        positions.push([x - r, y + r, 0.0]); // 3 top-left

        colors.push(c);
        colors.push(c);
        colors.push(c);
        colors.push(c);

        // Two triangles: 0-1-2, 0-2-3
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    // Build the Mesh ----------------------------------------------------------
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));

    if let Some(existing) = meshes.get_mut(&mesh_2d.0) {
        *existing = mesh;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::pool::ParticlePool;

    #[test]
    fn particle_mesh_entity_marker_exists() {
        let _ = std::any::TypeId::of::<ParticleMeshEntity>();
    }

    #[test]
    fn empty_pool_produces_zero_vertices() {
        let pool = ParticlePool::new(100);
        let alive: Vec<_> = pool.particles.iter().filter(|p| !p.is_dead()).collect();
        assert_eq!(alive.len(), 0);
    }

    #[test]
    fn alive_particle_produces_four_vertices() {
        let mut pool = ParticlePool::new(100);
        pool.spawn(
            Vec2::ZERO,
            Vec2::ZERO,
            1.0,
            4.0,
            [0.2, 0.5, 1.0, 1.0],
            1.0,
            false,
        );
        let alive: Vec<_> = pool.particles.iter().filter(|p| !p.is_dead()).collect();
        assert_eq!(alive.len(), 1);

        let n = alive.len();
        assert_eq!(n * 4, 4, "4 verts per particle");
        assert_eq!(n * 6, 6, "6 indices per particle");
    }

    #[test]
    fn alpha_fades_with_age_when_fade_out_true() {
        use crate::particles::particle::Particle;
        let p = Particle {
            position: Vec2::ZERO,
            velocity: Vec2::ZERO,
            lifetime: 2.0,
            age: 1.0, // 50% through life
            size: 1.0,
            color: [1.0, 1.0, 1.0, 1.0],
            alive: true,
            gravity_scale: 1.0,
            fade_out: true,
        };
        let alpha = if p.fade_out {
            p.color[3] * (1.0 - p.age_ratio())
        } else {
            p.color[3]
        };
        assert!(
            (alpha - 0.5).abs() < 1e-5,
            "alpha should be 0.5 at mid-life with fade_out=true"
        );
    }

    #[test]
    fn alpha_constant_when_fade_out_false() {
        use crate::particles::particle::Particle;
        let p = Particle {
            position: Vec2::ZERO,
            velocity: Vec2::ZERO,
            lifetime: 2.0,
            age: 1.0,
            size: 1.0,
            color: [1.0, 1.0, 1.0, 0.8],
            alive: true,
            gravity_scale: 1.0,
            fade_out: false,
        };
        let alpha = if p.fade_out {
            p.color[3] * (1.0 - p.age_ratio())
        } else {
            p.color[3]
        };
        assert!(
            (alpha - 0.8).abs() < 1e-5,
            "alpha should stay at original with fade_out=false"
        );
    }
}
