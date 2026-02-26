# Autotile Custom Renderer Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace bevy_ecs_tilemap with a custom mesh-per-chunk tile renderer using Blob47 autotiling and a combined horizontal atlas.

**Architecture:** Each chunk (32×32) = one Entity with Mesh2d + custom Material2d. Per-type PNGs combined into horizontal atlas at load time. 8-bit bitmask autotiling with deterministic variant selection. Eager point updates on block change.

**Tech Stack:** Bevy 0.18.0, WGSL shaders, RON config, custom Material2d

**Design doc:** `docs/plans/2026-02-26-autotile-custom-renderer-design.md`

**Key context files:**
- `src/world/chunk.rs` — current chunk system (to be rewritten)
- `src/world/mod.rs` — WorldPlugin (to be rewritten)
- `src/registry/tile.rs` — TileDef, TileRegistry (to be modified)
- `src/registry/mod.rs` — asset loading pipeline (to be extended)
- `src/interaction/block_action.rs` — block break/place (to be rewritten)
- `assets/world/terrain/dirt.ron` — existing autotile RON format
- `assets/world/tiles.registry.ron` — tile definitions

**Current state:** Rendering is already broken (`tiles.png` does not exist). Game compiles but shows no tiles.

---

## Task 1: Remove bevy_ecs_tilemap

Remove the dependency entirely. Stub out functions that used it so the project compiles.

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/main.rs`
- Modify: `src/world/chunk.rs`
- Modify: `src/interaction/block_action.rs`
- Modify: `src/world/mod.rs`

**Step 1: Remove from Cargo.toml**

```toml
# Remove this line:
bevy_ecs_tilemap = "0.18"
```

**Step 2: Remove from main.rs**

Remove `use bevy_ecs_tilemap::prelude::*;` (line 11) and `.add_plugins(TilemapPlugin)` (line 28).

**Step 3: Rewrite chunk.rs — remove all bevy_ecs_tilemap types**

Remove `use bevy_ecs_tilemap::prelude::*;` (line 5).

Remove `TilemapTextureHandle` resource.

Rewrite `spawn_chunk` as a minimal stub:

```rust
pub fn spawn_chunk(
    commands: &mut Commands,
    world_map: &mut WorldMap,
    loaded_chunks: &mut LoadedChunks,
    wc: &WorldConfig,
    tt: &TerrainTiles,
    display_chunk_x: i32,
    chunk_y: i32,
) {
    if loaded_chunks.map.contains_key(&(display_chunk_x, chunk_y)) {
        return;
    }
    let data_chunk_x = wc.wrap_chunk_x(display_chunk_x);
    let _chunk_data = world_map.get_or_generate_chunk(data_chunk_x, chunk_y, wc, tt);
    // TODO: build mesh + spawn entity (Task 6)
    let entity = commands.spawn(ChunkCoord { x: display_chunk_x, y: chunk_y }).id();
    loaded_chunks.map.insert((display_chunk_x, chunk_y), entity);
}
```

Update `chunk_loading_system` — remove `texture_handle: Res<TilemapTextureHandle>` parameter and the reference in `spawn_chunk` call.

**Step 4: Rewrite block_action.rs — remove all bevy_ecs_tilemap types**

Remove `use bevy_ecs_tilemap::prelude::*;` (line 3).

Remove `tilemap_query` parameter and all `TileStorage`/`TilePos`/`TilemapId`/`TileBundle` usage.

Simplify to just update WorldMap (visual update will come in Task 7):

```rust
pub fn block_interaction_system(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    player_query: Query<&Transform, With<Player>>,
    player_config: Res<PlayerConfig>,
    world_config: Res<WorldConfig>,
    terrain_tiles: Res<TerrainTiles>,
    tile_registry: Res<TileRegistry>,
    mut world_map: ResMut<WorldMap>,
) {
    // ... keep all the input/range checking logic ...
    
    if left_click {
        let current = world_map.get_tile(tile_x, tile_y, &world_config, &terrain_tiles);
        if !tile_registry.is_solid(current) { return; }
        world_map.set_tile(tile_x, tile_y, TileId::AIR, &world_config, &terrain_tiles);
        // TODO: update bitmasks + mark dirty (Task 7)
    } else if right_click {
        // ... keep overlap check ...
        let place_id = tile_registry.by_name("dirt");
        world_map.set_tile(tile_x, tile_y, place_id, &world_config, &terrain_tiles);
        // TODO: update bitmasks + mark dirty (Task 7)
    }
}
```

**Step 5: Update world/mod.rs**

Remove `TilemapTextureHandle` import. Remove `load_tile_atlas` system and its registration in `OnEnter(AppState::InGame)`.

```rust
pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldMap>()
            .init_resource::<LoadedChunks>()
            .add_systems(
                Update,
                chunk::chunk_loading_system.run_if(in_state(AppState::InGame)),
            );
    }
}
```

**Step 6: Verify compilation**

Run: `cargo build`
Expected: Compiles with no errors. Game runs, no tiles visible (expected — rendering not yet implemented).

**Step 7: Run existing tests**

Run: `cargo test`
Expected: All existing tests pass.

**Step 8: Commit**

```bash
git add -A && git commit -m "refactor: remove bevy_ecs_tilemap dependency"
```

---

## Task 2: Update TileDef data model

Add `autotile` field, add `damage_on_contact` and `effects` fields. Remove `texture_index`. Update RON and tests.

**Files:**
- Modify: `src/registry/tile.rs`
- Modify: `assets/world/tiles.registry.ron`

**Step 1: Update TileDef struct**

In `src/registry/tile.rs`, replace TileDef:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct TileDef {
    pub id: String,
    pub autotile: Option<String>,     // name of autotile atlas (None = invisible)
    pub solid: bool,
    pub hardness: f32,
    pub friction: f32,
    pub viscosity: f32,
    pub damage_on_contact: f32,       // DPS on touch
    #[serde(default)]
    pub effects: Vec<String>,         // contact effects
}
```

Remove `texture_index` method from `TileRegistry`. Keep `by_name`, `get`, `is_solid`.

Add new helper:

```rust
pub fn autotile_name(&self, id: TileId) -> Option<&str> {
    self.defs[id.0 as usize].autotile.as_deref()
}
```

**Step 2: Update tiles.registry.ron**

