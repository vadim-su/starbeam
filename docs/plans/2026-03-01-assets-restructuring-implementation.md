# Assets Restructuring Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Restructure `assets/` from monolithic registries to per-entity autodiscovery (content-pak style), making all data files hot-reloadable and eliminating hardcoded asset definitions.

**Architecture:** Three-layer structure: `engine/` (shaders, UI config), `content/` (tiles, objects, items, characters — autodiscovered by file extension), `world/` (configs, planet types, biomes). Central registries replaced by `load_folder()` + extension-based `RonLoader<T>`.

**Tech Stack:** Rust, Bevy 0.18, RON, custom `RonLoader<T>`

**Design doc:** `docs/plans/2026-03-01-assets-restructuring-design.md`

---

### Task 1: Create new directory structure and RON files

Move/create all asset files in the new layout. Old files stay in place temporarily so the game still compiles.

**Files:**
- Create: `assets/engine/shaders/` (move 4 .wgsl files)
- Create: `assets/engine/ui.config.ron`
- Create: `assets/content/tiles/{grass,dirt,stone}/` (split tiles.registry.ron + move PNGs)
- Create: `assets/content/tiles/{grass,dirt,stone}/item.ron` (new — from hardcoded items)
- Create: `assets/content/objects/{torch,wooden_chest,wooden_table}/` (split objects.objects.ron + move PNGs)
- Create: `assets/content/objects/torch/item.ron` (new — from hardcoded items)
- Create: `assets/content/items/` (empty, for future orphan items)
- Create: `assets/content/characters/adventurer/` (merge def + metadata)
- Move: biome parallax.ron → {name}.parallax.ron

**Step 1: Create directory structure**

```bash
# Engine
mkdir -p assets/engine/shaders

# Content
mkdir -p assets/content/tiles/air
mkdir -p assets/content/tiles/grass
mkdir -p assets/content/tiles/dirt
mkdir -p assets/content/tiles/stone
mkdir -p assets/content/objects/none
mkdir -p assets/content/objects/torch
mkdir -p assets/content/objects/wooden_chest
mkdir -p assets/content/objects/wooden_table
mkdir -p assets/content/items
mkdir -p assets/content/characters/adventurer/sprites/staying
mkdir -p assets/content/characters/adventurer/sprites/running
mkdir -p assets/content/characters/adventurer/sprites/jumping
```

**Step 2: Move shaders**

```bash
git mv assets/shaders/lit_sprite.wgsl assets/engine/shaders/lit_sprite.wgsl
git mv assets/shaders/tile.wgsl assets/engine/shaders/tile.wgsl
git mv assets/shaders/radiance_cascades.wgsl assets/engine/shaders/radiance_cascades.wgsl
git mv assets/shaders/rc_finalize.wgsl assets/engine/shaders/rc_finalize.wgsl
rmdir assets/shaders
```

**Step 3: Create engine/ui.config.ron** (from ui.ron, remove base_path)

```ron
(
    font_size: 12.0,

    colors: (
        bg_dark: "#1a1410",
        bg_medium: "#2a2420",
        border: "#5a4a3a",
        border_highlight: "#8a7a6a",
        selected: "#ffcc00",
        text: "#e0d0c0",
        text_dim: "#8a7a6a",
        rarity_common: "#aaaaaa",
        rarity_uncommon: "#55ff55",
        rarity_rare: "#5555ff",
        rarity_legendary: "#ffaa00",
    ),

    hotbar: (
        slots: 6,
        slot_size: 48.0,
        gap: 4.0,
        anchor: "BottomCenter",
        margin_bottom: 16.0,
        border_width: 2.0,
    ),

    inventory_screen: (
        anchor: "Center",
        width: 400.0,
        height: 320.0,
        padding: 16.0,
        equipment: (
            slot_size: 40.0,
            gap: 4.0,
        ),
        main_bag: (
            columns: 8,
            rows: 5,
            slot_size: 32.0,
            gap: 2.0,
        ),
        material_bag: (
            columns: 8,
            rows: 2,
            slot_size: 32.0,
            gap: 2.0,
        ),
    ),

    tooltip: (
        padding: 8.0,
        max_width: 200.0,
        border_width: 1.0,
    ),
)
```

**Step 4: Split tiles.registry.ron into per-tile RON files**

Create `assets/content/tiles/air/air.tile.ron`:
```ron
( id: "air", autotile: None, solid: false, hardness: 0.0, friction: 0.0, viscosity: 0.0, damage_on_contact: 0.0, effects: [], light_emission: (0, 0, 0), light_opacity: 0, albedo: (0, 0, 0), drops: [] )
```

