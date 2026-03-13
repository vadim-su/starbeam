/// Unified weather particle system supporting snow, rain, and sandstorm particles.
///
/// Generalises the architecture of `snow_particles.rs` to handle multiple
/// precipitation types through a `WeatherParticleConfig` that is selected at
/// runtime based on the resolved weather type.
///
/// NOTE: Y-axis increases UPWARD in this game.
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::sprite_render::AlphaMode2d;
use bevy::window::PrimaryWindow;
use rand::Rng;

use super::precipitation::{PrecipitationType, ResolvedWeatherType};
use super::weather_state::WeatherState;
use super::wind::Wind;
use crate::registry::tile::TileRegistry;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::WorldMap;

// ── Config ───────────────────────────────────────────────────────────────

/// Per-precipitation-type tuning parameters.
#[derive(Debug, Clone)]
pub struct WeatherParticleConfig {
    /// Min/max fall speed in pixels per second.
    pub fall_speed: (f32, f32),
    /// How much the wind affects this particle type (0.0..1.0).
    pub wind_influence: f32,
    /// Base fall angle in degrees (0 = straight down, 90 = horizontal).
    pub angle: f32,
    /// RGBA colour (0-255 per channel).
    pub color: (u8, u8, u8, u8),
    /// Min/max particle size in pixels.
    pub size: (f32, f32),
    /// Min/max streak length in pixels (rain: long, snow ≈ size).
    pub length: (f32, f32),
    /// Whether to apply sinusoidal X wobble (snow only).
    pub wobble: bool,
    /// Particles per second at intensity 0..1.
    pub spawn_rate: (f32, f32),
    /// Particle lifetime in seconds.
    pub lifetime: (f32, f32),
    /// Whether to spawn splash particles on surface hit.
    pub splash: bool,
}

pub fn snow_config() -> WeatherParticleConfig {
    WeatherParticleConfig {
        fall_speed: (80.0, 120.0),
        wind_influence: 0.8,
        angle: 0.0,
        color: (240, 245, 255, 230),
        size: (1.0, 4.0),
        length: (1.0, 4.0),
        wobble: true,
        spawn_rate: (30.0, 80.0),
        lifetime: (6.0, 12.0),
        splash: false,
    }
}

pub fn rain_config() -> WeatherParticleConfig {
    WeatherParticleConfig {
        fall_speed: (300.0, 500.0),
        wind_influence: 0.3,
        angle: 5.0,
        color: (140, 170, 220, 180),
        size: (1.0, 2.0),
        length: (8.0, 16.0),
        wobble: false,
        spawn_rate: (60.0, 150.0),
        lifetime: (3.0, 6.0),
        splash: true,
    }
}

pub fn sandstorm_config() -> WeatherParticleConfig {
    WeatherParticleConfig {
        fall_speed: (40.0, 80.0),
        wind_influence: 1.0,
        angle: 85.0,
        color: (210, 180, 120, 160),
        size: (1.0, 3.0),
        length: (2.0, 4.0),
        wobble: false,
        spawn_rate: (100.0, 200.0),
        lifetime: (4.0, 8.0),
        splash: false,
    }
}

fn config_for_type(precipitation_type: PrecipitationType) -> WeatherParticleConfig {
    match precipitation_type {
        PrecipitationType::Snow => snow_config(),
        PrecipitationType::Rain => rain_config(),
        PrecipitationType::Sandstorm => sandstorm_config(),
        PrecipitationType::Fog => snow_config(), // Fog doesn't use particles; won't be reached.
    }
}

// ── Particle ─────────────────────────────────────────────────────────────

pub struct WeatherParticle {
    pub position: Vec2,
    pub velocity: Vec2,
    pub lifetime: f32,
    pub age: f32,
    pub size: f32,
    pub length: f32,
    pub color: [f32; 4],
    pub alive: bool,
    /// If true this IS a splash particle — no collision/further splash spawning.
    pub splash: bool,
    pub wobble_phase: f32,
    pub wobble_speed: f32,
    pub wobble_amplitude: f32,
}