```ron
(
  tiles: [
    ( id: "air",   autotile: None,          solid: false, hardness: 0.0, friction: 0.0, viscosity: 0.0, damage_on_contact: 0.0, effects: [] ),
    ( id: "grass", autotile: Some("grass"),  solid: true,  hardness: 1.0, friction: 0.8, viscosity: 0.0, damage_on_contact: 0.0, effects: [] ),
    ( id: "dirt",  autotile: Some("dirt"),   solid: true,  hardness: 2.0, friction: 0.7, viscosity: 0.0, damage_on_contact: 0.0, effects: [] ),
    ( id: "stone", autotile: Some("stone"),  solid: true,  hardness: 5.0, friction: 0.6, viscosity: 0.0, damage_on_contact: 0.0, effects: [] ),
  ]
)
```

**Step 3: Update tests in tile.rs**

Update `test_registry()` to use new fields. Replace `texture_index` test with `autotile_name` test:

```rust
fn test_registry() -> TileRegistry {
    TileRegistry::from_defs(vec![
        TileDef {
            id: "air".into(), autotile: None, solid: false,
            hardness: 0.0, friction: 0.0, viscosity: 0.0,
            damage_on_contact: 0.0, effects: vec![],
        },
        TileDef {
            id: "grass".into(), autotile: Some("grass".into()), solid: true,
            hardness: 1.0, friction: 0.8, viscosity: 0.0,
            damage_on_contact: 0.0, effects: vec![],
        },
        // ... dirt, stone similarly
    ])
}

#[test]
fn autotile_name() {
    let reg = test_registry();
    assert_eq!(reg.autotile_name(TileId::AIR), None);
    assert_eq!(reg.autotile_name(TileId(1)), Some("grass"));
}
```

**Step 4: Verify**

Run: `cargo test`
Expected: All tests pass.

**Step 5: Commit**

```bash
git add -A && git commit -m "feat: update TileDef with autotile field, remove texture_index"
```

---

## Task 3: Autotile module — asset, registry, bitmask computation

Create the autotile system: RON asset loading, runtime registry, bitmask computation, variant selection.

**Files:**
- Create: `src/world/autotile.rs`
- Modify: `src/world/mod.rs` (add module)
- Modify: `src/registry/mod.rs` (register asset + loader, build AutotileRegistry)
- Modify: `src/registry/assets.rs` (add AutotileAsset)
- Rename: `assets/world/terrain/dirt.ron` → `assets/world/terrain/dirt.autotile.ron`

**Step 1: Define AutotileAsset in `src/registry/assets.rs`**

Add to existing file:

```rust
/// A single variant of a tile sprite for a given bitmask
#[derive(Debug, Clone, Deserialize)]
pub struct SpriteVariant {
    pub row: u32,
    pub weight: f32,
    // col and index fields from existing RON are ignored via serde
    #[serde(default)]
    pub col: u32,
    #[serde(default)]
    pub index: u32,
}

/// Mapping from a bitmask to its sprite variants
#[derive(Debug, Clone, Deserialize)]
pub struct BitmaskMapping {
    #[serde(default)]
    pub description: String,
    pub variants: Vec<SpriteVariant>,
}

/// Asset loaded from *.autotile.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct AutotileAsset {
    pub tile_size: u32,
    pub atlas_columns: u32,
    pub atlas_rows: u32,
    pub tiles: HashMap<u8, BitmaskMapping>,
}
```

Note: This matches the existing `dirt.ron` format exactly (with its `tiles` HashMap, `col`, `row`, `index` fields).

**Step 2: Register loader in `src/registry/mod.rs`**

Add to imports:

```rust
use assets::{AutotileAsset, ...};
```

In `RegistryPlugin::build`, add:

```rust
.init_asset::<AutotileAsset>()
.register_asset_loader(RonLoader::<AutotileAsset>::new(&["autotile.ron"]))
```

**Step 3: Rename dirt.ron → dirt.autotile.ron**

```bash
mv assets/world/terrain/dirt.ron assets/world/terrain/dirt.autotile.ron
```

**Step 4: Create `src/world/autotile.rs`**