Create `assets/content/tiles/grass/grass.tile.ron`:
```ron
( id: "grass", autotile: Some("grass"), solid: true, hardness: 1.0, friction: 0.8, viscosity: 0.0, damage_on_contact: 0.0, effects: [], light_emission: (0, 0, 0), light_opacity: 13, albedo: (34, 139, 34), drops: [( item_id: "dirt", min: 1, max: 1, chance: 1.0 )] )
```

Create `assets/content/tiles/dirt/dirt.tile.ron`:
```ron
( id: "dirt", autotile: Some("dirt"), solid: true, hardness: 2.0, friction: 0.7, viscosity: 0.0, damage_on_contact: 0.0, effects: [], light_emission: (0, 0, 0), light_opacity: 14, albedo: (139, 90, 43), drops: [( item_id: "dirt", min: 1, max: 1, chance: 1.0 )] )
```

Create `assets/content/tiles/stone/stone.tile.ron`:
```ron
( id: "stone", autotile: Some("stone"), solid: true, hardness: 5.0, friction: 0.6, viscosity: 0.0, damage_on_contact: 0.0, effects: [], light_emission: (0, 0, 0), light_opacity: 15, albedo: (128, 128, 128), drops: [( item_id: "stone", min: 1, max: 1, chance: 1.0 )] )
```

**Step 5: Move terrain assets into tile folders**

```bash
# Grass
git mv assets/world/terrain/grass.autotile.ron assets/content/tiles/grass/grass.autotile.ron
git mv assets/world/terrain/grass.png assets/content/tiles/grass/grass.png
git mv assets/world/terrain/sources/grass.png assets/content/tiles/grass/source.png

# Dirt
git mv assets/world/terrain/dirt.autotile.ron assets/content/tiles/dirt/dirt.autotile.ron
git mv assets/world/terrain/dirt.png assets/content/tiles/dirt/dirt.png
git mv assets/world/terrain/sources/dirt.png assets/content/tiles/dirt/source.png

# Stone
git mv assets/world/terrain/stone.autotile.ron assets/content/tiles/stone/stone.autotile.ron
git mv assets/world/terrain/stone.png assets/content/tiles/stone/stone.png
git mv assets/world/terrain/sources/stone.png assets/content/tiles/stone/source.png
```

**Step 6: Split objects.objects.ron into per-object RON files**

Create `assets/content/objects/none/none.object.ron`:
```ron
( id: "none", display_name: "None", size: (1, 1), sprite: "", placement: Any, object_type: Decoration )
```

Create `assets/content/objects/torch/torch.object.ron`:
```ron
(
  id: "torch_object",
  display_name: "Torch",
  size: (1, 1),
  sprite: "torch.png",
  solid_mask: [false],
  placement: FloorOrWall,
  light_emission: (255, 170, 40),
  object_type: LightSource,
  drops: [( item_id: "torch", min: 1, max: 1, chance: 1.0 )],
  sprite_columns: 4,
  sprite_rows: 4,
  sprite_fps: 10.0,
  flicker_speed: 3.0,
  flicker_strength: 0.5,
  flicker_min: 0.5,
)
```

Note: `sprite: "torch.png"` is now relative to the object's folder. The loading code will resolve this later (Task 5).

Create `assets/content/objects/wooden_chest/wooden_chest.object.ron`:
```ron
(
  id: "wooden_chest",
  display_name: "Wooden Chest",
  size: (2, 1),
  sprite: "wooden_chest.png",
  solid_mask: [true, true],
  placement: Floor,
  object_type: Container( slots: 16 ),
)
```

Create `assets/content/objects/wooden_table/wooden_table.object.ron`:
```ron
(
  id: "wooden_table",
  display_name: "Wooden Table",
  size: (3, 2),
  sprite: "wooden_table.png",
  solid_mask: [true, false, true, false, false, false],
  placement: Floor,
  object_type: Decoration,
)
```

**Step 7: Move object sprites**

```bash
git mv assets/objects/torch.png assets/content/objects/torch/torch.png
git mv assets/objects/wooden_chest.png assets/content/objects/wooden_chest/wooden_chest.png
git mv assets/objects/wooden_table.png assets/content/objects/wooden_table/wooden_table.png
```

**Step 8: Create item.ron files (new — from hardcoded item/plugin.rs)**

