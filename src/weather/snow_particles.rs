/// Snow particle system: dedicated pool, spawning, physics, collision, and
/// batched mesh rendering.
///
/// Follows the same architecture as `src/particles/` but with constant fall
/// speed (no gravity acceleration) and a sinusoidal wobble on the X axis.
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::sprite_render::AlphaMode2d;
use bevy::window::PrimaryWindow;
use rand::Rng;

use super::weather_state::WeatherState;
use super::wind::Wind;
use crate::registry::tile::TileRegistry;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::WorldMap;

// ── SnowParticle ────────────────────────────────────────────────────────

struct SnowParticle {
    position: Vec2,
    base_fall_speed: f32,
    lifetime: f32,
    age: f32,
    size: f32,
    alpha: f32,
    alive: bool,
    wobble_phase: f32,
    wobble_speed: f32,
    wobble_amplitude: f32,
}

impl SnowParticle {
    fn is_dead(&self) -> bool {
        !self.alive
    }
}

// ── SnowParticlePool ────────────────────────────────────────────────────

const POOL_CAPACITY: usize = 1500;

/// Object-pool for snow particles. Uses ring-buffer search for O(1)-amortised
/// allocation and force-recycles the oldest particle when at capacity.
#[derive(Resource)]
pub struct SnowParticlePool {
    particles: Vec<SnowParticle>,
    next_free: usize,
    /// Accumulator for fractional spawning (sub-frame remainder).
    pub spawn_accumulator: f32,
}

impl Default for SnowParticlePool {
    fn default() -> Self {
        Self {
            particles: Vec::with_capacity(POOL_CAPACITY),
            next_free: 0,
            spawn_accumulator: 0.0,
        }
    }
}

impl SnowParticlePool {
    fn spawn(
        &mut self,
        position: Vec2,
        base_fall_speed: f32,
        size: f32,
        alpha: f32,
    ) {
        let mut rng = rand::thread_rng();
        let wobble_phase = rng.gen_range(0.0..std::f32::consts::TAU);
        let wobble_speed = rng.gen_range(1.5..3.0);
        let wobble_amplitude = rng.gen_range(3.0..8.0);
        let lifetime = rng.gen_range(6.0..12.0);

        let particle = SnowParticle {
            position,
            base_fall_speed,
            lifetime,
            age: 0.0,
            size,
            alpha,
            alive: true,
            wobble_phase,
            wobble_speed,
            wobble_amplitude,
        };

        let len = self.particles.len();
        let capacity = self.particles.capacity();

        // 1. Ring-buffer search for a dead slot.
        if len > 0 {
            for i in 0..len {
                let idx = (self.next_free + i) % len;
                if self.particles[idx].is_dead() {
                    self.particles[idx] = particle;
                    self.next_free = (idx + 1) % len.max(1);
                    return;
                }
            }
        }

        // 2. Grow vec if under capacity.
        if len < capacity {
            self.particles.push(particle);
            self.next_free = self.particles.len() % self.particles.len().max(1);
            return;
        }

        // 3. At capacity — force-kill the oldest particle (max age) and reuse.
        if len == 0 {
            return;
        }
        let oldest_idx = self
            .particles
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                a.age
                    .partial_cmp(&b.age)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
            .unwrap();

        self.particles[oldest_idx] = particle;
        self.next_free = (oldest_idx + 1) % len.max(1);
    }
}

// ── Spawn system ────────────────────────────────────────────────────────

/// Spawn snow particles in a strip above the camera viewport.
pub fn spawn_snow_particles(
    mut pool: ResMut<SnowParticlePool>,
    weather: Res<WeatherState>,
    camera_query: Query<(&Transform, &Projection), With<Camera2d>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    time: Res<Time>,
) {
    let intensity = weather.intensity();
    if intensity <= 0.0 {
        return;
    }

    let Ok((cam_tf, projection)) = camera_query.single() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

    let Projection::Orthographic(ortho) = projection else {
        return;
    };

    let cam_x = cam_tf.translation.x;
    let cam_y = cam_tf.translation.y;
    let visible_w = window.width() * ortho.scale;
    let visible_h = window.height() * ortho.scale;

    let dt = time.delta_secs();

    // Spawn rate: 30-80 particles/sec scaled by intensity.
    let spawn_rate = 30.0 + 50.0 * intensity;
    pool.spawn_accumulator += spawn_rate * dt;

    let mut rng = rand::thread_rng();

    let cam_top = cam_y + visible_h * 0.5;
    let cam_left = cam_x - visible_w * 0.5;

    while pool.spawn_accumulator >= 1.0 {
        pool.spawn_accumulator -= 1.0;

        // Spawn zone: strip above camera top (+16..48px), full camera width.
        let x = cam_left + rng.r#gen::<f32>() * visible_w;
        let y = cam_top + rng.gen_range(16.0..48.0);

        // Size: random 1-4px
        let size = rng.gen_range(1.0..4.0_f32);

        // Fall speed: 120 - (size-1) * (80/3) — larger = slower
        let base_fall_speed = 120.0 - (size - 1.0) * (80.0 / 3.0);

        // Alpha: 0.6-1.0, larger slightly more transparent
        let alpha = 1.0 - (size - 1.0) / 3.0 * 0.4;
        let alpha = alpha.clamp(0.6, 1.0);

        pool.spawn(Vec2::new(x, y), base_fall_speed, size, alpha);
    }
}

// ── Update / physics system ─────────────────────────────────────────────