```rust
use std::collections::HashMap;
use bevy::prelude::*;
use crate::registry::assets::{AutotileAsset, SpriteVariant};

/// CHUNK_SIZE constant
pub const CHUNK_SIZE: u32 = 32;
pub const CHUNK_TILE_COUNT: usize = (CHUNK_SIZE * CHUNK_SIZE) as usize;

/// Runtime entry for one tile type's autotile data
pub struct AutotileEntry {
    /// Column index in combined atlas (set during atlas build)
    pub column_index: u32,
    /// bitmask value → variants. Array of 256, most entries empty vec.
    pub bitmask_map: Vec<Vec<SpriteVariant>>,  // len = 256
}

impl AutotileEntry {
    pub fn from_asset(asset: &AutotileAsset, column_index: u32) -> Self {
        let mut bitmask_map: Vec<Vec<SpriteVariant>> = vec![Vec::new(); 256];
        for (&bitmask, mapping) in &asset.tiles {
            bitmask_map[bitmask as usize] = mapping.variants.clone();
        }
        Self { column_index, bitmask_map }
    }

    /// Get variants for a bitmask, falling back to bitmask 0 if not found
    pub fn variants_for(&self, bitmask: u8) -> &[SpriteVariant] {
        let v = &self.bitmask_map[bitmask as usize];
        if v.is_empty() {
            &self.bitmask_map[0]
        } else {
            v
        }
    }
}

/// Registry of all autotile entries, keyed by name
#[derive(Resource, Default)]
pub struct AutotileRegistry {
    pub entries: HashMap<String, AutotileEntry>,
}

// --- Bitmask computation ---

/// Neighbor bit layout:
///   NW=128  N=1   NE=2
///   W=64    .     E=4
///   SW=32   S=16  SE=8
pub fn compute_bitmask(is_solid_at: impl Fn(i32, i32) -> bool, x: i32, y: i32) -> u8 {
    let n  = is_solid_at(x,     y + 1);
    let ne = is_solid_at(x + 1, y + 1);
    let e  = is_solid_at(x + 1, y);
    let se = is_solid_at(x + 1, y - 1);
    let s  = is_solid_at(x,     y - 1);
    let sw = is_solid_at(x - 1, y - 1);
    let w  = is_solid_at(x - 1, y);
    let nw = is_solid_at(x - 1, y + 1);

    let mut mask = 0u8;
    if n  { mask |= 1; }
    // Corners only matter if both adjacent cardinals are solid
    if ne && n && e { mask |= 2; }
    if e  { mask |= 4; }
    if se && s && e { mask |= 8; }
    if s  { mask |= 16; }
    if sw && s && w { mask |= 32; }
    if w  { mask |= 64; }
    if nw && n && w { mask |= 128; }

    mask
}

// --- Variant selection ---

/// Simple position-based hash for deterministic variant selection
pub fn position_hash(x: i32, y: i32, seed: u32) -> f32 {
    // FNV-1a inspired hash
    let mut h: u32 = 2166136261;
    h ^= x as u32;
    h = h.wrapping_mul(16777619);
    h ^= y as u32;
    h = h.wrapping_mul(16777619);
    h ^= seed;
    h = h.wrapping_mul(16777619);
    // Normalize to 0.0-1.0
    (h as f32) / (u32::MAX as f32)
}

pub fn select_variant(variants: &[SpriteVariant], x: i32, y: i32, seed: u32) -> u32 {
    if variants.len() <= 1 {
        return variants.first().map(|v| v.row).unwrap_or(0);
    }
    let t = position_hash(x, y, seed);
    let total_weight: f32 = variants.iter().map(|v| v.weight).sum();
    let target = t * total_weight;
    let mut acc = 0.0;
    for v in variants {
        acc += v.weight;
        if target <= acc {
            return v.row;
        }
    }
    variants.last().unwrap().row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmask_isolated() {
        // All neighbors empty
        let mask = compute_bitmask(|_, _| false, 5, 5);
        assert_eq!(mask, 0);
    }

    #[test]
    fn bitmask_surrounded() {
        // All neighbors solid
        let mask = compute_bitmask(|_, _| true, 5, 5);
        assert_eq!(mask, 255);
    }

    #[test]
    fn bitmask_north_only() {
        let mask = compute_bitmask(|x, y| x == 5 && y == 6, 5, 5);
        assert_eq!(mask, 1); // N bit
    }

    #[test]
    fn bitmask_cardinal_nsew() {
        let mask = compute_bitmask(|x, y| {
            (x == 5 && y == 6) || // N
            (x == 5 && y == 4) || // S
            (x == 6 && y == 5) || // E
            (x == 4 && y == 5)    // W
        }, 5, 5);
        assert_eq!(mask, 1 | 4 | 16 | 64); // N+E+S+W = 85
    }

    #[test]
    fn bitmask_corner_ignored_without_cardinals() {
        // NE is solid but N and E are not — NE bit should NOT be set
        let mask = compute_bitmask(|x, y| x == 6 && y == 6, 5, 5);
        assert_eq!(mask, 0);
    }

    #[test]
    fn bitmask_corner_set_with_cardinals() {
        // NE, N, E all solid — NE bit SHOULD be set
        let mask = compute_bitmask(|x, y| {
            (x == 5 && y == 6) || // N
            (x == 6 && y == 5) || // E
            (x == 6 && y == 6)    // NE
        }, 5, 5);
        assert_eq!(mask, 1 | 2 | 4); // N + NE + E = 7
    }

    #[test]
    fn position_hash_deterministic() {
        let h1 = position_hash(10, 20, 42);
        let h2 = position_hash(10, 20, 42);
        assert_eq!(h1, h2);
    }

    #[test]
    fn position_hash_varies() {
        let h1 = position_hash(10, 20, 42);
        let h2 = position_hash(11, 20, 42);
        assert_ne!(h1, h2);
    }

    #[test]
    fn position_hash_range() {
        for x in 0..100 {
            let h = position_hash(x, 50, 42);
            assert!(h >= 0.0 && h <= 1.0, "hash out of range: {h}");
        }
    }

    #[test]
    fn select_single_variant() {
        let variants = vec![SpriteVariant { row: 5, weight: 1.0, col: 0, index: 0 }];
        assert_eq!(select_variant(&variants, 0, 0, 42), 5);
    }

    #[test]
    fn select_variant_deterministic() {
        let variants = vec![
            SpriteVariant { row: 0, weight: 0.5, col: 0, index: 0 },
            SpriteVariant { row: 1, weight: 0.5, col: 0, index: 0 },
        ];
        let r1 = select_variant(&variants, 10, 20, 42);
        let r2 = select_variant(&variants, 10, 20, 42);
        assert_eq!(r1, r2);
    }
}
```

**Step 5: Add module to `src/world/mod.rs`**

```rust
pub mod autotile;
```

**Step 6: Verify**

Run: `cargo test`
Expected: All tests pass, including new autotile tests.

**Step 7: Commit**

```bash
git add -A && git commit -m "feat: add autotile module with bitmask computation and variant selection"
```

---

## Task 4: Atlas builder

Build the combined horizontal atlas from per-type PNGs at load time.

**Files:**
- Create: `src/world/atlas.rs`
- Modify: `src/world/mod.rs` (add module)

**Step 1: Create `src/world/atlas.rs`**