Create `assets/content/tiles/dirt/item.ron`:
```ron
(
  id: "dirt",
  display_name: "Dirt Block",
  description: "A block of common dirt.",
  max_stack: 999,
  rarity: Common,
  item_type: Block,
  icon: "item.png",
  placeable: Some("dirt"),
)
```

Create `assets/content/tiles/stone/item.ron`:
```ron
(
  id: "stone",
  display_name: "Stone Block",
  description: "A solid block of stone.",
  max_stack: 999,
  rarity: Common,
  item_type: Block,
  icon: "item.png",
  placeable: Some("stone"),
)
```

Create `assets/content/tiles/grass/item.ron`:
```ron
(
  id: "grass",
  display_name: "Grass Block",
  description: "A block of grass-covered dirt.",
  max_stack: 999,
  rarity: Common,
  item_type: Block,
  icon: "item.png",
  placeable: Some("grass"),
)
```

Create `assets/content/objects/torch/item.ron`:
```ron
(
  id: "torch",
  display_name: "Torch",
  description: "A simple torch that emits warm light.",
  max_stack: 999,
  rarity: Common,
  item_type: Block,
  icon: "item.png",
  placeable_object: Some("torch_object"),
)
```

**Step 9: Move item icons to parent folders**

```bash
git mv assets/items/dirt.png assets/content/tiles/dirt/item.png
git mv assets/items/stone.png assets/content/tiles/stone/item.png
git mv assets/items/grass.png assets/content/tiles/grass/item.png
git mv assets/items/torch.png assets/content/objects/torch/item.png
```

**Step 10: Create character.ron (merge def + metadata)**

Create `assets/content/characters/adventurer/adventurer.character.ron`:
```ron
(
  speed: 100.0,
  jump_velocity: 220.0,
  gravity: 500.0,
  width: 16.0,
  height: 32.0,
  magnet_radius: 48.0,
  magnet_strength: 400.0,
  pickup_radius: 20.0,

  sprite_size: (44, 44),
  animations: {
    "staying": (
      frames: ["sprites/staying/frame_000.png"],
      fps: 1.0,
    ),
    "running": (
      frames: [
        "sprites/running/frame_000.png",
        "sprites/running/frame_001.png",
        "sprites/running/frame_002.png",
        "sprites/running/frame_003.png",
      ],
      fps: 10.0,
    ),
    "jumping": (
      frames: [
        "sprites/jumping/frame_000.png",
        "sprites/jumping/frame_001.png",
        "sprites/jumping/frame_002.png",
        "sprites/jumping/frame_003.png",
        "sprites/jumping/frame_004.png",
        "sprites/jumping/frame_005.png",
        "sprites/jumping/frame_006.png",
      ],
      fps: 12.0,
    ),
  },
)
```

**Step 11: Move character sprites**

```bash
git mv assets/characters/adventurer/sprites/staying/frame_000.png assets/content/characters/adventurer/sprites/staying/frame_000.png
git mv assets/characters/adventurer/sprites/running/* assets/content/characters/adventurer/sprites/running/
git mv assets/characters/adventurer/sprites/jumping/* assets/content/characters/adventurer/sprites/jumping/
```

**Step 12: Rename parallax.ron files to add name prefix**

```bash
git mv assets/world/biomes/meadow/parallax.ron assets/world/biomes/meadow/meadow.parallax.ron
git mv assets/world/biomes/forest/parallax.ron assets/world/biomes/forest/forest.parallax.ron
git mv assets/world/biomes/rocky/parallax.ron assets/world/biomes/rocky/rocky.parallax.ron
```

Update parallax references inside biome RON files:
- `assets/world/biomes/meadow/meadow.biome.ron`: `parallax: Some("world/biomes/meadow/parallax.ron")` → `Some("world/biomes/meadow/meadow.parallax.ron")`
- `assets/world/biomes/forest/forest.biome.ron`: same pattern
- `assets/world/biomes/rocky/rocky.biome.ron`: same pattern

**Step 13: Commit**

```bash
git add -A
git commit -m "refactor: create new content-pak asset directory structure"
```

---

### Task 2: New asset types and loader registrations

Add `ItemDefAsset`, `CharacterDefAsset` asset types. Update RON loader registrations. Change `*.tile.ron` / `*.object.ron` to load single defs instead of registries.

**Files:**
- Modify: `src/registry/assets.rs` — add new asset types, change TileRegistryAsset → TileDefAsset, ObjectRegistryAsset → ObjectDefAsset
- Modify: `src/registry/mod.rs` — register new loaders, update imports
- Modify: `src/item/definition.rs` — derive Asset + TypePath on ItemDef

