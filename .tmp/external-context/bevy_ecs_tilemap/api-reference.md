---
source: Context7 API + docs.rs source
library: bevy_ecs_tilemap
package: bevy_ecs_tilemap
version: 0.18.1 (compatible with Bevy 0.18.x)
topic: Complete API reference - all structs, enums, bundles
fetched: 2026-02-25T12:00:00Z
official_docs: https://docs.rs/bevy_ecs_tilemap/0.18.1/bevy_ecs_tilemap/
---

# bevy_ecs_tilemap 0.18.1 - Complete API Reference

> **Crate version**: `bevy_ecs_tilemap = "0.18.1"` (depends on `bevy ^0.18.0`)

## Plugin Registration

```rust
use bevy::prelude::*;
use bevy_ecs_tilemap::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(TilemapPlugin)
        .run();
}
```

`TilemapPlugin` registers:
- `TilemapRenderingPlugin` (if `render` feature enabled)
- System: `update_changed_tile_positions` in `First` schedule
- Array texture preloading (if `atlas` feature NOT enabled)
- Reflection for all tilemap/tile components

---

## Map Components (from `bevy_ecs_tilemap::map`)

### `TilemapSize`
Size of the tilemap in tiles.
```rust
#[derive(Component, Reflect, Default, Clone, Copy, Debug, Hash, PartialEq)]
pub struct TilemapSize {
    pub x: u32,
    pub y: u32,
}

impl TilemapSize {
    pub const fn new(x: u32, y: u32) -> Self;
    pub const fn count(&self) -> usize; // x * y
}
// Conversions: From<UVec2>, Into<UVec2>, Into<Vec2>
```

### `TilemapTileSize`
Size of the tiles in pixels.
```rust
#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialOrd, PartialEq)]
pub struct TilemapTileSize {
    pub x: f32,
    pub y: f32,
}

impl TilemapTileSize {
    pub const fn new(x: f32, y: f32) -> Self;
}
// Conversions: From<Vec2>, Into<Vec2>, Into<TilemapGridSize>, Into<TilemapTextureSize>
```

### `TilemapGridSize`
Size of the tiles on the grid in pixels. Can differ from TilemapTileSize to create overlapping tiles.
```rust
#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialOrd, PartialEq)]
pub struct TilemapGridSize {
    pub x: f32,
    pub y: f32,
}

impl TilemapGridSize {
    pub const fn new(x: f32, y: f32) -> Self;
}
// Conversions: From<Vec2>, Into<Vec2>
// NOTE: TilemapTileSize auto-converts Into<TilemapGridSize>
```

### `TilemapSpacing`
Spacing between tiles in pixels inside of the texture atlas. Defaults to 0.0.
```rust
#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq)]
pub struct TilemapSpacing {
    pub x: f32,
    pub y: f32,
}

impl TilemapSpacing {
    pub const fn new(x: f32, y: f32) -> Self;
    pub const fn zero() -> Self;
}
```

### `TilemapTextureSize`
Size of the atlas texture in pixels.
```rust
#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq)]
pub struct TilemapTextureSize {
    pub x: f32,
    pub y: f32,
}

impl TilemapTextureSize {
    pub const fn new(x: f32, y: f32) -> Self;
}
```

### `TilemapType`
```rust
#[derive(Component, Reflect, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TilemapType {
    #[default]
    Square,
    Hexagon(HexCoordSystem),
    Isometric(IsoCoordSystem),
}

pub enum HexCoordSystem {
    RowEven, RowOdd, ColumnEven, ColumnOdd, Row, Column,
}

pub enum IsoCoordSystem {
    Diamond, Staggered,
}
```

### `TilemapTexture`
```rust
#[derive(Component, Reflect, Clone, Debug, Hash, PartialEq, Eq)]
pub enum TilemapTexture {
    /// All tile textures in a single sprite sheet image
    Single(Handle<Image>),
    
    /// Each tile texture is a separate image (requires same size, NOT available with "atlas" feature)
    #[cfg(not(feature = "atlas"))]
    Vector(Vec<Handle<Image>>),
    
    /// Tiles as array layers in KTX2/DDS container (NOT available with "atlas" feature)
    #[cfg(not(feature = "atlas"))]
    TextureContainer(Handle<Image>),
}

// Default: TilemapTexture::Single(Default::default())

impl TilemapTexture {
    pub fn image_handles(&self) -> Vec<&Handle<Image>>;
    pub fn verify_ready(&self, images: &Res<Assets<Image>>) -> bool;
    pub fn set_images_to_copy_src(&self, images: &mut ResMut<Assets<Image>>);
}
```

### `TilemapId`
Reference to the tilemap entity that a tile belongs to.
```rust
#[derive(Component, Reflect, Clone, Copy, Debug, Hash, Deref, DerefMut, PartialEq, Eq)]
pub struct TilemapId(pub Entity);
// Default: Entity::from_raw_u32(0)
```

### `TilemapRenderSettings`
```rust
#[derive(Component, Debug, Copy, Clone)]
#[require(VisibilityClass)]
pub struct TilemapRenderSettings {
    /// Chunk size in tiles for render meshes.
    /// Larger = better for static tilemaps.
    /// Smaller = better for frequently changing tilemaps.
    pub render_chunk_size: UVec2,
    
    /// If true, uses chunk z and y for sorting (for isometric).
    pub y_sort: bool,
}

// Default: render_chunk_size = CHUNK_SIZE_2D (64x64), y_sort = false
pub const CHUNK_SIZE_2D: UVec2 = UVec2::from_array([64, 64]);
```