```rust
use bevy::prelude::*;
use bevy::image::{Image, ImageSampler, ImageSamplerDescriptor, ImageFilterMode, ImageAddressMode};
use bevy::render::render_resource::{TextureFormat, Extent3d, TextureDimension};

/// Parameters of the combined atlas for UV computation
#[derive(Resource, Debug, Clone)]
pub struct AtlasParams {
    pub tile_size: u32,        // 16
    pub rows: u32,             // 47
    pub atlas_width: u32,      // N_types * tile_size
    pub atlas_height: u32,     // rows * tile_size = 752
}

/// Combined atlas texture handle
#[derive(Resource)]
pub struct TileAtlas {
    pub image: Handle<Image>,
    pub params: AtlasParams,
}

/// Build a combined horizontal atlas from individual per-type spritesheet images.
/// Each source image is a single column of 47 sprites (16×752px).
/// Returns the combined Image + column index mapping.
///
/// `sources` is an ordered list of (name, Image) pairs.
/// Returns (combined Image, HashMap<name, column_index>).
pub fn build_combined_atlas(
    sources: &[(&str, &Image)],
    tile_size: u32,
    rows: u32,
) -> (Image, std::collections::HashMap<String, u32>) {
    use std::collections::HashMap;

    let num_types = sources.len() as u32;
    let atlas_width = num_types * tile_size;
    let atlas_height = rows * tile_size;

    // Create RGBA8 image
    let mut data = vec![0u8; (atlas_width * atlas_height * 4) as usize];

    let mut column_map = HashMap::new();

    for (col_idx, (name, src_image)) in sources.iter().enumerate() {
        column_map.insert(name.to_string(), col_idx as u32);

        let src_data = &src_image.data;
        let src_width = src_image.width();
        let src_height = src_image.height();

        // Copy pixel by pixel from source into atlas column
        let copy_h = src_height.min(atlas_height);
        let copy_w = src_width.min(tile_size);

        for y in 0..copy_h {
            for x in 0..copy_w {
                let src_idx = ((y * src_width + x) * 4) as usize;
                let dst_x = col_idx as u32 * tile_size + x;
                let dst_idx = ((y * atlas_width + dst_x) * 4) as usize;

                if src_idx + 3 < src_data.len() && dst_idx + 3 < data.len() {
                    data[dst_idx]     = src_data[src_idx];
                    data[dst_idx + 1] = src_data[src_idx + 1];
                    data[dst_idx + 2] = src_data[src_idx + 2];
                    data[dst_idx + 3] = src_data[src_idx + 3];
                }
            }
        }
    }

    let mut image = Image::new(
        Extent3d {
            width: atlas_width,
            height: atlas_height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        default(),
    );

    // Pixel art: nearest filtering, clamp to edge
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Nearest,
        min_filter: ImageFilterMode::Nearest,
        mipmap_filter: ImageFilterMode::Nearest,
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        ..default()
    });

    (image, column_map)
}

/// Compute UV coordinates for a tile sprite in the combined atlas.
/// Returns (u_min, u_max, v_min, v_max) with half-pixel inset.
pub fn atlas_uv(
    column: u32,
    row: u32,
    params: &AtlasParams,
) -> (f32, f32, f32, f32) {
    let ts = params.tile_size as f32;
    let half = 0.5;

    let u_min = (column as f32 * ts + half) / params.atlas_width as f32;
    let u_max = (column as f32 * ts + ts - half) / params.atlas_width as f32;
    let v_min = (row as f32 * ts + half) / params.atlas_height as f32;
    let v_max = (row as f32 * ts + ts - half) / params.atlas_height as f32;

    (u_min, u_max, v_min, v_max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_uv_first_tile() {
        let params = AtlasParams {
            tile_size: 16,
            rows: 47,
            atlas_width: 48,   // 3 types
            atlas_height: 752, // 47 * 16
        };
        let (u_min, u_max, v_min, v_max) = atlas_uv(0, 0, &params);
        assert!(u_min > 0.0, "half-pixel inset");
        assert!(u_max < 16.0 / 48.0);
        assert!(v_min > 0.0);
        assert!(v_max < 16.0 / 752.0);
    }

    #[test]
    fn atlas_uv_second_column() {
        let params = AtlasParams {
            tile_size: 16,
            rows: 47,
            atlas_width: 48,
            atlas_height: 752,
        };
        let (u_min, _, _, _) = atlas_uv(1, 0, &params);
        let expected = (16.0 + 0.5) / 48.0;
        assert!((u_min - expected).abs() < 0.001);
    }

    #[test]
    fn atlas_uv_last_row() {
        let params = AtlasParams {
            tile_size: 16,
            rows: 47,
            atlas_width: 48,
            atlas_height: 752,
        };
        let (_, _, v_min, v_max) = atlas_uv(0, 46, &params);
        assert!(v_min > 46.0 * 16.0 / 752.0);
        assert!(v_max < 47.0 * 16.0 / 752.0);
    }
}
```

**Step 2: Add module to `src/world/mod.rs`**

```rust
pub mod atlas;
```

**Step 3: Verify**

Run: `cargo test`
Expected: All tests pass.

**Step 4: Commit**

```bash
git add -A && git commit -m "feat: add atlas builder with horizontal stitching and UV computation"
```

---

## Task 5: TileMaterial and shader

Create the custom Material2d and WGSL shader for tile rendering.

**Files:**
- Create: `src/world/tile_renderer.rs`
- Create: `assets/shaders/tile.wgsl`
- Modify: `src/world/mod.rs` (add module)

**Step 1: Create `assets/shaders/tile.wgsl`**

```wgsl
#import bevy_sprite::{
    mesh2d_vertex_output::VertexOutput,
}

@group(2) @binding(0) var atlas_texture: texture_2d<f32>;
@group(2) @binding(1) var atlas_sampler: sampler;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(atlas_texture, atlas_sampler, mesh.uv);
    if color.a < 0.01 {
        discard;
    }
    return color;
}
```

Note: Using Bevy 0.18 Material2d convention. Group 2 is the material bind group. Vertex transformation is handled by Bevy's built-in `Mesh2d` vertex shader via `mesh2d_vertex_output`.

**Step 2: Create `src/world/tile_renderer.rs`**

```rust
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::sprite::Material2d;

#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct TileMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub atlas: Handle<Image>,
}

impl Material2d for TileMaterial {
    fn fragment_shader() -> bevy::render::render_resource::ShaderRef {
        "shaders/tile.wgsl".into()
    }
}

/// Shared material handle for all chunk entities
#[derive(Resource)]
pub struct SharedTileMaterial {
    pub handle: Handle<TileMaterial>,
}
```

**Step 3: Add module to `src/world/mod.rs`**

```rust
pub mod tile_renderer;
```

**Step 4: Register Material2d plugin in main.rs**

Add to `src/main.rs`:

```rust
use bevy::sprite::Material2dPlugin;
use world::tile_renderer::TileMaterial;

// In main(), add:
.add_plugins(Material2dPlugin::<TileMaterial>::default())
```

**Step 5: Verify**

Run: `cargo build`
Expected: Compiles. Shader will be validated at runtime when first used.

**Step 6: Commit**

```bash
git add -A && git commit -m "feat: add TileMaterial and tile.wgsl shader"
```

---

## Task 6: Mesh builder and chunk rendering

Build chunk meshes from tile data, wire into chunk spawn/despawn lifecycle.