**Step 1: Add TileDefAsset and ObjectDefAsset to assets.rs**

Replace in `src/registry/assets.rs`:

```rust
// OLD (lines 11-21):
/// Asset loaded from tiles.registry.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct TileRegistryAsset {
    pub tiles: Vec<TileDef>,
}

/// Asset loaded from objects.registry.ron
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct ObjectRegistryAsset {
    pub objects: Vec<ObjectDef>,
}

// NEW — keep old types for backward compat during migration, add new ones:

/// Single tile definition loaded from *.tile.ron (autodiscovered)
pub type TileDefAsset = TileDef;  // TileDef already has Deserialize

/// Single object definition loaded from *.object.ron (autodiscovered)
pub type ObjectDefAsset = ObjectDef;  // ObjectDef already has Deserialize

/// Single item definition loaded from item.ron (autodiscovered)
pub type ItemDefAsset = ItemDef;

/// Character definition loaded from *.character.ron (autodiscovered)
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct CharacterDefAsset {
    pub speed: f32,
    pub jump_velocity: f32,
    pub gravity: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default = "default_magnet_radius")]
    pub magnet_radius: f32,
    #[serde(default = "default_magnet_strength")]
    pub magnet_strength: f32,
    #[serde(default = "default_pickup_radius")]
    pub pickup_radius: f32,
    pub sprite_size: (u32, u32),
    pub animations: HashMap<String, AnimationDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnimationDef {
    pub frames: Vec<String>,
    pub fps: f32,
}
```

Note: `TileDef`, `ObjectDef`, and `ItemDef` all need `#[derive(Asset, TypePath)]` added to their struct definitions for this to work. If they don't already have it, add it:
- `src/registry/tile.rs` — add `Asset, TypePath` to `TileDef` derives
- `src/object/definition.rs` — add `Asset, TypePath` to `ObjectDef` derives
- `src/item/definition.rs` — add `Asset, TypePath` to `ItemDef` derives

**Step 2: Update mod.rs loader registrations**

In `src/registry/mod.rs`, replace the loader registrations (lines 61-76):

```rust
// OLD loaders:
.init_asset::<TileRegistryAsset>()
.init_asset::<ObjectRegistryAsset>()
.init_asset::<PlayerDefAsset>()
.init_asset::<WorldConfigAsset>()
.init_asset::<ParallaxConfigAsset>()
.init_asset::<AutotileAsset>()
.register_asset_loader(RonLoader::<TileRegistryAsset>::new(&["registry.ron"]))
.register_asset_loader(RonLoader::<ObjectRegistryAsset>::new(&["objects.ron"]))
.register_asset_loader(RonLoader::<PlayerDefAsset>::new(&["def.ron"]))
.register_asset_loader(RonLoader::<WorldConfigAsset>::new(&["config.ron"]))
.register_asset_loader(RonLoader::<ParallaxConfigAsset>::new(&["parallax.ron"]))
.register_asset_loader(RonLoader::<AutotileAsset>::new(&["autotile.ron"]))
.init_asset::<PlanetTypeAsset>()
.init_asset::<BiomeAsset>()
.register_asset_loader(RonLoader::<PlanetTypeAsset>::new(&["planet.ron"]))
.register_asset_loader(RonLoader::<BiomeAsset>::new(&["biome.ron"]))

// NEW loaders:
.init_asset::<TileDefAsset>()
.init_asset::<ObjectDefAsset>()
.init_asset::<ItemDefAsset>()
.init_asset::<CharacterDefAsset>()
.init_asset::<AutotileAsset>()
.init_asset::<ParallaxConfigAsset>()
.init_asset::<PlanetTypeAsset>()
.init_asset::<BiomeAsset>()
.register_asset_loader(RonLoader::<TileDefAsset>::new(&["tile.ron"]))
.register_asset_loader(RonLoader::<ObjectDefAsset>::new(&["object.ron"]))
.register_asset_loader(RonLoader::<ItemDefAsset>::new(&["item.ron"]))
.register_asset_loader(RonLoader::<CharacterDefAsset>::new(&["character.ron"]))
.register_asset_loader(RonLoader::<AutotileAsset>::new(&["autotile.ron"]))
.register_asset_loader(RonLoader::<ParallaxConfigAsset>::new(&["parallax.ron"]))
.register_asset_loader(RonLoader::<PlanetTypeAsset>::new(&["planet.ron"]))
.register_asset_loader(RonLoader::<BiomeAsset>::new(&["biome.ron"]))
```

