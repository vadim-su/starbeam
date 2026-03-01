# Assets Restructuring Design

**Date:** 2026-03-01  
**Status:** Approved  
**Scope:** Directory structure, RON data format, asset loading pipeline

## Problem

Current `assets/` structure has accumulated inconsistencies that will not scale to Starbound-level content (100+ tiles, 200+ objects, hundreds of items):

1. **Monolithic registries** — `tiles.registry.ron` and `objects.objects.ron` contain all definitions in one file
2. **Hardcoded items** — item definitions live in `src/item/plugin.rs`, not in data files
3. **Hardcoded animations** — character sprite paths hardcoded in `src/player/animation.rs`, `metadata.ron` unused by code
4. **`include_str!()` for configs** — `day_night.config.ron` and `ui.ron` baked at compile time, no hot-reload
5. **Extension conflict** — `*.config.ron` registered globally for `WorldConfigAsset`, but `day_night.config.ron` has different schema
6. **Dead references** — `ui.ron` has unused `base_path`, stale comments in `assets.rs`
7. **Inconsistent naming** — `metadata.ron`, `ui.ron` lack type suffixes; `parallax.ron` lacks name prefix; `objects.objects.ron` reads awkwardly
8. **Scattered assets** — related assets (tile + its drop item + icon) spread across `world/terrain/`, `items/`, `world/tiles.registry.ron`

## Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Grouping | By game domain (Starbound-style) | Everything about one entity in one folder |
| Registries | Autodiscovery (scan folders) | Add folder = add content, no manifest editing |
| Scale target | Hundreds of each type | Structure must not degrade at scale |
| Engine vs content | `engine/` separated from `content/` | Clear boundary for artists/designers |
| Item co-location | `item.ron` + `item.png` inside parent's folder | Tile's drop item lives with the tile |

## Directory Structure

```
assets/
  engine/                              # engine infrastructure
    shaders/
      lit_sprite.wgsl
      tile.wgsl
      radiance_cascades.wgsl
      rc_finalize.wgsl
    ui.config.ron

  content/                             # all game content
    tiles/
      grass/
        grass.tile.ron                 # tile definition
        grass.autotile.ron             # bitmask mapping
        grass.png                      # generated atlas
        source.png                     # source for autotile47.py
        item.ron                       # drop item definition
        item.png                       # inventory icon
      dirt/
        dirt.tile.ron
        dirt.autotile.ron
        dirt.png
        source.png
        item.ron
        item.png
      stone/
        stone.tile.ron
        stone.autotile.ron
        stone.png
        source.png
        item.ron
        item.png

    objects/
      torch/
        torch.object.ron
        torch.png                      # object sprite
        item.ron                       # drop item
        item.png                       # inventory icon
      wooden_chest/
        wooden_chest.object.ron
        wooden_chest.png
      wooden_table/
        wooden_table.object.ron
        wooden_table.png

    items/                             # orphan items (crafting materials, etc.)
      # empty for now

    characters/
      adventurer/
        adventurer.character.ron       # merged def + metadata
        sprites/
          staying/
            frame_000.png
          running/
            frame_000.png
            frame_001.png
            frame_002.png
            frame_003.png
          jumping/
            frame_000.png ... frame_006.png

  world/                               # world configs and generation
    world.config.ron
    day_night.config.ron
    planet_types/
      garden.planet.ron
    biomes/
      meadow/
        meadow.biome.ron
        meadow.parallax.ron
        backgrounds/
          sky.png
          far_hills.png
          near_hills.png
      forest/
        forest.biome.ron
        forest.parallax.ron
        backgrounds/
          sky.png
          far_trees.png
          near_trees.png
      rocky/
        rocky.biome.ron
        rocky.parallax.ron
        backgrounds/
          sky.png
          far_rocks.png
          near_rocks.png
      underground_dirt/
        underground_dirt.biome.ron
      underground_rock/
        underground_rock.biome.ron
      core_magma/
        core_magma.biome.ron
```

## RON File Formats

### `*.tile.ron` — TileDef