**Files:**
- Create: `src/world/mesh_builder.rs`
- Modify: `src/world/chunk.rs` (add bitmasks/damage to ChunkData, rewrite spawn/despawn)
- Modify: `src/world/mod.rs` (add module, update plugin)
- Modify: `src/registry/mod.rs` (load autotile assets, build atlas + registries)

**Step 1: Create `src/world/mesh_builder.rs`**

```rust
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology, Mesh, MeshVertexAttribute, VertexAttributeValues};

use super::atlas::{AtlasParams, atlas_uv};
use super::autotile::{AutotileRegistry, select_variant, CHUNK_SIZE, CHUNK_TILE_COUNT};
use crate::registry::tile::{TileId, TileRegistry};

/// Reusable buffers for mesh building (avoid allocations)
#[derive(Resource)]
pub struct MeshBuildBuffers {
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl Default for MeshBuildBuffers {
    fn default() -> Self {
        Self {
            positions: Vec::with_capacity(CHUNK_TILE_COUNT * 4),
            uvs: Vec::with_capacity(CHUNK_TILE_COUNT * 4),
            indices: Vec::with_capacity(CHUNK_TILE_COUNT * 6),
        }
    }
}

/// Build a Mesh for one chunk from its tile data, bitmasks, and registries.
pub fn build_chunk_mesh(
    tiles: &[TileId],
    bitmasks: &[u8],
    display_chunk_x: i32,
    chunk_y: i32,
    tile_size: f32,
    seed: u32,
    tile_registry: &TileRegistry,
    autotile_registry: &AutotileRegistry,
    atlas_params: &AtlasParams,
    buffers: &mut MeshBuildBuffers,
) -> Mesh {
    buffers.positions.clear();
    buffers.uvs.clear();
    buffers.indices.clear();

    let base_x = display_chunk_x * CHUNK_SIZE as i32;
    let base_y = chunk_y * CHUNK_SIZE as i32;

    for local_y in 0..CHUNK_SIZE {
        for local_x in 0..CHUNK_SIZE {
            let idx = (local_y * CHUNK_SIZE + local_x) as usize;
            let tile_id = tiles[idx];

            if tile_id == TileId::AIR {
                continue;
            }

            let autotile_name = match tile_registry.autotile_name(tile_id) {
                Some(name) => name,
                None => continue,
            };

            let entry = match autotile_registry.entries.get(autotile_name) {
                Some(e) => e,
                None => continue, // autotile not loaded yet
            };

            let bitmask = bitmasks[idx];
            let variants = entry.variants_for(bitmask);

            let world_x = base_x + local_x as i32;
            let world_y = base_y + local_y as i32;
            let sprite_row = select_variant(variants, world_x, world_y, seed);

            // Quad position in world space
            let px = world_x as f32 * tile_size;
            let py = world_y as f32 * tile_size;

            // UV in atlas
            let (u_min, u_max, v_min, v_max) = atlas_uv(
                entry.column_index, sprite_row, atlas_params,
            );

            // 4 vertices, 6 indices
            let vi = buffers.positions.len() as u32;

            buffers.positions.extend_from_slice(&[
                [px,             py,             0.0], // BL
                [px + tile_size, py,             0.0], // BR
                [px + tile_size, py + tile_size, 0.0], // TR
                [px,             py + tile_size, 0.0], // TL
            ]);

            buffers.uvs.extend_from_slice(&[
                [u_min, v_max], // BL
                [u_max, v_max], // BR
                [u_max, v_min], // TR
                [u_min, v_min], // TL
            ]);

            buffers.indices.extend_from_slice(&[
                vi, vi + 1, vi + 2,
                vi, vi + 2, vi + 3,
            ]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        bevy::render::render_asset::RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, buffers.positions.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, buffers.uvs.clone());
    mesh.insert_indices(Indices::U32(buffers.indices.clone()));

    mesh
}
```

**Step 2: Add bitmasks and damage to ChunkData in `src/world/chunk.rs`**

```rust
pub struct ChunkData {
    pub tiles: Vec<TileId>,
    pub bitmasks: Vec<u8>,
    pub damage: Vec<u8>,
}
```

Update `get_or_generate_chunk` to initialize bitmasks and damage:

```rust
.or_insert_with(|| {
    let tiles = terrain_gen::generate_chunk_tiles(wc.seed, chunk_x, chunk_y, wc, tt);
    let len = tiles.len();
    ChunkData {
        tiles,
        bitmasks: vec![0; len],
        damage: vec![0; len],
    }
})
```

**Step 3: Add `ChunkDirty` marker and bitmask initialization**

In `src/world/chunk.rs`:

```rust
/// Marker: chunk mesh needs rebuild
#[derive(Component)]
pub struct ChunkDirty;
```

Add function to compute bitmasks for an entire chunk:

```rust
use crate::world::autotile::{compute_bitmask, CHUNK_SIZE};

pub fn init_chunk_bitmasks(
    world_map: &mut WorldMap,
    chunk_x: i32,
    chunk_y: i32,
    wc: &WorldConfig,
    tt: &TerrainTiles,
    registry: &TileRegistry,
) -> Vec<u8> {
    let mut bitmasks = vec![0u8; (CHUNK_SIZE * CHUNK_SIZE) as usize];
    let base_x = chunk_x * CHUNK_SIZE as i32;
    let base_y = chunk_y * CHUNK_SIZE as i32;

    for local_y in 0..CHUNK_SIZE {
        for local_x in 0..CHUNK_SIZE {
            let world_x = base_x + local_x as i32;
            let world_y = base_y + local_y as i32;
            let idx = (local_y * CHUNK_SIZE + local_x) as usize;
            bitmasks[idx] = compute_bitmask(
                |x, y| {
                    let tile = world_map.get_tile(x, y, wc, tt);
                    registry.is_solid(tile)
                },
                world_x,
                world_y,
            );
        }
    }
    bitmasks
}
```

**Step 4: Rewrite `spawn_chunk`**