Note: `*.config.ron` is NO LONGER registered globally. WorldConfig, DayNightConfig, UiConfig will all be loaded explicitly with type annotation.

**Step 3: Update RegistryHandles**

In `src/registry/mod.rs`, update `RegistryHandles` to hold Vec of individual handles:

```rust
// OLD:
pub struct RegistryHandles {
    pub tiles: Handle<TileRegistryAsset>,
    pub objects: Handle<ObjectRegistryAsset>,
    pub player: Handle<PlayerDefAsset>,
    pub world_config: Handle<WorldConfigAsset>,
}

// NEW:
pub struct RegistryHandles {
    pub tiles: Vec<Handle<TileDefAsset>>,
    pub objects: Vec<Handle<ObjectDefAsset>>,
    pub items: Vec<Handle<ItemDefAsset>>,
    pub character: Handle<CharacterDefAsset>,
    pub world_config: Handle<WorldConfigAsset>,
}
```

**Step 4: Compile check**

```bash
cargo check 2>&1 | head -50
```

Expected: compilation errors in loading.rs and hot_reload.rs (they still reference old types). That's fine — Task 3 fixes them.

**Step 5: Commit**

```bash
git add -A
git commit -m "refactor: add new per-entity asset types and loader registrations"
```

---

### Task 3: Rewrite loading pipeline for autodiscovery

Replace `start_loading` and `check_loading` to use `load_folder()` on `content/` directories, collecting individual TileDefAsset/ObjectDefAsset/ItemDefAsset handles.

**Files:**
- Modify: `src/registry/loading.rs` — rewrite start_loading, check_loading, start_autotile_loading
- Modify: `src/registry/hot_reload.rs` — update to work with new handle types

**Step 1: Rewrite start_loading (loading.rs lines 54-65)**

```rust
// OLD:
pub(crate) fn start_loading(mut commands: Commands, asset_server: Res<AssetServer>) {
    let tiles = asset_server.load::<TileRegistryAsset>("world/tiles.registry.ron");
    let objects = asset_server.load::<ObjectRegistryAsset>("world/objects.objects.ron");
    let player = asset_server.load::<PlayerDefAsset>("characters/adventurer/adventurer.def.ron");
    let world_config = asset_server.load::<WorldConfigAsset>("world/world.config.ron");
    ...
}

// NEW:
pub(crate) fn start_loading(mut commands: Commands, asset_server: Res<AssetServer>) {
    // Autodiscovery: load all content folders recursively
    // Bevy's load_folder returns handles for all assets matching registered loaders
    let tile_folder = asset_server.load_folder("content/tiles");
    let object_folder = asset_server.load_folder("content/objects");
    let content_folder = asset_server.load_folder("content");  // catches all item.ron
    let character_folder = asset_server.load_folder("content/characters");

    // Explicit config loads (not autodiscovered)
    let world_config = asset_server.load::<WorldConfigAsset>("world/world.config.ron");

    commands.insert_resource(LoadingAssets {
        tile_folder,
        object_folder,
        content_folder,
        character_folder,
        world_config,
    });
}
```

Update `LoadingAssets` struct accordingly:

```rust
#[derive(Resource)]
pub(crate) struct LoadingAssets {
    tile_folder: Handle<LoadedFolder>,
    object_folder: Handle<LoadedFolder>,
    content_folder: Handle<LoadedFolder>,
    character_folder: Handle<LoadedFolder>,
    world_config: Handle<WorldConfigAsset>,
}
```

**Step 2: Rewrite check_loading to collect discovered assets**

The check_loading system waits for all folders to load, then iterates typed asset handles from the loaded folders to build registries. This is the core autodiscovery logic.

Key pattern: iterate `loaded_folder.handles`, try `assets.get(handle.id().typed::<TileDefAsset>())` for each, collect successful ones.

Note: Bevy `load_folder` loads ALL files recursively. The `RonLoader<T>` extension matching ensures only `*.tile.ron` files become `TileDefAsset`, only `*.object.ron` → `ObjectDefAsset`, etc. PNG files load as `Image`. Non-matching extensions are ignored.

**Step 3: Rewrite start_autotile_loading path resolution**

```rust
// OLD (lines 390-393):
let ron_handle = asset_server.load::<AutotileAsset>(format!("world/terrain/{name}.autotile.ron"));
let img_handle = asset_server.load::<Image>(format!("world/terrain/{name}.png"));

// NEW:
let ron_handle = asset_server.load::<AutotileAsset>(format!("content/tiles/{name}/{name}.autotile.ron"));
let img_handle = asset_server.load::<Image>(format!("content/tiles/{name}/{name}.png"));
```