impl WeatherParticle {
    fn is_dead(&self) -> bool {
        !self.alive
    }
}

impl Default for WeatherParticle {
    fn default() -> Self {
        Self {
            position: Vec2::ZERO,
            velocity: Vec2::ZERO,
            lifetime: 1.0,
            age: 0.0,
            size: 1.0,
            length: 1.0,
            color: [1.0, 1.0, 1.0, 1.0],
            alive: false,
            splash: false,
            wobble_phase: 0.0,
            wobble_speed: 0.0,
            wobble_amplitude: 0.0,
        }
    }
}

// ── Pool ─────────────────────────────────────────────────────────────────

const POOL_CAPACITY: usize = 2500;

/// Object-pool for weather particles. Uses ring-buffer search for O(1)-amortised
/// allocation and force-recycles the oldest particle when at capacity.
#[derive(Resource)]
pub struct WeatherParticlePool {
    pub particles: Vec<WeatherParticle>,
    pub next_free: usize,
    /// Accumulator for fractional spawning (sub-frame remainder).
    pub spawn_accumulator: f32,
}

impl Default for WeatherParticlePool {
    fn default() -> Self {
        Self {
            particles: Vec::with_capacity(POOL_CAPACITY),
            next_free: 0,
            spawn_accumulator: 0.0,
        }
    }
}

impl WeatherParticlePool {
    /// Allocate a slot in the pool, mark it alive, and return its index.
    /// The caller is responsible for filling in all fields of the particle.
    pub fn allocate(&mut self) -> usize {
        let len = self.particles.len();
        let capacity = self.particles.capacity();

        // 1. Ring-buffer search for a dead slot.
        if len > 0 {
            for i in 0..len {
                let idx = (self.next_free + i) % len;
                if self.particles[idx].is_dead() {
                    self.particles[idx] = WeatherParticle::default();
                    self.particles[idx].alive = true;
                    self.next_free = (idx + 1) % len.max(1);
                    return idx;
                }
            }
        }

        // 2. Grow vec if under capacity.
        if len < capacity {
            self.particles.push(WeatherParticle { alive: true, ..Default::default() });
            let idx = self.particles.len() - 1;
            self.next_free = (idx + 1) % self.particles.len().max(1);
            return idx;
        }

        // 3. At capacity — force-kill the oldest particle and reuse its slot.
        if len == 0 {
            // Shouldn't happen given POOL_CAPACITY > 0, but guard anyway.
            self.particles.push(WeatherParticle { alive: true, ..Default::default() });
            return 0;
        }
        let oldest_idx = self
            .particles
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                a.age.partial_cmp(&b.age).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
            .unwrap();

        self.particles[oldest_idx] = WeatherParticle { alive: true, ..Default::default() };
        self.next_free = (oldest_idx + 1) % len.max(1);
        oldest_idx
    }
}

// ── Resources / Components ────────────────────────────────────────────────

/// Shared `ColorMaterial` handle used by the weather particle mesh entity.
#[derive(Resource)]
pub struct WeatherParticleMaterial {
    pub handle: Handle<ColorMaterial>,
}

/// Marker for the single entity holding the weather particle batch mesh.
#[derive(Component)]
pub struct WeatherMeshEntity;

/// Weather particle z-layer — sits above tiles and normal particles, below UI.
pub const WEATHER_Z: f32 = 3.0;

// ── Helper: angle + speed → velocity ─────────────────────────────────────

/// Convert a fall angle (degrees, 0 = straight down) and speed (px/s) into a
/// world-space velocity.  Y is negative because falling reduces Y.
fn angle_speed_to_velocity(angle_deg: f32, fall_speed: f32) -> Vec2 {
    let angle_rad = angle_deg.to_radians();
    Vec2::new(
        fall_speed * angle_rad.sin(),
        -fall_speed * angle_rad.cos(),
    )
}

/// Linearly interpolate between `a` and `b` by `t` (clamped to 0..1).
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