```rust
use crate::world::mesh_builder::{build_chunk_mesh, MeshBuildBuffers};
use crate::world::atlas::TileAtlas;
use crate::world::autotile::AutotileRegistry;
use crate::world::tile_renderer::SharedTileMaterial;

pub fn spawn_chunk(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    world_map: &mut WorldMap,
    loaded_chunks: &mut LoadedChunks,
    wc: &WorldConfig,
    tt: &TerrainTiles,
    registry: &TileRegistry,
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

    let data_chunk_x = wc.wrap_chunk_x(display_chunk_x);

    // Generate chunk data if needed
    world_map.get_or_generate_chunk(data_chunk_x, chunk_y, wc, tt);

    // Compute bitmasks (needs mutable world_map for neighbor lookups)
    let bitmasks = init_chunk_bitmasks(world_map, data_chunk_x, chunk_y, wc, tt, registry);

    // Store bitmasks in chunk data
    if let Some(chunk) = world_map.chunks.get_mut(&(data_chunk_x, chunk_y)) {
        chunk.bitmasks = bitmasks;
    }

    // Build mesh (re-borrow chunk data immutably)
    let chunk_data = &world_map.chunks[&(data_chunk_x, chunk_y)];
    let mesh = build_chunk_mesh(
        &chunk_data.tiles,
        &chunk_data.bitmasks,
        display_chunk_x,
        chunk_y,
        wc.tile_size,
        wc.seed,
        registry,
        autotile_registry,
        &atlas.params,
        buffers,
    );

    let mesh_handle = meshes.add(mesh);

    let entity = commands.spawn((
        ChunkCoord { x: display_chunk_x, y: chunk_y },
        Mesh2d(mesh_handle),
        MeshMaterial2d(material.handle.clone()),
        Transform::from_translation(Vec3::ZERO),
        Visibility::default(),
    )).id();

    loaded_chunks.map.insert((display_chunk_x, chunk_y), entity);
}
```

**Step 5: Update `chunk_loading_system`**

Update signature to include new resources, remove old `TilemapTextureHandle`:

```rust
pub fn chunk_loading_system(
    mut commands: Commands,
    camera_query: Query<&Transform, With<Camera2d>>,
    mut world_map: ResMut<WorldMap>,
    mut loaded_chunks: ResMut<LoadedChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    registry: Res<TileRegistry>,
    autotile_registry: Res<AutotileRegistry>,
    atlas: Res<TileAtlas>,
    material: Res<SharedTileMaterial>,
    mut buffers: ResMut<MeshBuildBuffers>,
    wc: Res<WorldConfig>,
    tt: Res<TerrainTiles>,
) {
    // ... keep camera/desired chunk calculation identical ...

    for &(display_cx, cy) in &desired {
        if !loaded_chunks.map.contains_key(&(display_cx, cy)) {
            spawn_chunk(
                &mut commands, &mut meshes, &mut world_map, &mut loaded_chunks,
                &wc, &tt, &registry, &autotile_registry, &atlas, &material,
                &mut buffers, display_cx, cy,
            );
        }
    }

    // despawn unchanged
    // ...
}
```

**Step 6: Add `rebuild_dirty_chunks` system**

In `src/world/chunk.rs`:

```rust
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
            coord.x,
            coord.y,
            wc.tile_size,
            wc.seed,
            &registry,
            &autotile_registry,
            &atlas.params,
            &mut buffers,
        );

        let mesh_handle = meshes.add(mesh);
        commands.entity(entity)
            .insert(Mesh2d(mesh_handle))
            .remove::<ChunkDirty>();
    }
}
```

**Step 7: Add module to `src/world/mod.rs`**

```rust
pub mod mesh_builder;
```

**Step 8: Verify**

Run: `cargo build`
Expected: Compiles. Will not render correctly yet until autotile assets are loaded (Task 8).

**Step 9: Commit**

```bash
git add -A && git commit -m "feat: mesh builder and chunk spawn with custom tile renderer"
```

---

## Task 7: Asset loading pipeline — autotile + atlas construction

Wire autotile asset loading and atlas construction into the registry loading pipeline.

**Files:**
- Modify: `src/registry/mod.rs` (load autotile assets, build atlas, create resources)
- Create placeholder: `assets/world/terrain/stone.autotile.ron` (copy from dirt)
- Create placeholder: `assets/world/terrain/grass.autotile.ron` (copy from dirt)

**Step 1: Create placeholder autotile RON files**

Copy `dirt.autotile.ron` to `stone.autotile.ron` and `grass.autotile.ron`. These will use the dirt spritesheet until real assets are created.

**Step 2: Create placeholder PNG files**

Copy `assets/world/terrain/dirt.png` to `stone.png` and `grass.png` (same visual, will be replaced later with real art).

**Step 3: Extend loading pipeline in `src/registry/mod.rs`**

This is the most complex step. We need to:
1. After TileRegistry is built, read which autotile names are needed
2. Load each `{name}.png` and `{name}.autotile.ron`
3. Build combined atlas
4. Build AutotileRegistry
5. Create TileMaterial
6. Insert all resources

Add a new state or system that runs after `check_loading` transitions to InGame. The cleanest approach is an `OnEnter(AppState::InGame)` system:

```rust
// In RegistryPlugin::build, add:
.add_systems(OnEnter(AppState::InGame), build_autotile_resources)
```

```rust
fn build_autotile_resources(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut tile_materials: ResMut<Assets<TileMaterial>>,
    autotile_assets: Res<Assets<AutotileAsset>>,
    registry: Res<TileRegistry>,
) {
    use crate::world::atlas::{build_combined_atlas, AtlasParams, TileAtlas};
    use crate::world::autotile::{AutotileRegistry, AutotileEntry};
    use crate::world::tile_renderer::{TileMaterial, SharedTileMaterial};
    use crate::world::mesh_builder::MeshBuildBuffers;

    // Collect unique autotile names from registry
    let mut autotile_names: Vec<String> = Vec::new();
    for def in &registry.defs {
        if let Some(ref name) = def.autotile {
            if !autotile_names.contains(name) {
                autotile_names.push(name.clone());
            }
        }
    }

    // Load source images and autotile RON assets synchronously
    // Note: For MVP, we load these blocking. Future: async pipeline.
    let tile_size = 16u32;
    let rows = 47u32;
    let mut source_images: Vec<(String, Image)> = Vec::new();
    let mut autotile_data: std::collections::HashMap<String, AutotileAsset> = std::collections::HashMap::new();

    for name in &autotile_names {
        // Load PNG
        let png_path = format!("world/terrain/{name}.png");
        let img_handle: Handle<Image> = asset_server.load(&png_path);
        // We need the image data now — it should already be loaded or we wait
        // For MVP: use blocking load via include_bytes or load-and-wait pattern
        // Better: load in Loading state and wait
        // TODO: This needs to be refactored to async loading

        // Load autotile RON
        let ron_path = format!("world/terrain/{name}.autotile.ron");
        let _ron_handle: Handle<AutotileAsset> = asset_server.load(&ron_path);
    }

    // ... (see Step 4 for the full async approach)
}
```

