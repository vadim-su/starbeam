use std::collections::HashMap;
use std::collections::HashSet;

use bevy::prelude::*;

use crate::registry::tile::{TileId, TileRegistry};
use crate::registry::world::WorldConfig;
use crate::world::atlas::TileAtlas;
use crate::world::autotile::{compute_bitmask, AutotileRegistry};
use crate::world::ctx::{WorldCtx, WorldCtxRef};
use crate::world::lighting;
use crate::world::mesh_builder::{build_chunk_mesh, MeshBuildBuffers};
use crate::world::terrain_gen;
use crate::world::tile_renderer::SharedTileMaterial;

/// Marker component on tilemap entities to identify which chunk they represent.
#[derive(Component)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
}

/// Marker component indicating a chunk's mesh needs rebuilding.
#[derive(Component)]
pub struct ChunkDirty;

/// Tile data for a single chunk. Row-major: index = local_y * chunk_size + local_x.
pub struct ChunkData {
    pub tiles: Vec<TileId>,
    pub bitmasks: Vec<u8>,
    /// Per-tile RGB light level: [0,0,0] = full dark, [255,255,255] = full light.
    pub light_levels: Vec<[u8; 3]>,
    #[allow(dead_code)] // Reserved for future block-damage system
    pub damage: Vec<u8>,
}

impl ChunkData {
    pub fn get(&self, local_x: u32, local_y: u32, chunk_size: u32) -> TileId {
        self.tiles[(local_y * chunk_size + local_x) as usize]
    }

    pub fn set(&mut self, local_x: u32, local_y: u32, tile: TileId, chunk_size: u32) {
        self.tiles[(local_y * chunk_size + local_x) as usize] = tile;
    }
}

/// Authoritative world tile data. Chunks are lazily generated and cached.
#[derive(Resource, Default)]
pub struct WorldMap {
    pub(crate) chunks: HashMap<(i32, i32), ChunkData>,
}

impl WorldMap {
    /// Read-only access to a chunk by coordinates.
    pub fn chunk(&self, cx: i32, cy: i32) -> Option<&ChunkData> {
        self.chunks.get(&(cx, cy))
    }

    /// Mutable access to a chunk by coordinates.
    #[allow(dead_code)] // public API for future use
    pub fn chunk_mut(&mut self, cx: i32, cy: i32) -> Option<&mut ChunkData> {
        self.chunks.get_mut(&(cx, cy))
    }
}

impl WorldMap {
    pub fn get_or_generate_chunk(
        &mut self,
        chunk_x: i32,
        chunk_y: i32,
        ctx: &WorldCtxRef,
    ) -> &ChunkData {
        self.chunks.entry((chunk_x, chunk_y)).or_insert_with(|| {
            let tiles = terrain_gen::generate_chunk_tiles(chunk_x, chunk_y, ctx);
            let len = tiles.len();
            ChunkData {
                tiles,
                bitmasks: vec![0; len],
                light_levels: vec![[0, 0, 0]; len],
                damage: vec![0; len],
            }
        })
    }

    /// Read-only: returns tile if chunk is loaded, None otherwise.
    /// Takes &self — safe for parallel access.
    pub fn get_tile(&self, tile_x: i32, tile_y: i32, ctx: &WorldCtxRef) -> Option<TileId> {
        if tile_y < 0 {
            return Some(ctx.tile_registry.by_name("stone"));
        }
        if tile_y >= ctx.config.height_tiles {
            return Some(TileId::AIR);
        }
        let wrapped_x = ctx.config.wrap_tile_x(tile_x);
        let (cx, cy) = tile_to_chunk(wrapped_x, tile_y, ctx.config.chunk_size);
        let (lx, ly) = tile_to_local(wrapped_x, tile_y, ctx.config.chunk_size);
        self.chunks
            .get(&(cx, cy))
            .map(|chunk| chunk.get(lx, ly, ctx.config.chunk_size))
    }