```ron
TileDef(
    id: "grass",
    solid: true,
    hardness: 1.0,
    friction: 1.0,
    viscosity: 0.0,
    light_emission: (0, 0, 0),
    light_opacity: 255,
    albedo: (34, 139, 34),
    drops: Some((item_id: "grass", quantity: 1)),
    autotile: Some("grass"),
)
```

### `item.ron` — ItemDef

```ron
ItemDef(
    id: "grass",
    name: "Grass Block",
    icon: "item.png",
    max_stack: 999,
    places_tile: Some("grass"),
)
```

For objects:
```ron
ItemDef(
    id: "torch",
    name: "Torch",
    icon: "item.png",
    max_stack: 999,
    places_object: Some("torch"),
)
```

### `*.object.ron` — ObjectDef

```ron
ObjectDef(
    id: "torch",
    size: (1, 1),
    sprite: "torch.png",
    sprite_columns: 4,
    sprite_rows: 4,
    animation_fps: 10.0,
    flicker_intensity: 0.15,
    flicker_speed: 3.0,
    solid_mask: [false],
    placement: Floor,
    object_type: Decoration,
    light_emission: Some((255, 170, 40)),
    light_radius: Some(80.0),
    drops: Some((item_id: "torch", quantity: 1)),
)
```

### `*.character.ron` — CharacterDef

Merges `adventurer.def.ron` + `metadata.ron`:

```ron
CharacterDef(
    speed: 100.0,
    jump_velocity: 220.0,
    gravity: 500.0,
    hitbox: (width: 16.0, height: 32.0),
    magnet_radius: 48.0,
    pickup_radius: 16.0,
    sprite_size: (44, 44),
    animations: {
        "staying": Animation(
            frames: ["sprites/staying/frame_000.png"],
            fps: 1.0,
        ),
        "running": Animation(
            frames: [
                "sprites/running/frame_000.png",
                "sprites/running/frame_001.png",
                "sprites/running/frame_002.png",
                "sprites/running/frame_003.png",
            ],
            fps: 10.0,
        ),
        "jumping": Animation(
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

### Naming Convention

| Extension | Asset Type | Autodiscovery Pattern |
|---|---|---|
| `*.tile.ron` | `TileDefAsset` | `content/tiles/*/*.tile.ron` |
| `*.object.ron` | `ObjectDefAsset` | `content/objects/*/*.object.ron` |
| `item.ron` | `ItemDefAsset` | `content/**/item.ron` |
| `*.character.ron` | `CharacterDefAsset` | `content/characters/*/*.character.ron` |
| `*.autotile.ron` | `AutotileAsset` | `content/tiles/*/*.autotile.ron` |
| `*.planet.ron` | `PlanetTypeAsset` | `world/planet_types/*.planet.ron` |
| `*.biome.ron` | `BiomeAsset` | `world/biomes/*/*.biome.ron` |
| `*.parallax.ron` | `ParallaxAsset` | `world/biomes/*/*.parallax.ron` |
| `*.config.ron` | Various (loaded explicitly) | No autodiscovery |

## Autodiscovery and Loading

### Principle

No central registries. Engine scans folders and loads all matching RON files by extension.

### Loading Pipeline

```
AppState::Loading
  1. load_folder("content/tiles")      → Vec<Handle<TileDefAsset>>
     load_folder("content/objects")    → Vec<Handle<ObjectDefAsset>>
     load_folder("content")            → catches all item.ron via ItemDefAsset
     load_folder("content/characters") → Vec<Handle<CharacterDefAsset>>
     load("world/world.config.ron")    → WorldConfigAsset         (explicit)
     load("world/day_night.config.ron") → DayNightConfigAsset     (explicit, NEW type)
     load("engine/ui.config.ron")      → UiConfigAsset            (explicit, NEW type)

AppState::LoadingBiomes
  2. All TileDef/ObjectDef/ItemDef/CharacterDef ready
     → build TileRegistry, ObjectRegistry, ItemRegistry
     → load planet type from world/planet_types/
     → load biomes from world/biomes/

AppState::LoadingAutotile
  3. Find tiles with autotile != None
     → load *.autotile.ron + *.png from same tile folder
     → build combined atlas
```

### Path Resolution

All paths in RON files are **relative to the RON file's directory**:

```ron
// content/objects/torch/torch.object.ron
ObjectDef(
    sprite: "torch.png",        // resolves to content/objects/torch/torch.png
)
```

### Extension Conflict Resolution

`*.config.ron` is NOT registered globally. Config files are loaded explicitly with type annotation:

```rust
let world_cfg  = asset_server.load::<WorldConfigAsset>("world/world.config.ron");
let day_night  = asset_server.load::<DayNightConfigAsset>("world/day_night.config.ron");
let ui_cfg     = asset_server.load::<UiConfigAsset>("engine/ui.config.ron");
```

### Killing `include_str!()`

`day_night.config.ron` and `ui.ron` move to AssetServer loading → hot-reload enabled.

## Migration Map

### Files split apart

| Old File | New Files |
|---|---|
| `world/tiles.registry.ron` | `content/tiles/{grass,dirt,stone}/*.tile.ron` |
| `world/objects.objects.ron` | `content/objects/{torch,wooden_chest,wooden_table}/*.object.ron` |
| `src/item/plugin.rs` (hardcoded items) | `content/tiles/{grass,dirt,stone}/item.ron`, `content/objects/torch/item.ron` |

### Files merged

| Old Files | New File |
|---|---|
| `characters/adventurer/adventurer.def.ron` + `characters/adventurer/metadata.ron` | `content/characters/adventurer/adventurer.character.ron` |

### Files moved

| Old Path | New Path |
|---|---|
| `shaders/*.wgsl` | `engine/shaders/*.wgsl` |
| `ui.ron` | `engine/ui.config.ron` (remove `base_path`) |
| `world/terrain/grass.*` | `content/tiles/grass/` |
| `world/terrain/dirt.*` | `content/tiles/dirt/` |
| `world/terrain/stone.*` | `content/tiles/stone/` |
| `world/terrain/sources/{name}.png` | `content/tiles/{name}/source.png` |
| `objects/{name}.png` | `content/objects/{name}/{name}.png` |
| `items/{name}.png` | `content/tiles/{name}/item.png` or `content/objects/{name}/item.png` |
| `characters/adventurer/sprites/**` | `content/characters/adventurer/sprites/**` |
| `world/biomes/*/parallax.ron` | `world/biomes/*/{name}.parallax.ron` |

### Files deleted

| Path | Reason |
|---|---|
| `world/backgrounds/` + `.gitkeep` | Empty artifact |
| `ui/` (empty directory) | Unused |
| `world/terrain/` (entire directory) | Content moved to `content/tiles/` |
| `world/tiles.registry.ron` | Split into individual files |
| `world/objects.objects.ron` | Split into individual files |
| `items/` (top-level) | Icons moved to parent entities |
| `objects/` (top-level) | Sprites moved to `content/objects/` |
| `characters/` (top-level) | Moved to `content/characters/` |

## Code Changes

| File | Change |
|---|---|
| `src/registry/mod.rs` | New types: `ItemDefAsset`, `CharacterDefAsset`, `DayNightConfigAsset`, `UiConfigAsset`. Remove global `*.config.ron` registration. |
| `src/registry/loading.rs` | `load_folder()` instead of explicit `load()` for content. Build `ItemRegistry` from autodiscovery. |
| `src/registry/assets.rs` | New structs `ItemDef`, `CharacterDef`. Fix stale comments. |
| `src/item/plugin.rs` | Remove hardcoded items, read from `ItemRegistry`. |
| `src/player/animation.rs` | Remove hardcoded sprite paths, read from `CharacterDef.animations`. |
| `src/world/day_night.rs` | `include_str!()` → `asset_server.load::<DayNightConfigAsset>()`. |
| `src/ui/game_ui/theme.rs` | `include_str!()` → `asset_server.load::<UiConfigAsset>()`. |
| `src/world/lit_sprite.rs` | Update shader path: `shaders/` → `engine/shaders/`. |
| `src/world/tile_renderer.rs` | Update shader path. |
| `src/world/rc_pipeline.rs` | Update shader path. |
| `scripts/autotile47.py` | Update input/output paths for new structure. |