**Note:** The synchronous loading approach above is problematic with Bevy's async asset system. A better approach is to load autotile assets during the `Loading` state and wait for them.

**Step 4: Proper async approach — extend LoadingAssets**

In `src/registry/mod.rs`, extend the loading pipeline:

```rust
#[derive(Resource)]
struct LoadingAssets {
    tiles: Handle<TileRegistryAsset>,
    player: Handle<PlayerDefAsset>,
    world_config: Handle<WorldConfigAsset>,
    parallax: Handle<ParallaxConfigAsset>,
    // NEW: autotile assets loaded after we know tile names
    autotile_rons: Vec<(String, Handle<AutotileAsset>)>,
    autotile_images: Vec<(String, Handle<Image>)>,
}
```

Split loading into two phases:
1. `start_loading` — load tiles.registry.ron (and other base assets)
2. `check_base_loading` — when base assets ready, read autotile names, start loading per-type assets
3. `check_autotile_loading` — when all autotile assets ready, build atlas + registry, transition to InGame

Add a new intermediate state:

```rust
#[derive(States, Default, Debug, Clone, Eq, PartialEq, Hash)]
pub enum AppState {
    #[default]
    Loading,
    LoadingAutotile,   // NEW
    InGame,
}
```

`check_loading` transitions to `LoadingAutotile` instead of `InGame`.
New system `start_autotile_loading` runs on `OnEnter(LoadingAutotile)`.
New system `check_autotile_loading` runs in `Update` during `LoadingAutotile`.

When all autotile assets are loaded:
- Build combined atlas image
- Build AutotileRegistry  
- Create SharedTileMaterial
- Insert MeshBuildBuffers
- Transition to InGame

**Full implementation for these systems should be written in the implementation — this plan provides the architecture. Key code patterns:**

```rust
fn start_autotile_loading(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    registry: Res<TileRegistry>,
) {
    let mut rons = Vec::new();
    let mut imgs = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for def in &registry.defs {
        if let Some(ref name) = def.autotile {
            if seen.insert(name.clone()) {
                let ron_handle = asset_server.load::<AutotileAsset>(
                    format!("world/terrain/{name}.autotile.ron")
                );
                let img_handle = asset_server.load::<Image>(
                    format!("world/terrain/{name}.png")
                );
                rons.push((name.clone(), ron_handle));
                imgs.push((name.clone(), img_handle));
            }
        }
    }

    commands.insert_resource(LoadingAutotileAssets { rons, imgs });
}

fn check_autotile_loading(
    mut commands: Commands,
    loading: Res<LoadingAutotileAssets>,
    autotile_assets: Res<Assets<AutotileAsset>>,
    images: Res<Assets<Image>>,
    mut image_assets: ResMut<Assets<Image>>,
    mut tile_materials: ResMut<Assets<TileMaterial>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    // Check all loaded
    let all_rons = loading.rons.iter().all(|(_, h)| autotile_assets.contains(h));
    let all_imgs = loading.imgs.iter().all(|(_, h)| images.contains(h));
    if !all_rons || !all_imgs { return; }

    // Build combined atlas
    let sources: Vec<(&str, &Image)> = loading.imgs.iter()
        .map(|(name, handle)| (name.as_str(), images.get(handle).unwrap()))
        .collect();

    let (atlas_image, column_map) = build_combined_atlas(&sources, 16, 47);
    let params = AtlasParams { tile_size: 16, rows: 47,
        atlas_width: sources.len() as u32 * 16,
        atlas_height: 47 * 16,
    };
    let atlas_handle = image_assets.add(atlas_image);

    // Build AutotileRegistry
    let mut autotile_reg = AutotileRegistry::default();
    for (name, handle) in &loading.rons {
        let asset = autotile_assets.get(handle).unwrap();
        let col_idx = column_map[name];
        autotile_reg.entries.insert(name.clone(), AutotileEntry::from_asset(asset, col_idx));
    }

    // Create material
    let material = tile_materials.add(TileMaterial { atlas: atlas_handle.clone() });

    // Insert resources
    commands.insert_resource(TileAtlas { image: atlas_handle, params });
    commands.insert_resource(autotile_reg);
    commands.insert_resource(SharedTileMaterial { handle: material });
    commands.insert_resource(MeshBuildBuffers::default());

    commands.remove_resource::<LoadingAutotileAssets>();
    next_state.set(AppState::InGame);
    info!("Autotile atlas built, entering InGame");
}
```

**Step 5: Verify**

Run: `cargo build && cargo run`
Expected: Game starts, tiles render with autotile sprites from dirt atlas (all types look like dirt for now — placeholder assets).

**Step 6: Commit**

```bash
git add -A && git commit -m "feat: autotile asset loading pipeline with combined atlas construction"
```

---

## Task 8: Block interaction with bitmask updates and dirty flags

Rewrite block_action.rs to use the new autotile system.

**Files:**
- Modify: `src/interaction/block_action.rs`
- Modify: `src/world/chunk.rs` (add `update_bitmasks_around` function)
- Modify: `src/world/mod.rs` (add `rebuild_dirty_chunks` to systems)

**Step 1: Add `update_bitmasks_around` to chunk.rs**

```rust
use std::collections::HashSet;

/// Recompute bitmasks for the 3×3 area around (center_x, center_y).
/// Returns set of affected chunk coords that need mesh rebuild.
pub fn update_bitmasks_around(
    world_map: &mut WorldMap,
    center_x: i32,
    center_y: i32,
    wc: &WorldConfig,
    tt: &TerrainTiles,
    registry: &TileRegistry,
) -> HashSet<(i32, i32)> {
    let mut dirty_chunks = HashSet::new();

    for dy in -1..=1 {
        for dx in -1..=1 {
            let x = center_x + dx;
            let y = center_y + dy;

            if y < 0 || y >= wc.height_tiles { continue; }

            let wrapped_x = wc.wrap_tile_x(x);
            let (cx, cy) = tile_to_chunk(wrapped_x, y, wc.chunk_size);
            let (lx, ly) = tile_to_local(wrapped_x, y, wc.chunk_size);
            let idx = (ly * wc.chunk_size + lx) as usize;

            let new_mask = compute_bitmask(
                |bx, by| {
                    let tile = world_map.get_tile(bx, by, wc, tt);
                    registry.is_solid(tile)
                },
                wrapped_x, y,
            );

            if let Some(chunk) = world_map.chunks.get_mut(&(cx, cy)) {
                chunk.bitmasks[idx] = new_mask;
                dirty_chunks.insert((cx, cy));
            }
        }
    }

    dirty_chunks
}
```