    /// Mutating: gets tile with lazy chunk generation.
    /// Only for systems that need to generate world (chunk_loading, block_action).
    pub fn get_tile_mut(&mut self, tile_x: i32, tile_y: i32, ctx: &WorldCtxRef) -> TileId {
        if tile_y < 0 {
            return ctx.tile_registry.by_name("stone"); // bedrock
        }
        if tile_y >= ctx.config.height_tiles {
            return TileId::AIR; // sky
        }
        let wrapped_x = ctx.config.wrap_tile_x(tile_x);
        let (cx, cy) = tile_to_chunk(wrapped_x, tile_y, ctx.config.chunk_size);
        let (lx, ly) = tile_to_local(wrapped_x, tile_y, ctx.config.chunk_size);
        self.get_or_generate_chunk(cx, cy, ctx)
            .get(lx, ly, ctx.config.chunk_size)
    }

    pub fn set_tile(&mut self, tile_x: i32, tile_y: i32, tile: TileId, ctx: &WorldCtxRef) {
        if tile_y < 0 || tile_y >= ctx.config.height_tiles {
            return;
        }
        let wrapped_x = ctx.config.wrap_tile_x(tile_x);
        let (cx, cy) = tile_to_chunk(wrapped_x, tile_y, ctx.config.chunk_size);
        let (lx, ly) = tile_to_local(wrapped_x, tile_y, ctx.config.chunk_size);
        self.get_or_generate_chunk(cx, cy, ctx);
        self.chunks
            .get_mut(&(cx, cy))
            .unwrap()
            .set(lx, ly, tile, ctx.config.chunk_size);
    }

    /// Read-only: returns whether tile is solid (false for unloaded chunks).
    pub fn is_solid(&self, tile_x: i32, tile_y: i32, ctx: &WorldCtxRef) -> bool {
        self.get_tile(tile_x, tile_y, ctx)
            .is_some_and(|tile| ctx.tile_registry.is_solid(tile))
    }
}

/// Tracks which chunks currently have spawned tilemap entities.
#[derive(Resource, Default)]
pub struct LoadedChunks {
    pub(crate) map: HashMap<(i32, i32), Entity>,
}

// --- Coordinate conversion helpers ---

pub fn tile_to_chunk(tile_x: i32, tile_y: i32, chunk_size: u32) -> (i32, i32) {
    (
        tile_x.div_euclid(chunk_size as i32),
        tile_y.div_euclid(chunk_size as i32),
    )
}

pub fn tile_to_local(tile_x: i32, tile_y: i32, chunk_size: u32) -> (u32, u32) {
    (
        tile_x.rem_euclid(chunk_size as i32) as u32,
        tile_y.rem_euclid(chunk_size as i32) as u32,
    )
}

pub fn world_to_tile(world_x: f32, world_y: f32, tile_size: f32) -> (i32, i32) {
    (
        (world_x / tile_size).floor() as i32,
        (world_y / tile_size).floor() as i32,
    )
}

/// Recompute bitmasks for 3x3 area around (center_x, center_y).
/// Returns set of affected data chunk coords that need mesh rebuild.
pub fn update_bitmasks_around(
    world_map: &mut WorldMap,
    center_x: i32,
    center_y: i32,
    ctx: &WorldCtxRef,
) -> HashSet<(i32, i32)> {
    let mut dirty_chunks = HashSet::new();

    for dy in -1..=1 {
        for dx in -1..=1 {
            let x = center_x + dx;
            let y = center_y + dy;

            if y < 0 || y >= ctx.config.height_tiles {
                continue;
            }

            let wrapped_x = ctx.config.wrap_tile_x(x);
            let (cx, cy) = tile_to_chunk(wrapped_x, y, ctx.config.chunk_size);
            let (lx, ly) = tile_to_local(wrapped_x, y, ctx.config.chunk_size);
            let idx = (ly * ctx.config.chunk_size + lx) as usize;

            let new_mask = compute_bitmask(
                |bx, by| {
                    let tile = world_map.get_tile_mut(bx, by, ctx);
                    ctx.tile_registry.is_solid(tile)
                },
                wrapped_x,
                y,
            );

            if let Some(chunk) = world_map.chunks.get_mut(&(cx, cy)) {
                chunk.bitmasks[idx] = new_mask;
                dirty_chunks.insert((cx, cy));
            }
        }
    }

    dirty_chunks
}