**Step 4: Update hot_reload.rs for new handle types**

The hot_reload systems listen for `AssetEvent::Modified`. They need to be updated to iterate `Vec<Handle<TileDefAsset>>` instead of checking a single `Handle<TileRegistryAsset>`.

**Step 5: Compile and run**

```bash
cargo run 2>&1 | head -30
```

Expected: game starts, tiles/objects load from new paths.

**Step 6: Commit**

```bash
git add -A
git commit -m "refactor: rewrite loading pipeline for content autodiscovery"
```

---

### Task 4: Data-driven items — remove hardcoded ItemDef

Replace hardcoded item definitions in `src/item/plugin.rs` with autodiscovery from `item.ron` files.

**Files:**
- Modify: `src/item/plugin.rs` — remove hardcoded ItemDef vec, build ItemRegistry from loaded ItemDefAssets
- Modify: `src/registry/loading.rs` — build ItemRegistry from autodiscovered item.ron files
- Modify: `src/ui/game_ui/mod.rs:163` — resolve relative icon paths

**Step 1: Update item/plugin.rs**

```rust
// OLD (lines 10-64): hardcoded ItemRegistry::from_defs(vec![...])
// NEW: ItemRegistry is built in check_loading from autodiscovered ItemDefAssets
// plugin.rs only registers systems:

impl Plugin for ItemPlugin {
    fn build(&self, app: &mut App) {
        // ItemRegistry is inserted by RegistryPlugin during loading
        app.add_systems(Update, despawn_expired_drops);
    }
}
```

**Step 2: Build ItemRegistry in check_loading**

In the check_loading system, after collecting all ItemDefAsset handles:

```rust
let mut items = Vec::new();
for handle in &item_handles {
    if let Some(item_def) = item_assets.get(handle) {
        // Resolve relative icon path: item.ron's folder + icon field
        let mut resolved = item_def.clone();
        let parent = asset_path_parent(handle);  // e.g. "content/tiles/grass/"
        resolved.icon = format!("{}{}", parent, item_def.icon);  // "content/tiles/grass/item.png"
        items.push(resolved);
    }
}
commands.insert_resource(ItemRegistry::from_defs(items));
```

**Step 3: Resolve relative sprite paths for objects too**

Same pattern for ObjectDef.sprite:
```rust
// "torch.png" → "content/objects/torch/torch.png"
resolved.sprite = format!("{}{}", parent, def.sprite);
```

**Step 4: Compile and test item pickup/placement**

```bash
cargo run
```

Verify: items show correct icons in hotbar, placing tiles/objects works.

**Step 5: Commit**

```bash
git add -A
git commit -m "refactor: data-driven items, remove hardcoded ItemDef"
```

---

### Task 5: Data-driven character animations

Replace hardcoded animation frame paths in `src/player/animation.rs` with data from `adventurer.character.ron`.

**Files:**
- Modify: `src/player/animation.rs` — load frames from CharacterDefAsset
- Modify: `src/registry/loading.rs` — pass CharacterDefAsset to PlayerConfig or a new CharacterConfig resource

**Step 1: Extend PlayerConfig or create CharacterConfig**

Add animation data to a resource available at InGame state:

```rust
// In src/registry/player.rs or new file:
#[derive(Resource)]
pub struct CharacterConfig {
    // physics (from CharacterDefAsset)
    pub speed: f32,
    pub jump_velocity: f32,
    pub gravity: f32,
    pub width: f32,
    pub height: f32,
    pub magnet_radius: f32,
    pub magnet_strength: f32,
    pub pickup_radius: f32,
    // sprites
    pub sprite_size: (u32, u32),
    pub animations: HashMap<String, AnimationDef>,
    /// Base path for resolving relative frame paths
    pub base_path: String,
}
```

**Step 2: Rewrite load_character_animations**

```rust
// OLD: 12 hardcoded asset_server.load() calls
// NEW: read from CharacterConfig resource

pub fn load_character_animations(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    config: Res<CharacterConfig>,
) {
    let resolve = |relative: &str| -> Handle<Image> {
        asset_server.load(format!("{}{}", config.base_path, relative))
    };

    let idle_frames = config.animations.get("staying")
        .map(|a| a.frames.iter().map(|f| resolve(f)).collect())
        .unwrap_or_default();

    let running_frames = config.animations.get("running")
        .map(|a| a.frames.iter().map(|f| resolve(f)).collect())
        .unwrap_or_default();

    let jumping_frames = config.animations.get("jumping")
        .map(|a| a.frames.iter().map(|f| resolve(f)).collect())
        .unwrap_or_default();

    commands.insert_resource(CharacterAnimations {
        idle: idle_frames,
        running: running_frames,
        jumping: jumping_frames,
    });
}
```

