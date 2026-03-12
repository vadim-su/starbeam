use bevy::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::combat::block_damage::BlockDamageMap;
use crate::registry::tile::TileId;
use crate::registry::AppState;
use crate::sets::GameSet;
use crate::world::chunk::{Layer, WorldMap};
use crate::world::ctx::WorldCtx;

/// Marker component for crack overlay entities.
#[derive(Component)]
pub struct CrackOverlay {
    pub tile_x: i32,
    pub tile_y: i32,
}

/// Resource holding procedurally generated crack textures for each damage stage.
#[derive(Resource)]
pub struct CrackTextures {
    pub stages: [Handle<Image>; 4],
}

/// Generate a 16x16 RGBA crack texture for the given stage (0-3).
/// More cracks appear at higher stages.
fn generate_crack_image(stage: usize) -> Image {
    let mut data = vec![0u8; 16 * 16 * 4];

    // Define crack lines per stage: each is a list of (x0, y0) -> (x1, y1) segments
    let crack_sets: &[&[(i32, i32, i32, i32)]] = &[
        // Stage 0 (~25%): 1-2 crack lines
        &[(3, 2, 7, 9), (10, 5, 13, 12)],
        // Stage 1 (~50%): 2-3 crack lines
        &[(2, 1, 8, 10), (10, 4, 14, 13), (5, 7, 9, 15)],
        // Stage 2 (~75%): 3-4 crack lines
        &[
            (1, 2, 7, 11),
            (9, 3, 14, 12),
            (4, 6, 10, 15),
            (6, 0, 3, 8),
        ],
        // Stage 3 (~100%): 5-6 crack lines
        &[
            (1, 1, 6, 10),
            (8, 2, 14, 11),
            (3, 5, 9, 15),
            (11, 0, 7, 8),
            (0, 8, 5, 14),
            (12, 6, 15, 13),
        ],
    ];

    let lines = crack_sets[stage.min(3)];

    for &(x0, y0, x1, y1) in lines {
        draw_line(&mut data, x0, y0, x1, y1);
    }

    Image::new(
        Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Draw a line from (x0, y0) to (x1, y1) using Bresenham's algorithm,
/// writing semi-transparent black pixels.
fn draw_line(data: &mut [u8], x0: i32, y0: i32, x1: i32, y1: i32) {
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };

    let mut x = x0;
    let mut y = y0;
    let mut err = dx - dy;

    loop {
        if x >= 0 && x < 16 && y >= 0 && y < 16 {
            let idx = (y as usize * 16 + x as usize) * 4;
            data[idx] = 0;       // R
            data[idx + 1] = 0;   // G
            data[idx + 2] = 0;   // B
            data[idx + 3] = 160; // A — semi-transparent
        }

        if x == x1 && y == y1 {
            break;
        }

        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
}

/// System that runs on `OnEnter(AppState::InGame)` to create the CrackTextures resource.
pub fn init_crack_textures(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let stages = [
        images.add(generate_crack_image(0)),
        images.add(generate_crack_image(1)),
        images.add(generate_crack_image(2)),
        images.add(generate_crack_image(3)),
    ];
    commands.insert_resource(CrackTextures { stages });
}

/// System that syncs crack overlay entities with the current BlockDamageMap.
pub fn update_crack_overlays(
    mut commands: Commands,
    crack_textures: Option<Res<CrackTextures>>,
    block_damage: Res<BlockDamageMap>,
    world_map: Res<WorldMap>,
    ctx: WorldCtx,
    mut existing: Query<(Entity, &CrackOverlay, &mut Sprite)>,
) {
    let Some(textures) = crack_textures else {
        return;
    };

    let ctx_ref = ctx.as_ref();
    let tile_size = ctx_ref.config.tile_size;

    // Update or despawn existing overlays.
    for (entity, overlay, mut sprite) in existing.iter_mut() {
        let key = (overlay.tile_x, overlay.tile_y);
        if let Some(state) = block_damage.damage.get(&key) {
            // Tile still damaged — compute stage and update sprite image.
            let tile_id = world_map
                .get_tile(overlay.tile_x, overlay.tile_y, Layer::Fg, &ctx_ref)
                .unwrap_or(TileId::AIR);
            let hardness = if tile_id != TileId::AIR {
                ctx_ref.tile_registry.get(tile_id).hardness.max(0.001)
            } else {
                1.0
            };
            let stage =
                ((state.accumulated / hardness * 4.0) as usize).min(3);
            sprite.image = textures.stages[stage].clone();
        } else {
            // No longer damaged — despawn.
            commands.entity(entity).despawn();
        }
    }

    // Collect positions of existing overlays so we don't double-spawn.
    let existing_positions: std::collections::HashSet<(i32, i32)> = existing
        .iter()
        .map(|(_, overlay, _)| (overlay.tile_x, overlay.tile_y))
        .collect();

    // Spawn new overlays for newly damaged tiles.
    for (&(tx, ty), state) in block_damage.damage.iter() {
        if existing_positions.contains(&(tx, ty)) {
            continue;
        }

        let tile_id = world_map
            .get_tile(tx, ty, Layer::Fg, &ctx_ref)
            .unwrap_or(TileId::AIR);
        if tile_id == TileId::AIR {
            continue;
        }

        let hardness = ctx_ref.tile_registry.get(tile_id).hardness.max(0.001);
        let stage = ((state.accumulated / hardness * 4.0) as usize).min(3);

        let world_x = tx as f32 * tile_size + tile_size / 2.0;
        let world_y = ty as f32 * tile_size + tile_size / 2.0;

        commands.spawn((
            CrackOverlay { tile_x: tx, tile_y: ty },
            Sprite {
                image: textures.stages[stage].clone(),
                custom_size: Some(Vec2::splat(tile_size)),
                ..default()
            },
            Transform::from_translation(Vec3::new(world_x, world_y, 0.1)),
        ));
    }
}

/// Plugin registration helper — call from InteractionPlugin::build.
pub fn register(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), init_crack_textures)
        .add_systems(
            Update,
            update_crack_overlays
                .in_set(GameSet::Input)
                .run_if(in_state(AppState::InGame)),
        );
}