/// Compute bitmasks for all tiles in a chunk using neighbor solidity checks.
pub fn init_chunk_bitmasks(
    world_map: &mut WorldMap,
    chunk_x: i32,
    chunk_y: i32,
    ctx: &WorldCtxRef,
) -> Vec<u8> {
    let chunk_size = ctx.config.chunk_size;
    let mut bitmasks = vec![0u8; (chunk_size * chunk_size) as usize];
    let base_x = chunk_x * chunk_size as i32;
    let base_y = chunk_y * chunk_size as i32;

    for local_y in 0..chunk_size {
        for local_x in 0..chunk_size {
            let world_x = base_x + local_x as i32;
            let world_y = base_y + local_y as i32;
            let idx = (local_y * chunk_size + local_x) as usize;
            bitmasks[idx] = compute_bitmask(
                |x, y| {
                    let tile = world_map.get_tile_mut(x, y, ctx);
                    ctx.tile_registry.is_solid(tile)
                },
                world_x,
                world_y,
            );
        }
    }
    bitmasks
}

/// Spawn a chunk entity with a built mesh and material.
#[allow(clippy::too_many_arguments)]
pub fn spawn_chunk(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    world_map: &mut WorldMap,
    loaded_chunks: &mut LoadedChunks,
    ctx: &WorldCtxRef,
    autotile_registry: &AutotileRegistry,
    atlas: &TileAtlas,
    material: &SharedTileMaterial,
    buffers: &mut MeshBuildBuffers,
    display_chunk_x: i32,
    chunk_y: i32,
) {
    if loaded_chunks.map.contains_key(&(display_chunk_x, chunk_y)) {
        return;
    }

    let data_chunk_x = ctx.config.wrap_chunk_x(display_chunk_x);
    world_map.get_or_generate_chunk(data_chunk_x, chunk_y, ctx);

    let bitmasks = init_chunk_bitmasks(world_map, data_chunk_x, chunk_y, ctx);
    if let Some(chunk) = world_map.chunks.get_mut(&(data_chunk_x, chunk_y)) {
        chunk.bitmasks = bitmasks;
    }

    // Compute lighting (immutable borrow for compute, then mutable to write back)
    let light_levels = lighting::compute_chunk_lighting(&*world_map, data_chunk_x, chunk_y, ctx);
    if let Some(chunk) = world_map.chunks.get_mut(&(data_chunk_x, chunk_y)) {
        chunk.light_levels = light_levels;
    }

    let chunk_data = &world_map.chunks[&(data_chunk_x, chunk_y)];
    let mesh = build_chunk_mesh(
        &chunk_data.tiles,
        &chunk_data.bitmasks,
        &chunk_data.light_levels,
        display_chunk_x,
        chunk_y,
        ctx.config.chunk_size,
        ctx.config.tile_size,
        ctx.config.seed,
        ctx.tile_registry,
        autotile_registry,
        &atlas.params,
        buffers,
    );

    let mesh_handle = meshes.add(mesh);

    let entity = commands
        .spawn((
            ChunkCoord {
                x: display_chunk_x,
                y: chunk_y,
            },
            Mesh2d(mesh_handle),
            MeshMaterial2d(material.handle.clone()),
            Transform::from_translation(Vec3::ZERO),
            Visibility::default(),
        ))
        .id();

    loaded_chunks.map.insert((display_chunk_x, chunk_y), entity);
}

pub fn despawn_chunk(
    commands: &mut Commands,
    loaded_chunks: &mut LoadedChunks,
    chunk_x: i32,
    chunk_y: i32,
) {
    if let Some(entity) = loaded_chunks.map.remove(&(chunk_x, chunk_y)) {
        commands.entity(entity).despawn();
    }
}