### `FrustumCulling`
```rust
#[derive(Component, Reflect, Debug, Clone, Copy, Deref)]
pub struct FrustumCulling(pub bool);
// Default: true
```

### `TilemapAnchor`
```rust
// From anchor module - allows anchoring tilemap like a sprite
// Available via: use bevy_ecs_tilemap::prelude::TilemapAnchor;
```

---

## Tile Components (from `bevy_ecs_tilemap::tiles`)

### `TilePos`
```rust
#[derive(Component, Reflect, Default, Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct TilePos {
    pub x: u32,
    pub y: u32,
}

impl TilePos {
    pub const fn new(x: u32, y: u32) -> Self;
    pub fn to_index(&self, tilemap_size: &TilemapSize) -> usize; // (y * size.x) + x
    pub fn within_map_bounds(&self, map_size: &TilemapSize) -> bool;
}
// Conversions: From<UVec2>, Into<UVec2>, Into<Vec2>
```

### `TileTextureIndex`
```rust
#[derive(Component, Reflect, Default, Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct TileTextureIndex(pub u32);
// Index into atlas (horizontal-based) or texture array
```

### `TileColor`
```rust
#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
pub struct TileColor(pub Color);
// Conversions: From<Color>
```

### `TileVisible`
```rust
#[derive(Component, Reflect, Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct TileVisible(pub bool);
// Default: true
```

### `TileFlip`
```rust
#[derive(Component, Reflect, Default, Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct TileFlip {
    pub x: bool,  // Flip along X axis
    pub y: bool,  // Flip along Y axis
    pub d: bool,  // Anti-diagonal flip
}
```

### `TilePosOld`
```rust
#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
pub struct TilePosOld(pub TilePos);
// Automatically updated by update_changed_tile_positions system
```

### `AnimatedTile`
```rust
#[derive(Component, Reflect, Clone, Copy, Debug)]
pub struct AnimatedTile {
    pub start: u32,  // Start frame index (inclusive)
    pub end: u32,    // End frame index (exclusive)
    pub speed: f32,  // Animation playback speed
}
```

---

## TileStorage

```rust
#[derive(Component, Reflect, Default, Debug, Clone)]
pub struct TileStorage {
    tiles: Vec<Option<Entity>>,  // private
    pub size: TilemapSize,
}

impl TileStorage {
    /// Creates empty storage for given size
    pub fn empty(size: TilemapSize) -> Self;
    
    /// Get tile entity at position. Panics if out of bounds.
    pub fn get(&self, tile_pos: &TilePos) -> Option<Entity>;
    
    /// Get tile entity, returns None if out of bounds
    pub fn checked_get(&self, tile_pos: &TilePos) -> Option<Entity>;
    
    /// Set tile entity at position. Panics if out of bounds.
    pub fn set(&mut self, tile_pos: &TilePos, tile_entity: Entity);
    
    /// Set tile entity, no-op if out of bounds
    pub fn checked_set(&mut self, tile_pos: &TilePos, tile_entity: Entity);
    
    /// Remove tile at position, returns the entity. Panics if out of bounds.
    pub fn remove(&mut self, tile_pos: &TilePos) -> Option<Entity>;
    
    /// Remove tile, returns None if out of bounds
    pub fn checked_remove(&mut self, tile_pos: &TilePos) -> Option<Entity>;
    
    /// Remove all tiles, returning iterator of removed entities
    pub fn drain(&mut self) -> impl Iterator<Item = Entity>;
    
    /// Iterator over all positions
    pub fn iter(&self) -> impl Iterator<Item = &Option<Entity>>;
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Option<Entity>>;
}
```

---

## TileBundle

```rust
#[derive(Bundle, Default, Clone, Copy, Debug)]
pub struct TileBundle {
    pub position: TilePos,
    pub texture_index: TileTextureIndex,
    pub tilemap_id: TilemapId,
    pub visible: TileVisible,
    pub flip: TileFlip,
    pub color: TileColor,
    pub old_position: TilePosOld,
    pub sync: SyncToRenderWorld,
}
```

---

## TilemapBundle (with `render` feature)

```rust
// Type alias:
pub type TilemapBundle = MaterialTilemapBundle<StandardTilemapMaterial>;

#[derive(Bundle, Debug, Default, Clone)]
pub struct MaterialTilemapBundle<M: MaterialTilemap> {
    pub grid_size: TilemapGridSize,
    pub map_type: TilemapType,
    pub size: TilemapSize,
    pub spacing: TilemapSpacing,
    pub storage: TileStorage,
    pub texture: TilemapTexture,
    pub tile_size: TilemapTileSize,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub render_settings: TilemapRenderSettings,
    pub visibility: Visibility,
    pub inherited_visibility: InheritedVisibility,
    pub view_visibility: ViewVisibility,
    pub frustum_culling: FrustumCulling,
    pub material: MaterialTilemapHandle<M>,
    pub sync: SyncToRenderWorld,
    pub anchor: TilemapAnchor,
}
```

---

## Prelude Exports

```rust
pub use crate::TilemapPlugin;
pub use crate::TilemapBundle;              // render feature
pub use crate::MaterialTilemapBundle;      // render feature
pub use crate::anchor::TilemapAnchor;
pub use crate::array_texture_preload::*;   // non-atlas + render
pub use crate::helpers;
pub use crate::helpers::filling::*;        // fill_tilemap, fill_tilemap_rect
pub use crate::helpers::geometry::*;
pub use crate::helpers::transform::*;
pub use crate::map::*;                     // All map types
pub use crate::tiles::*;                   // All tile types + TileStorage
pub use crate::render::material::*;        // render feature
```