**Step 2: Update block_interaction_system**

```rust
use crate::world::chunk::{
    tile_to_local, world_to_tile, update_bitmasks_around,
    ChunkCoord, ChunkDirty, LoadedChunks, WorldMap,
};

pub fn block_interaction_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    player_query: Query<&Transform, With<Player>>,
    player_config: Res<PlayerConfig>,
    world_config: Res<WorldConfig>,
    terrain_tiles: Res<TerrainTiles>,
    tile_registry: Res<TileRegistry>,
    mut world_map: ResMut<WorldMap>,
    loaded_chunks: Res<LoadedChunks>,
) {
    // ... keep input/range checking identical ...

    if left_click {
        let current = world_map.get_tile(tile_x, tile_y, &world_config, &terrain_tiles);
        if !tile_registry.is_solid(current) { return; }
        world_map.set_tile(tile_x, tile_y, TileId::AIR, &world_config, &terrain_tiles);
    } else if right_click {
        // ... keep overlap check identical ...
        let place_id = tile_registry.by_name("dirt");
        world_map.set_tile(tile_x, tile_y, place_id, &world_config, &terrain_tiles);
    } else {
        return;
    }

    // Update bitmasks and mark dirty chunks
    let dirty = update_bitmasks_around(
        &mut world_map, tile_x, tile_y,
        &world_config, &terrain_tiles, &tile_registry,
    );

    for (cx, cy) in dirty {
        // Find ALL loaded chunk entities that map to this data chunk
        for (&(display_cx, display_cy), &entity) in &loaded_chunks.map {
            if world_config.wrap_chunk_x(display_cx) == cx && display_cy == cy {
                commands.entity(entity).insert(ChunkDirty);
            }
        }
    }
}
```

**Step 3: Wire systems in WorldPlugin**

In `src/world/mod.rs`:

```rust
impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldMap>()
            .init_resource::<LoadedChunks>()
            .add_systems(
                Update,
                (
                    chunk::chunk_loading_system,
                    chunk::rebuild_dirty_chunks,
                ).chain().run_if(in_state(AppState::InGame)),
            );
    }
}
```

Note: `block_interaction_system` is registered in `InteractionPlugin` and runs before these due to system ordering. Use `.chain()` or explicit ordering if needed:

```rust
// In InteractionPlugin, ensure block_action runs before chunk systems
app.add_systems(
    Update,
    block_action::block_interaction_system
        .before(chunk::rebuild_dirty_chunks)
        .run_if(in_state(AppState::InGame)),
);
```

**Step 4: Verify**

Run: `cargo run`
Expected: Break/place blocks → tiles update visually with correct autotile transitions.

**Step 5: Commit**

```bash
git add -A && git commit -m "feat: block interaction with bitmask updates and dirty chunk rebuild"
```

---

## Task 9: Final verification and cleanup

**Files:**
- Possibly modify: various files for compilation fixes
- Remove: dead code, unused imports

**Step 1: Full test suite**

Run: `cargo test`
Expected: All tests pass.

**Step 2: Clippy**

Run: `cargo clippy`
Expected: No warnings (or only pre-existing ones).

**Step 3: Run the game and verify visually**

Run: `cargo run`

Verify:
- [ ] Tiles render with autotile sprites (correct edges/corners)
- [ ] Breaking a block updates surrounding tiles visually
- [ ] Placing a block updates surrounding tiles visually
- [ ] Moving around loads/unloads chunks smoothly
- [ ] No texture bleeding between tiles
- [ ] Camera zoom works (pixel-perfect rendering)

**Step 4: Cleanup dead code**

Remove any `#[allow(dead_code)]`, unused imports, TODO comments that are resolved.

**Step 5: Final commit**

```bash
git add -A && git commit -m "feat: complete autotile custom renderer integration"
```

---

## Summary of all files changed

### Created
- `src/world/autotile.rs` — AutotileRegistry, bitmask computation, variant selection
- `src/world/atlas.rs` — AtlasBuilder, UV computation, TileAtlas resource
- `src/world/tile_renderer.rs` — TileMaterial, SharedTileMaterial
- `src/world/mesh_builder.rs` — build_chunk_mesh, MeshBuildBuffers
- `assets/shaders/tile.wgsl` — trivial fragment shader
- `assets/world/terrain/dirt.autotile.ron` — renamed from dirt.ron
- `assets/world/terrain/stone.autotile.ron` — placeholder (copy of dirt)
- `assets/world/terrain/grass.autotile.ron` — placeholder (copy of dirt)
- `assets/world/terrain/stone.png` — placeholder (copy of dirt.png)
- `assets/world/terrain/grass.png` — placeholder (copy of dirt.png)

### Modified
- `Cargo.toml` — removed bevy_ecs_tilemap
- `src/main.rs` — removed TilemapPlugin, added Material2dPlugin
- `src/registry/tile.rs` — TileDef: removed texture_index, added autotile/effects
- `src/registry/assets.rs` — added AutotileAsset, SpriteVariant, BitmaskMapping
- `src/registry/mod.rs` — added LoadingAutotile state, autotile loading pipeline
- `src/world/mod.rs` — added modules, updated plugin with new systems
- `src/world/chunk.rs` — ChunkData bitmasks/damage, spawn/despawn rewrite, dirty system
- `src/interaction/block_action.rs` — removed bevy_ecs_tilemap, added bitmask updates
- `assets/world/tiles.registry.ron` — updated TileDef format

### Deleted
- `assets/world/terrain/dirt.ron` — renamed to .autotile.ron

### Removed dependency
- `bevy_ecs_tilemap` from Cargo.toml