#[allow(clippy::too_many_arguments)]
pub fn chunk_loading_system(
    mut commands: Commands,
    camera_query: Query<&Transform, With<Camera2d>>,
    ctx: WorldCtx,
    mut world_map: ResMut<WorldMap>,
    mut loaded_chunks: ResMut<LoadedChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    autotile_registry: Res<AutotileRegistry>,
    atlas: Res<TileAtlas>,
    material: Res<SharedTileMaterial>,
    mut buffers: ResMut<MeshBuildBuffers>,
) {
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };
    let camera_pos = camera_transform.translation.truncate();
    let ctx_ref = ctx.as_ref();

    let (cam_tile_x, cam_tile_y) =
        world_to_tile(camera_pos.x, camera_pos.y, ctx_ref.config.tile_size);
    let (cam_chunk_x, cam_chunk_y) =
        tile_to_chunk(cam_tile_x, cam_tile_y, ctx_ref.config.chunk_size);

    let mut desired: HashSet<(i32, i32)> = HashSet::new();
    let load_radius = ctx_ref.config.chunk_load_radius;
    let world_chunks = ctx_ref.config.width_chunks();

    let mut add_chunks_around = |center_cx: i32| {
        for display_cx in (center_cx - load_radius)..=(center_cx + load_radius) {
            for cy in (cam_chunk_y - load_radius)..=(cam_chunk_y + load_radius) {
                if cy >= 0 && cy < ctx_ref.config.height_chunks() {
                    desired.insert((display_cx, cy));
                }
            }
        }
    };

    add_chunks_around(cam_chunk_x);

    if cam_chunk_x < load_radius {
        add_chunks_around(cam_chunk_x + world_chunks);
    } else if cam_chunk_x >= world_chunks - load_radius {
        add_chunks_around(cam_chunk_x - world_chunks);
    }

    for &(display_cx, cy) in &desired {
        if !loaded_chunks.map.contains_key(&(display_cx, cy)) {
            spawn_chunk(
                &mut commands,
                &mut meshes,
                &mut world_map,
                &mut loaded_chunks,
                &ctx_ref,
                &autotile_registry,
                &atlas,
                &material,
                &mut buffers,
                display_cx,
                cy,
            );
        }
    }

    let to_remove: Vec<(i32, i32)> = loaded_chunks
        .map
        .keys()
        .filter(|k| !desired.contains(k))
        .copied()
        .collect();
    for (cx, cy) in to_remove {
        despawn_chunk(&mut commands, &mut loaded_chunks, cx, cy);
    }
}