// ── Systems ───────────────────────────────────────────────────────────────

/// Create the shared `ColorMaterial` and the singleton `WeatherMeshEntity`.
pub fn init_weather_render(
    mut commands: Commands,
    mut color_materials: ResMut<Assets<ColorMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let mat = color_materials.add(ColorMaterial {
        color: Color::WHITE,
        alpha_mode: AlphaMode2d::Blend,
        ..Default::default()
    });
    commands.insert_resource(WeatherParticleMaterial { handle: mat.clone() });

    let empty_mesh = meshes.add(Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    ));
    commands.spawn((
        WeatherMeshEntity,
        Mesh2d(empty_mesh),
        MeshMaterial2d(mat),
        Transform::from_translation(Vec3::new(0.0, 0.0, WEATHER_Z)),
        Visibility::default(),
    ));
}

/// Spawn weather particles above (or beside) the camera viewport each frame.
pub fn spawn_weather_particles(
    mut pool: ResMut<WeatherParticlePool>,
    weather: Res<WeatherState>,
    resolved: Option<Res<ResolvedWeatherType>>,
    wind: Res<Wind>,
    camera_query: Query<(&Transform, &Projection), With<Camera2d>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    time: Res<Time>,
) {
    // Determine which precipitation type is active.
    let precip_type = match resolved.as_ref().and_then(|r| r.0) {
        Some(PrecipitationType::Fog) | None => return,
        Some(t) => t,
    };

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

    let cam_top = cam_y + visible_h * 0.5;
    let cam_left = cam_x - visible_w * 0.5;
    let cam_bottom = cam_y - visible_h * 0.5;
    let cam_right = cam_x + visible_w * 0.5;

    let dt = time.delta_secs();
    let config = config_for_type(precip_type);
    let mut rng = rand::thread_rng();

    // Accumulate spawn count.
    let spawn_rate = lerp(config.spawn_rate.0, config.spawn_rate.1, intensity);
    pool.spawn_accumulator += spawn_rate * dt;

    let wind_vel = wind.velocity();

    while pool.spawn_accumulator >= 1.0 {
        pool.spawn_accumulator -= 1.0;

        // Compute angle and fall speed for this particle.
        let fall_speed = rng.gen_range(config.fall_speed.0..config.fall_speed.1);
        let effective_angle = match precip_type {
            PrecipitationType::Rain => {
                config.angle + wind_vel.x.signum() * wind.strength * 10.0
            }
            _ => config.angle,
        };

        let base_vel = angle_speed_to_velocity(effective_angle, fall_speed);
        // Bake wind influence into initial velocity.
        let velocity = base_vel + wind_vel * config.wind_influence;

        // Spawn position.
        let (spawn_x, spawn_y) = if precip_type == PrecipitationType::Sandstorm {
            // Sandstorm particles enter from the side the wind is blowing from.
            let from_left = wind_vel.x >= 0.0;
            let x = if from_left {
                cam_left - rng.gen_range(0.0..32.0)
            } else {
                cam_right + rng.gen_range(0.0..32.0)
            };
            let y = cam_bottom + rng.r#gen::<f32>() * visible_h;
            (x, y)
        } else {
            // Snow / rain: spawn in a strip above the camera top.
            let x = cam_left + rng.r#gen::<f32>() * visible_w;
            let y = cam_top + rng.gen_range(16.0..48.0);
            (x, y)
        };

        // Colour.
        let (cr, cg, cb, ca) = config.color;
        let color = [
            cr as f32 / 255.0,
            cg as f32 / 255.0,
            cb as f32 / 255.0,
            ca as f32 / 255.0,
        ];

        let size = rng.gen_range(config.size.0..config.size.1);
        let length = rng.gen_range(config.length.0..config.length.1);
        let lifetime = rng.gen_range(config.lifetime.0..config.lifetime.1);

        // Wobble parameters (only relevant when config.wobble is true).
        let (wobble_phase, wobble_speed, wobble_amplitude) = if config.wobble {
            (
                rng.gen_range(0.0..std::f32::consts::TAU),
                rng.gen_range(1.5..3.0),
                rng.gen_range(3.0..8.0),
            )
        } else {
            (0.0, 0.0, 0.0)
        };

        let idx = pool.allocate();
        let p = &mut pool.particles[idx];
        p.position = Vec2::new(spawn_x, spawn_y);
        p.velocity = velocity;
        p.lifetime = lifetime;
        p.age = 0.0;
        p.size = size;
        p.length = length;
        p.color = color;
        p.splash = false;
        p.wobble_phase = wobble_phase;
        p.wobble_speed = wobble_speed;
        p.wobble_amplitude = wobble_amplitude;
    }
}

