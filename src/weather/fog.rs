use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use rand::Rng;

use crate::weather::precipitation::{PrecipitationType, ResolvedWeatherType};
use crate::weather::weather_state::WeatherState;
use crate::weather::wind::Wind;

/// Marker component for the fullscreen fog overlay sprite.
#[derive(Component)]
pub struct FogOverlay;

/// Component for individual drifting fog cloud sprites.
#[derive(Component)]
pub struct FogCloud {
    pub drift_speed: f32,
    pub alpha_phase: f32,
    pub alpha_speed: f32,
    pub base_alpha: f32,
}

/// Resource holding the procedurally generated fog cloud texture.
#[derive(Resource)]
pub struct FogCloudTexture {
    pub handle: Handle<Image>,
}

/// Generate a 64x32 fog cloud image with a radial gradient.
fn generate_fog_cloud_image() -> Image {
    let width = 64u32;
    let height = 32u32;
    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    let radius_x = center_x;
    let radius_y = center_y;
    let max_alpha: f32 = 80.0;

    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let dx = (x as f32 - center_x) / radius_x;
            let dy = (y as f32 - center_y) / radius_y;
            let dist = (dx * dx + dy * dy).sqrt();
            let alpha = (max_alpha * (1.0 - dist.powi(2)).max(0.0)) as u8;
            data.extend_from_slice(&[255, 255, 255, alpha]);
        }
    }

    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    )
}

/// System that initializes fog resources and spawns fog entities on entering InGame.
pub fn init_fog(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let handle = images.add(generate_fog_cloud_image());
    let texture_handle = handle.clone();
    commands.insert_resource(FogCloudTexture { handle });

    // Spawn the fullscreen fog overlay (below particles at z=3.0)
    commands.spawn((
        FogOverlay,
        Sprite {
            color: Color::srgba(1.0, 1.0, 1.0, 0.0),
            custom_size: Some(Vec2::new(2000.0, 1200.0)),
            ..default()
        },
        Transform::from_translation(Vec3::new(0.0, 0.0, 2.5)),
    ));

    // Spawn 8 drifting fog cloud entities
    let mut rng = rand::thread_rng();
    for i in 0..8 {
        let x = -1000.0 + i as f32 * 250.0;
        let y = rng.gen_range(-200.0_f32..200.0_f32);
        let drift_speed = rng.gen_range(10.0_f32..30.0_f32);
        let alpha_phase = rng.gen_range(0.0_f32..std::f32::consts::TAU);
        let alpha_speed = rng.gen_range(0.3_f32..0.8_f32);
        let base_alpha = rng.gen_range(0.3_f32..0.6_f32);

        commands.spawn((
            FogCloud {
                drift_speed,
                alpha_phase,
                alpha_speed,
                base_alpha,
            },
            Sprite {
                image: texture_handle.clone(),
                color: Color::srgba(1.0, 1.0, 1.0, 0.0),
                custom_size: Some(Vec2::new(400.0, 200.0)),
                ..default()
            },
            Transform::from_translation(Vec3::new(x, y, 2.6)),
        ));
    }
}

/// System that updates the fullscreen fog overlay alpha and position.
pub fn update_fog_overlay(
    weather: Res<WeatherState>,
    resolved: Res<ResolvedWeatherType>,
    time: Res<Time>,
    camera_q: Query<&Transform, With<Camera2d>>,
    mut overlay_q: Query<(&mut Sprite, &mut Transform), (With<FogOverlay>, Without<Camera2d>)>,
) {
    let Ok(cam_tf) = camera_q.single() else {
        return;
    };

    let is_fog = resolved.0.as_ref() == Some(&PrecipitationType::Fog);
    let target_alpha = if is_fog {
        weather.intensity() * 0.3
    } else {
        0.0
    };

    let dt = time.delta_secs();

    for (mut sprite, mut transform) in overlay_q.iter_mut() {
        // Follow camera
        transform.translation.x = cam_tf.translation.x;
        transform.translation.y = cam_tf.translation.y;

        // Lerp alpha toward target
        let current_alpha = sprite.color.alpha();
        let new_alpha = current_alpha + (target_alpha - current_alpha) * (0.5 * dt);
        sprite.color.set_alpha(new_alpha);
    }
}

/// System that updates drifting fog cloud sprites.
pub fn update_fog_clouds(
    weather: Res<WeatherState>,
    resolved: Res<ResolvedWeatherType>,
    wind: Res<Wind>,
    time: Res<Time>,
    camera_q: Query<&Transform, (With<Camera2d>, Without<FogCloud>)>,
    mut cloud_q: Query<(&mut FogCloud, &mut Sprite, &mut Transform), Without<Camera2d>>,
) {
    let is_fog = resolved.0.as_ref() == Some(&PrecipitationType::Fog);
    let dt = time.delta_secs();

    let cam_x = camera_q
        .single()
        .map(|t| t.translation.x)
        .unwrap_or(0.0);

    for (mut cloud, mut sprite, mut transform) in cloud_q.iter_mut() {
        if !is_fog {
            // Fade out
            let current_alpha = sprite.color.alpha();
            let new_alpha = current_alpha + (0.0 - current_alpha) * (0.5 * dt);
            sprite.color.set_alpha(new_alpha);
        } else {
            // Drift along wind
            transform.translation.x +=
                wind.velocity().x * 0.3 * dt + cloud.drift_speed * dt;

            // Pulse alpha sinusoidally
            cloud.alpha_phase += cloud.alpha_speed * dt;
            let alpha =
                (cloud.base_alpha + 0.15 * cloud.alpha_phase.sin()) * weather.intensity();
            sprite.color.set_alpha(alpha.clamp(0.0, 1.0));

            // Wrap clouds around camera
            let dist_x = (transform.translation.x - cam_x).abs();
            if dist_x > 1200.0 {
                let side = if transform.translation.x > cam_x {
                    -1.0
                } else {
                    1.0
                };
                transform.translation.x = cam_x + side * 1100.0;
            }
        }
    }
}