/// Rebuild meshes for chunks marked as dirty (e.g. after tile modification).
#[allow(clippy::too_many_arguments)]
pub fn rebuild_dirty_chunks(
    mut commands: Commands,
    query: Query<(Entity, &ChunkCoord), With<ChunkDirty>>,
    mut meshes: ResMut<Assets<Mesh>>,
    world_map: Res<WorldMap>,
    wc: Res<WorldConfig>,
    registry: Res<TileRegistry>,
    autotile_registry: Res<AutotileRegistry>,
    atlas: Res<TileAtlas>,
    mut buffers: ResMut<MeshBuildBuffers>,
) {
    for (entity, coord) in &query {
        let data_chunk_x = wc.wrap_chunk_x(coord.x);
        let Some(chunk_data) = world_map.chunks.get(&(data_chunk_x, coord.y)) else {
            continue;
        };

        let mesh = build_chunk_mesh(
            &chunk_data.tiles,
            &chunk_data.bitmasks,
            &chunk_data.light_levels,
            coord.x,
            coord.y,
            wc.chunk_size,
            wc.tile_size,
            wc.seed,
            &registry,
            &autotile_registry,
            &atlas.params,
            &mut buffers,
        );

        let mesh_handle = meshes.add(mesh);
        commands
            .entity(entity)
            .insert(Mesh2d(mesh_handle))
            .remove::<ChunkDirty>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::fixtures;

    #[test]
    fn tile_to_chunk_basic() {
        let wc = fixtures::test_world_config();
        assert_eq!(tile_to_chunk(0, 0, wc.chunk_size), (0, 0));
        assert_eq!(tile_to_chunk(31, 31, wc.chunk_size), (0, 0));
        assert_eq!(tile_to_chunk(32, 0, wc.chunk_size), (1, 0));
        assert_eq!(tile_to_chunk(63, 63, wc.chunk_size), (1, 1));
    }

    #[test]
    fn tile_to_local_basic() {
        let wc = fixtures::test_world_config();
        assert_eq!(tile_to_local(0, 0, wc.chunk_size), (0, 0));
        assert_eq!(tile_to_local(31, 31, wc.chunk_size), (31, 31));
        assert_eq!(tile_to_local(32, 0, wc.chunk_size), (0, 0));
        assert_eq!(tile_to_local(33, 35, wc.chunk_size), (1, 3));
    }

    #[test]
    fn world_to_tile_basic() {
        let wc = fixtures::test_world_config();
        assert_eq!(world_to_tile(0.0, 0.0, wc.tile_size), (0, 0));
        assert_eq!(world_to_tile(32.0, 0.0, wc.tile_size), (1, 0));
        assert_eq!(world_to_tile(31.9, 63.9, wc.tile_size), (0, 1));
        assert_eq!(world_to_tile(64.0, 64.0, wc.tile_size), (2, 2));
    }

    #[test]
    fn world_to_tile_negative() {
        let wc = fixtures::test_world_config();
        assert_eq!(world_to_tile(-1.0, -1.0, wc.tile_size), (-1, -1));
        assert_eq!(world_to_tile(-32.0, 0.0, wc.tile_size), (-1, 0));
    }

    #[test]
    fn worldmap_get_tile_mut_deterministic() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let mut map = WorldMap::default();
        let t1 = map.get_tile_mut(100, 500, &ctx);
        let t2 = map.get_tile_mut(100, 500, &ctx);
        assert_eq!(t1, t2);
    }

    #[test]
    fn worldmap_get_tile_returns_none_for_unloaded() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let map = WorldMap::default();
        assert_eq!(map.get_tile(100, 500, &ctx), None);
    }

    #[test]
    fn worldmap_get_tile_returns_some_for_loaded() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let mut map = WorldMap::default();
        // Pre-generate the chunk via get_tile_mut
        let expected = map.get_tile_mut(100, 500, &ctx);
        // Read-only get_tile should return the same value
        assert_eq!(map.get_tile(100, 500, &ctx), Some(expected));
    }

    #[test]
    fn worldmap_is_solid_returns_false_for_unloaded() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let map = WorldMap::default();
        assert!(!map.is_solid(100, 500, &ctx));
    }

    #[test]
    fn worldmap_set_tile() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let mut map = WorldMap::default();
        map.set_tile(100, 500, TileId::AIR, &ctx);
        assert_eq!(map.get_tile(100, 500, &ctx), Some(TileId::AIR));
    }

    #[test]
    fn worldmap_y_out_of_bounds() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let map = WorldMap::default();
        assert_eq!(map.get_tile(0, wc.height_tiles, &ctx), Some(TileId::AIR));
        assert_eq!(map.get_tile(0, -1, &ctx), Some(tr.by_name("stone")));
    }

    #[test]
    fn worldmap_x_wraps() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let mut map = WorldMap::default();
        // Use get_tile_mut to lazily generate chunks for wrap test
        let t1 = map.get_tile_mut(-1, 500, &ctx);
        let t2 = map.get_tile_mut(wc.width_tiles - 1, 500, &ctx);
        assert_eq!(t1, t2);

        let t3 = map.get_tile_mut(wc.width_tiles, 500, &ctx);
        let t4 = map.get_tile_mut(0, 500, &ctx);
        assert_eq!(t3, t4);
    }

    #[test]
    fn worldmap_set_tile_wraps() {
        let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
        let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
        let mut map = WorldMap::default();
        map.set_tile(-1, 500, TileId::AIR, &ctx);
        assert_eq!(
            map.get_tile(wc.width_tiles - 1, 500, &ctx),
            Some(TileId::AIR)
        );
    }
}