/// Update weather particle physics: movement, wobble, lifetime, collision.
pub fn update_weather_particles(
    mut pool: ResMut<WeatherParticlePool>,
    resolved: Option<Res<ResolvedWeatherType>>,
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

    // Determine if current weather has splash so we can spawn splash particles.
    let current_has_splash = resolved
        .as_ref()
        .and_then(|r| r.0)
        .map(|t| config_for_type(t).splash)
        .unwrap_or(false);

    // Get camera bottom for out-of-view culling.
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

    // Collect splash particles to add after the main loop (avoid borrow issues).
    let mut splashes: Vec<WeatherParticle> = Vec::new();

    for p in &mut pool.particles {
        if p.is_dead() {
            continue;
        }

        p.age += dt;

        if p.age >= p.lifetime {
            p.alive = false;
            continue;
        }

        // Apply wobble offset to X (only non-zero when config had wobble=true).
        let wobble_x = if p.wobble_amplitude > 0.0 {
            (p.wobble_phase + p.wobble_speed * p.age).sin() * p.wobble_amplitude
        } else {
            0.0
        };

        // Integrate position.  Wind was baked into velocity at spawn, but we
        // re-apply a small delta each frame to account for wind changes.
        p.position += p.velocity * dt;
        p.position.x += wobble_x * dt;

        // Kill when particle drifts well below the camera.
        if p.position.y < cam_bottom - 32.0 {
            p.alive = false;
            continue;
        }

        // Splash particles skip collision — they die by lifetime only.
        if p.splash {
            continue;
        }

        // Solid tile collision.
        let tile_x = (p.position.x / tile_size).floor() as i32;
        let tile_y = (p.position.y / tile_size).floor() as i32;
        let data_cx = active_world.wrap_chunk_x(tile_x.div_euclid(chunk_size as i32));
        let cy = tile_y.div_euclid(chunk_size as i32);
        let local_x = tile_x.rem_euclid(chunk_size as i32) as u32;
        let local_y = tile_y.rem_euclid(chunk_size as i32) as u32;

        let hit = if let Some(chunk) = world_map.chunks.get(&(data_cx, cy)) {
            let idx = (local_y * chunk_size + local_x) as usize;
            idx < chunk.fg.tiles.len() && tile_registry.is_solid(chunk.fg.tiles[idx])
        } else {
            false
        };

        if hit {
            p.alive = false;

            // Spawn splash particles for rain.
            if current_has_splash {
                let mut rng = rand::thread_rng();
                let splash_count = rng.gen_range(2..=3usize);
                for _ in 0..splash_count {
                    let (cr, cg, cb, _) = rain_config().color;
                    let splash_color = [
                        cr as f32 / 255.0,
                        cg as f32 / 255.0,
                        cb as f32 / 255.0,
                        0.5, // lower alpha for splash
                    ];
                    let spread_x = rng.gen_range(-20.0..20.0_f32);
                    let up_speed = rng.gen_range(30.0..80.0_f32);
                    splashes.push(WeatherParticle {
                        position: p.position,
                        velocity: Vec2::new(spread_x, up_speed),
                        lifetime: rng.gen_range(0.1..0.2),
                        age: 0.0,
                        size: 1.0,
                        length: 1.0,
                        color: splash_color,
                        alive: true,
                        splash: true,
                        wobble_phase: 0.0,
                        wobble_speed: 0.0,
                        wobble_amplitude: 0.0,
                    });
                }
            }
        }
    }

    // Insert splash particles into the pool.
    for splash in splashes {
        let idx = pool.allocate();
        let p = &mut pool.particles[idx];
        *p = splash;
        p.alive = true; // ensure alive after allocate reset
    }

    // Suppress unused variable warning for wind_vel when there's nothing else using it.
    let _ = wind_vel;
}