/// Update snow particle physics: constant fall, wind, wobble, collision.
pub fn update_snow_particles(
    mut pool: ResMut<SnowParticlePool>,
    wind: Res<Wind>,
    time: Res<Time>,
    camera_query: Query<(&Transform, &Projection), With<Camera2d>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    world_map: Res<WorldMap>,
    tile_registry: Res<TileRegistry>,
    active_world: Res<ActiveWorld>,
) {
    let dt = time.delta_secs();
    let tile_size = active_world.tile_size;
    let chunk_size = active_world.chunk_size;
    let wind_vel = wind.velocity();

    // Get camera bottom for kill zone.
    let cam_bottom = if let Ok((cam_tf, projection)) = camera_query.single() {
        if let Ok(window) = windows.single() {
            let scale = match projection {
                Projection::Orthographic(ortho) => ortho.scale,
                _ => 1.0,
            };
            cam_tf.translation.y - window.height() * scale * 0.5
        } else {
            f32::MIN
        }
    } else {
        f32::MIN
    };

    for p in &mut pool.particles {
        if p.is_dead() {
            continue;
        }

        // Age
        p.age += dt;

        // Kill if lifetime exceeded
        if p.age >= p.lifetime {
            p.alive = false;
            continue;
        }

        // Wobble
        let wobble = (p.wobble_phase + p.wobble_speed * p.age).sin() * p.wobble_amplitude;

        // Constant fall speed (no gravity acceleration)
        p.position.y += (-p.base_fall_speed + wind_vel.y) * dt;

        // Wind applied to X + wobble
        p.position.x += (wind_vel.x + wobble) * dt;

        // Kill when below camera - 32px
        if p.position.y < cam_bottom - 32.0 {
            p.alive = false;
            continue;
        }

        // Kill on solid tile collision (same pattern as particles/physics.rs)
        let tile_x = (p.position.x / tile_size).floor() as i32;
        let tile_y = (p.position.y / tile_size).floor() as i32;
        let data_cx = active_world.wrap_chunk_x(tile_x.div_euclid(chunk_size as i32));
        let cy = tile_y.div_euclid(chunk_size as i32);
        let local_x = tile_x.rem_euclid(chunk_size as i32) as u32;
        let local_y = tile_y.rem_euclid(chunk_size as i32) as u32;

        if let Some(chunk) = world_map.chunks.get(&(data_cx, cy)) {
            let idx = (local_y * chunk_size + local_x) as usize;
            if idx < chunk.fg.tiles.len() {
                let tile_id = chunk.fg.tiles[idx];
                if tile_registry.is_solid(tile_id) {
                    p.alive = false;
                }
            }
        }
    }
}

// ── Rendering ───────────────────────────────────────────────────────────

/// Marker for the single entity that holds the snow particle batch mesh.
#[derive(Component)]
pub struct SnowMeshEntity;

/// Shared `ColorMaterial` handle used by the snow mesh entity.
#[derive(Resource)]
pub struct SharedSnowMaterial {
    pub handle: Handle<ColorMaterial>,
}

/// Snow z-layer — sits above tiles and normal particles, below UI.
pub const SNOW_Z: f32 = 3.0;

/// Create the shared `ColorMaterial` and the singleton `SnowMeshEntity`.
pub fn init_snow_render(
    mut commands: Commands,
    mut color_materials: ResMut<Assets<ColorMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let mat = color_materials.add(ColorMaterial {
        color: Color::WHITE,
        alpha_mode: AlphaMode2d::Blend,
        ..Default::default()
    });
    commands.insert_resource(SharedSnowMaterial {
        handle: mat.clone(),
    });

    let empty_mesh = meshes.add(Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    ));
    commands.spawn((
        SnowMeshEntity,
        Mesh2d(empty_mesh),
        MeshMaterial2d(mat),
        Transform::from_translation(Vec3::new(0.0, 0.0, SNOW_Z)),
        Visibility::default(),
    ));
}

/// Rebuild the snow particle batch mesh every frame.
pub fn rebuild_snow_mesh(
    pool: Res<SnowParticlePool>,
    mut meshes: ResMut<Assets<Mesh>>,
    query: Query<&Mesh2d, With<SnowMeshEntity>>,
) {
    let Ok(mesh_2d) = query.single() else {
        return;
    };

    let alive: Vec<_> = pool.particles.iter().filter(|p| !p.is_dead()).collect();

    // Build vertex data
    let n = alive.len();
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n * 4);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(n * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(n * 6);

    for (i, p) in alive.iter().enumerate() {
        let base = (i * 4) as u32;
        let x = p.position.x;
        let y = p.position.y;
        let r = p.size * 0.5;

        // Fade out in the last 20% of lifetime.
        let age_ratio = p.age / p.lifetime;
        let alpha = if age_ratio > 0.8 {
            p.alpha * (1.0 - (age_ratio - 0.8) / 0.2)
        } else {
            p.alpha
        };
        let c = [1.0, 1.0, 1.0, alpha]; // white snow

        // Four corners of the quad:
        //  3──2
        //  │ /│
        //  │/ │
        //  0──1
        positions.push([x - r, y - r, 0.0]);
        positions.push([x + r, y - r, 0.0]);
        positions.push([x + r, y + r, 0.0]);
        positions.push([x - r, y + r, 0.0]);

        colors.push(c);
        colors.push(c);
        colors.push(c);
        colors.push(c);

        // Two triangles: 0-1-2, 0-2-3
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    // Build the Mesh
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