**Step 3: Compile and test character animations**

```bash
cargo run
```

Verify: character idle/run/jump animations play correctly.

**Step 4: Commit**

```bash
git add -A
git commit -m "refactor: data-driven character animations from character.ron"
```

---

### Task 6: Hot-reloadable day_night and UI configs

Replace `include_str!()` with `AssetServer::load()` for both `day_night.config.ron` and `ui.config.ron`.

**Files:**
- Create: `src/registry/assets.rs` — add `DayNightConfigAsset`
- Modify: `src/world/day_night.rs:221-224` — use AssetServer instead of include_str
- Modify: `src/ui/game_ui/theme.rs:99-102` — use AssetServer instead of include_str
- Modify: `src/registry/mod.rs` — explicit load of these configs (no global extension)

**Step 1: Add DayNightConfigAsset**

In `src/registry/assets.rs` (or `src/world/day_night.rs` alongside the existing `DayNightConfig`):

```rust
#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct DayNightConfigAsset {
    // same fields as current DayNightConfig
    pub cycle_duration_secs: f32,
    pub dawn_ratio: f32,
    pub day_ratio: f32,
    pub sunset_ratio: f32,
    pub night_ratio: f32,
    pub phases: Vec<PhaseConfig>,
}
```

Note: This requires either a custom extension for the loader (e.g. `"daynight.ron"`) OR explicit typed load. Since we decided no global `*.config.ron`, use explicit:

```rust
let day_night = asset_server.load::<DayNightConfigAsset>("world/day_night.config.ron");
```

This needs a dedicated loader. Simplest: register `RonLoader::<DayNightConfigAsset>::new(&["daynight.ron"])` and rename file to `day_night.daynight.ron`. OR: use a unique extension like `"dnconfig.ron"`.

Alternative (simpler): Since day_night and UI configs are few, load them as raw strings and parse manually with `ron::from_str`. This avoids extension conflicts entirely. The Bevy asset server still provides hot-reload for raw assets.

**Recommended approach:** Rename to unique extensions:
- `world/day_night.config.ron` → `world/day_night.daynight.ron` (loader: `*.daynight.ron`)
- `engine/ui.config.ron` → `engine/ui.uitheme.ron` (loader: `*.uitheme.ron`)

OR: Keep `*.config.ron` files but don't register a global loader — manually load via `AssetServer::load::<RawRon>(path)` and parse.

Decide based on implementation convenience. The key requirement is: no `include_str!()`, hot-reload works.

**Step 2: Rewrite load_day_night_config**

```rust
// OLD:
pub fn load_day_night_config(mut commands: Commands) {
    let ron_str = include_str!("../../assets/world/day_night.config.ron");
    let config: DayNightConfig = ron::from_str(ron_str).expect("...");
    ...
}

// NEW — loaded via AssetServer in start_loading, inserted during check_loading
```

**Step 3: Rewrite UiTheme::load()**

```rust
// OLD:
impl UiTheme {
    pub fn load() -> Self {
        let ron_str = include_str!("../../../assets/ui.ron");
        ron::from_str(ron_str).expect("Failed to parse ui.ron")
    }
}

// NEW — loaded via AssetServer, remove base_path field from UiTheme
```

Also remove the dead `base_path` field from `UiTheme` struct (line 90).

**Step 4: Compile and test hot-reload**

```bash
cargo run
```

Edit `assets/engine/ui.uitheme.ron` while game runs → UI should update.

**Step 5: Commit**

```bash
git add -A
git commit -m "refactor: hot-reloadable day/night and UI configs via AssetServer"
```

---

### Task 7: Update shader paths

**Files:**
- Modify: `src/world/lit_sprite.rs:35,39` — `"shaders/..."` → `"engine/shaders/..."`
- Modify: `src/world/tile_renderer.rs:25,29` — same
- Modify: `src/world/rc_pipeline.rs:165-166` — same

**Step 1: Update all shader paths**