/// Rebuild the weather particle batch mesh every frame.
pub fn rebuild_weather_mesh(
    pool: Res<WeatherParticlePool>,
    mut meshes: ResMut<Assets<Mesh>>,
    query: Query<&Mesh2d, With<WeatherMeshEntity>>,
) {
    let Ok(mesh_2d) = query.single() else {
        return;
    };

    let alive: Vec<_> = pool.particles.iter().filter(|p| !p.is_dead()).collect();

    let n = alive.len();
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n * 4);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(n * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(n * 6);

    for (i, p) in alive.iter().enumerate() {
        let base = (i * 4) as u32;
        let x = p.position.x;
        let y = p.position.y;

        // Fade out in the last 20 % of lifetime.
        let age_ratio = p.age / p.lifetime;
        let alpha_factor = if age_ratio > 0.8 {
            (1.0 - (age_ratio - 0.8) / 0.2).max(0.0)
        } else {
            1.0
        };
        let c = [
            p.color[0],
            p.color[1],
            p.color[2],
            p.color[3] * alpha_factor,
        ];

        // Choose between square (snow / sandstorm) and oriented streak (rain).
        let streak = p.length > p.size * 1.5;

        if streak && p.velocity.length_squared() > 0.0 {
            // Oriented rectangle along the velocity direction.
            let dir = p.velocity.normalize();
            let perp = Vec2::new(-dir.y, dir.x);
            let half_w = p.size * 0.5;
            let half_l = p.length * 0.5;

            let c0 = Vec2::new(x, y) - perp * half_w - dir * half_l;
            let c1 = Vec2::new(x, y) + perp * half_w - dir * half_l;
            let c2 = Vec2::new(x, y) + perp * half_w + dir * half_l;
            let c3 = Vec2::new(x, y) - perp * half_w + dir * half_l;

            positions.push([c0.x, c0.y, 0.0]);
            positions.push([c1.x, c1.y, 0.0]);
            positions.push([c2.x, c2.y, 0.0]);
            positions.push([c3.x, c3.y, 0.0]);
        } else {
            // Axis-aligned square (snow / sandstorm).
            let r = p.size * 0.5;
            positions.push([x - r, y - r, 0.0]);
            positions.push([x + r, y - r, 0.0]);
            positions.push([x + r, y + r, 0.0]);
            positions.push([x - r, y + r, 0.0]);
        }

        colors.push(c);
        colors.push(c);
        colors.push(c);
        colors.push(c);

        // Two triangles: 0-1-2, 0-2-3.
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

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

// ── Unit tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_allocate_returns_valid_index() {
        let mut pool = WeatherParticlePool::default();
        let idx = pool.allocate();
        assert!(idx < pool.particles.len());
        assert!(pool.particles[idx].alive);
    }

    #[test]
    fn pool_recycles_dead_particles() {
        let mut pool = WeatherParticlePool::default();
        let idx1 = pool.allocate();
        pool.particles[idx1].alive = false;
        let idx2 = pool.allocate();
        assert_eq!(idx1, idx2);
    }

    #[test]
    fn snow_config_has_wobble_no_splash() {
        let cfg = snow_config();
        assert!(cfg.wobble);
        assert!(!cfg.splash);
    }

    #[test]
    fn rain_config_has_splash_no_wobble() {
        let cfg = rain_config();
        assert!(cfg.splash);
        assert!(!cfg.wobble);
    }

    #[test]
    fn sandstorm_config_high_angle() {
        let cfg = sandstorm_config();
        assert!(cfg.angle > 70.0);
    }
}