```rust
// lit_sprite.rs:
"shaders/lit_sprite.wgsl" → "engine/shaders/lit_sprite.wgsl"

// tile_renderer.rs:
"shaders/tile.wgsl" → "engine/shaders/tile.wgsl"

// rc_pipeline.rs:
"shaders/radiance_cascades.wgsl" → "engine/shaders/radiance_cascades.wgsl"
"shaders/rc_finalize.wgsl" → "engine/shaders/rc_finalize.wgsl"
```

**Step 2: Compile and run**

```bash
cargo run
```

Verify: lighting and tile rendering work (shaders load correctly).

**Step 3: Commit**

```bash
git add -A
git commit -m "refactor: move shader paths to engine/shaders/"
```

---

### Task 8: Update tests and fix stale references

**Files:**
- Modify: `src/registry/assets.rs` test (line 168) — update path
- Modify: `src/object/definition.rs` tests — update sprite paths
- Modify: `src/object/placement.rs` tests — update sprite paths
- Modify: `src/object/registry.rs` tests — update sprite paths
- Modify: `src/item/definition.rs` tests — update icon paths
- Modify: `src/item/registry.rs` tests — update icon paths
- Modify: `src/world/chunk.rs` tests — update sprite paths
- Modify: `src/registry/biome.rs` tests — update parallax path
- Fix stale doc comments in `src/registry/assets.rs`

**Step 1: Update test paths**

All test helper functions that create dummy ObjectDef/ItemDef with paths like `"objects/torch.png"` or `"items/dirt.png"` should use `"content/objects/torch/torch.png"` or `"content/tiles/dirt/item.png"` (or just empty strings, since tests don't actually load files).

**Step 2: Fix stale doc comments in assets.rs**

```rust
// Line 11: "Asset loaded from tiles.registry.ron" → remove or update
// Line 17: "Asset loaded from objects.registry.ron" → remove
// Line 23: "Asset loaded from player.def.ron" → remove
// Line 62: "Asset loaded from bg.parallax.ron" → update
```

**Step 3: Run all tests**

```bash
cargo test 2>&1
```

Expected: all tests pass.

**Step 4: Commit**

```bash
git add -A
git commit -m "refactor: update test paths and fix stale doc comments"
```

---

### Task 9: Delete old files and clean up

Remove all old asset files that have been migrated.

**Files:**
- Delete: `assets/world/tiles.registry.ron`
- Delete: `assets/world/objects.objects.ron`
- Delete: `assets/world/terrain/` (entire directory — content moved to content/tiles/)
- Delete: `assets/world/backgrounds/` (empty, only .gitkeep)
- Delete: `assets/items/` (icons moved to parent entities)
- Delete: `assets/objects/` (sprites moved to content/objects/)
- Delete: `assets/characters/` (moved to content/characters/)
- Delete: `assets/ui.ron` (replaced by engine/ui.config.ron)
- Delete: `assets/ui/` (empty directory)

**Step 1: Remove old files**

```bash
git rm assets/world/tiles.registry.ron
git rm assets/world/objects.objects.ron
git rm -r assets/world/terrain/
git rm -r assets/world/backgrounds/
git rm -r assets/items/
git rm -r assets/objects/
git rm -r assets/characters/
git rm assets/ui.ron
rm -rf assets/ui/
```

**Step 2: Verify no dangling references**

```bash
grep -r "world/tiles.registry" src/
grep -r "world/objects.objects" src/
grep -r "\"items/" src/
grep -r "\"objects/" src/
grep -r "\"characters/" src/
grep -r "\"shaders/" src/
grep -r "include_str!" src/
```

Expected: no matches (all references updated in previous tasks).

**Step 3: Full build and test**

```bash
cargo test && cargo run
```

Expected: everything compiles, all tests pass, game runs correctly.

**Step 4: Commit**

```bash
git add -A
git commit -m "refactor: remove old asset files, restructuring complete"
```

---

### Task 10: Update autotile47.py script

Update the Python autotile generation script to work with new paths.

**Files:**
- Modify: `scripts/autotile47.py` — update documentation/examples for new paths

**Step 1: Update script usage examples**

The script itself uses CLI arguments for paths, so no hardcoded paths need changing. But update any help text or examples:

```
# OLD usage:
# python scripts/autotile47.py assets/world/terrain/sources/grass.png -o assets/world/terrain/grass.png --ron assets/world/terrain/grass.autotile.ron

# NEW usage:
# python scripts/autotile47.py assets/content/tiles/grass/source.png -o assets/content/tiles/grass/grass.png --ron assets/content/tiles/grass/grass.autotile.ron
```

**Step 2: Commit**

```bash
git add -A
git commit -m "docs: update autotile47.py usage for new asset paths"
```
