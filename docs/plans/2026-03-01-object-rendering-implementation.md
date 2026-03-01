# Object Rendering & Torch Migration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Render placed objects as visible, animated sprites with RC lighting integration; migrate torch from tile to placeable object.

**Architecture:** Extend LitSpriteMaterial with sprite UV rect for sprite sheet sub-regions. Per-type shared materials for efficient synchronized animation. Object light emission injected into RC emissive buffer via occupancy grid lookup.

**Tech Stack:** Bevy 0.18, WGSL shaders, Rust

---

### Task 1: Extend LitSpriteMaterial with sprite_uv_rect

**Files:**
- Modify: `src/world/lit_sprite.rs` — add field to material
- Modify: `assets/shaders/lit_sprite.wgsl` — add uniform + UV remapping
- Modify: `src/player/mod.rs:73-77` — add default field
- Modify: `src/interaction/block_action.rs:50-54` — add default field

**Step 1: Add `sprite_uv_rect` to LitSpriteMaterial**

In `src/world/lit_sprite.rs`, add a new uniform field:

```rust
#[derive(Asset, AsBindGroup, Clone, TypePath)]
pub struct LitSpriteMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub sprite: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    pub lightmap: Handle<Image>,
    #[uniform(4)]
    pub lightmap_uv_rect: Vec4,
    /// Sprite sheet sub-region: (scale_x, scale_y, offset_x, offset_y).
    /// Default (1,1,0,0) = full texture. For sprite sheets, scale = frame size / sheet size,
    /// offset = frame position in normalized coords.
    #[uniform(5)]
    pub sprite_uv_rect: Vec4,
}
```

**Step 2: Update shader**

In `assets/shaders/lit_sprite.wgsl`, add the new uniform and apply UV remapping:

```wgsl
struct LightmapXform {
    scale: vec2<f32>,
    offset: vec2<f32>,
}

struct SpriteUvRect {
    scale: vec2<f32>,
    offset: vec2<f32>,
}

@group(2) @binding(4) var<uniform> lm_xform: LightmapXform;
@group(2) @binding(5) var<uniform> sprite_rect: SpriteUvRect;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Handle flipped UVs, then apply sprite sheet sub-region
    let base_uv = vec2<f32>(abs(in.uv.x), in.uv.y);
    let uv = base_uv * sprite_rect.scale + sprite_rect.offset;

    let color = textureSample(sprite_texture, sprite_sampler, uv);
    if color.a < 0.01 {
        discard;
    }

    let lightmap_uv = in.world_pos * lm_xform.scale + lm_xform.offset;
    let light = textureSample(lightmap_texture, lightmap_sampler, lightmap_uv).rgb;

    return vec4<f32>(color.rgb * light, color.a);
}
```

**Step 3: Update all LitSpriteMaterial construction sites**

In `src/player/mod.rs:73-77`:
```rust
let material = lit_materials.add(LitSpriteMaterial {
    sprite: animations.idle[0].clone(),
    lightmap: fallback_lm.0.clone(),
    lightmap_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
    sprite_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
});
```

In `src/interaction/block_action.rs:50-54`:
```rust
let material = lit_materials.add(LitSpriteMaterial {
    sprite: sprite_image,
    lightmap: fallback_lm.0.clone(),
    lightmap_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
    sprite_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
});
```

**Step 4: Run tests**

Run: `cargo test`
Expected: 208 tests pass (no behavioral change, just added default field)

**Step 5: Commit**

```bash
git add src/world/lit_sprite.rs assets/shaders/lit_sprite.wgsl src/player/mod.rs src/interaction/block_action.rs
git commit -m "feat(render): add sprite_uv_rect to LitSpriteMaterial for sprite sheet support"
```

---

### Task 2: Add ObjectDef fields + FloorOrWall placement rule

**Files:**
- Modify: `src/object/definition.rs` — add animation + flicker fields, FloorOrWall variant
- Modify: `src/object/placement.rs` — handle FloorOrWall in can_place_object
- Modify: `src/object/plugin.rs` — update hardcoded defs with new fields
- Test: `src/object/placement.rs` (existing tests + new test)

**Step 1: Add fields to ObjectDef and PlacementRule**

In `src/object/definition.rs`:

Add `FloorOrWall` to PlacementRule:
```rust
#[derive(Debug, Clone, Deserialize)]
pub enum PlacementRule {
    Floor,
    Wall,
    Ceiling,
    FloorOrWall,
    Any,
}
```

Add default helper functions:
```rust
fn default_one() -> u32 { 1 }
fn default_zero_f32() -> f32 { 0.0 }
fn default_one_f32() -> f32 { 1.0 }
```

Add fields to ObjectDef (after `drops`):
```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ObjectDef {
    pub id: String,
    pub display_name: String,
    pub size: (u32, u32),
    pub sprite: String,
    #[serde(default = "default_solid_mask")]
    pub solid_mask: Vec<bool>,
    pub placement: PlacementRule,
    #[serde(default = "default_light_emission")]
    pub light_emission: [u8; 3],
    pub object_type: ObjectType,
    #[serde(default)]
    pub drops: Vec<DropDef>,
    // Animation
    #[serde(default = "default_one")]
    pub sprite_columns: u32,
    #[serde(default = "default_one")]
    pub sprite_rows: u32,
    #[serde(default = "default_zero_f32")]
    pub sprite_fps: f32,
    // Flicker (for light sources)
    #[serde(default = "default_zero_f32")]
    pub flicker_speed: f32,
    #[serde(default = "default_zero_f32")]
    pub flicker_strength: f32,
    #[serde(default = "default_one_f32")]
    pub flicker_min: f32,
}
```

**Step 2: Handle FloorOrWall in can_place_object**

In `src/object/placement.rs`, add case in the match:
```rust
PlacementRule::FloorOrWall => {
    let floor_ok = (0..w).all(|dx| world_map.is_solid(anchor_x + dx, anchor_y - 1, ctx));
    let left_solid = world_map.is_solid(anchor_x - 1, anchor_y, ctx);
    let right_solid = world_map.is_solid(anchor_x + w, anchor_y, ctx);
    if !floor_ok && !left_solid && !right_solid {
        return false;
    }
}
```

**Step 3: Update all ObjectDef constructions**

Add the new fields to ALL ObjectDef literals across the codebase. Every ObjectDef needs:
```rust
sprite_columns: 1,
sprite_rows: 1,
sprite_fps: 0.0,
flicker_speed: 0.0,
flicker_strength: 0.0,
flicker_min: 1.0,
```

Files to update:
- `src/object/plugin.rs` — 4 defs (none, torch_object, wooden_chest, wooden_table)
- `src/object/definition.rs` — 3 defs in tests
- `src/object/registry.rs` — 2 defs in tests
- `src/object/placement.rs` — 4 defs in test helper
- `src/world/chunk.rs` — 3 defs in test helper

For the torch_object in plugin.rs, set animation + flicker values:
```rust
ObjectDef {
    id: "torch_object".into(),
    // ... existing fields ...
    sprite_columns: 4,
    sprite_rows: 5,
    sprite_fps: 10.0,
    flicker_speed: 3.0,
    flicker_strength: 0.5,
    flicker_min: 0.5,
},
```

**Step 4: Add FloorOrWall placement test**

In `src/object/placement.rs` tests:
```rust
#[test]
fn floor_or_wall_placement_accepts_floor() {
    let (wc, bm, br, tr, pc, nc) = fixtures::test_world_ctx();
    let ctx = fixtures::make_ctx(&wc, &bm, &br, &tr, &pc, &nc);
    let mut map = WorldMap::default();
    map.get_or_generate_chunk(0, 0, &ctx);

    // Create a registry with a FloorOrWall object
    let obj_reg = ObjectRegistry::from_defs(vec![
        ObjectDef { id: "none".into(), display_name: "None".into(), size: (1, 1), sprite: "".into(), solid_mask: vec![false], placement: PlacementRule::Any, light_emission: [0, 0, 0], object_type: ObjectType::Decoration, drops: vec![], sprite_columns: 1, sprite_rows: 1, sprite_fps: 0.0, flicker_speed: 0.0, flicker_strength: 0.0, flicker_min: 1.0 },
        ObjectDef { id: "torch".into(), display_name: "Torch".into(), size: (1, 1), sprite: "objects/torch.png".into(), solid_mask: vec![false], placement: PlacementRule::FloorOrWall, light_emission: [240, 180, 80], object_type: ObjectType::LightSource, drops: vec![], sprite_columns: 4, sprite_rows: 5, sprite_fps: 10.0, flicker_speed: 3.0, flicker_strength: 0.5, flicker_min: 0.5 },
    ]);
    let torch_id = ObjectId(1);

    let test_y = 5;
    let test_x = 5;
    map.set_tile(test_x, test_y, Layer::Fg, TileId::AIR, &ctx);

    // No floor, no wall → fail
    map.set_tile(test_x, test_y - 1, Layer::Fg, TileId::AIR, &ctx);
    map.set_tile(test_x - 1, test_y, Layer::Fg, TileId::AIR, &ctx);
    map.set_tile(test_x + 1, test_y, Layer::Fg, TileId::AIR, &ctx);
    assert!(!can_place_object(&map, &obj_reg, torch_id, test_x, test_y, &ctx));

    // Floor only → pass
    map.set_tile(test_x, test_y - 1, Layer::Fg, TileId(1), &ctx);
    assert!(can_place_object(&map, &obj_reg, torch_id, test_x, test_y, &ctx));

    // Reset floor, add wall → pass
    map.set_tile(test_x, test_y - 1, Layer::Fg, TileId::AIR, &ctx);
    map.set_tile(test_x - 1, test_y, Layer::Fg, TileId(1), &ctx);
    assert!(can_place_object(&map, &obj_reg, torch_id, test_x, test_y, &ctx));
}
```

**Step 5: Run tests**

Run: `cargo test`
Expected: 209+ tests pass

**Step 6: Commit**

```bash
git add src/object/definition.rs src/object/placement.rs src/object/plugin.rs src/object/registry.rs src/world/chunk.rs
git commit -m "feat(object): add animation/flicker fields to ObjectDef, FloorOrWall placement rule"
```

---

### Task 3: ObjectSpriteMaterials + sprite loading

**Files:**
- Modify: `src/object/plugin.rs` — add sprite loading system, ObjectSpriteMaterials resource
- Modify: `src/object/mod.rs` — re-export new types

**Step 1: Add ObjectSpriteMaterials resource and loading system**

In `src/object/plugin.rs`, add the resource and a loading system:

```rust
use std::collections::HashMap;
use bevy::prelude::*;
use bevy::sprite_render::MeshMaterial2d;

use super::definition::{ObjectDef, ObjectId, ObjectType, PlacementRule};
use super::registry::ObjectRegistry;
use crate::world::lit_sprite::{FallbackLightmap, LitSpriteMaterial, SharedLitQuad};

/// Per-type shared materials and animation state for rendered objects.
#[derive(Resource)]
pub struct ObjectSpriteMaterials {
    pub materials: HashMap<ObjectId, Handle<LitSpriteMaterial>>,
    pub animated: Vec<AnimatedObjectType>,
}

/// Tracks animation state for one object type.
pub struct AnimatedObjectType {
    pub object_id: ObjectId,
    pub material: Handle<LitSpriteMaterial>,
    pub timer: Timer,
    pub current_frame: u32,
    pub total_frames: u32,
    pub columns: u32,
    pub rows: u32,
}
```

Add a system that loads sprite textures and creates materials:

```rust
pub fn load_object_sprites(
    mut commands: Commands,
    object_registry: Res<ObjectRegistry>,
    asset_server: Res<AssetServer>,
    fallback_lm: Res<FallbackLightmap>,
    mut lit_materials: ResMut<Assets<LitSpriteMaterial>>,
) {
    let mut materials = HashMap::new();
    let mut animated = Vec::new();

    for idx in 0..object_registry.len() {
        let id = ObjectId(idx as u16);
        if id == ObjectId::NONE {
            continue;
        }
        let def = object_registry.get(id);
        if def.sprite.is_empty() {
            continue;
        }

        let texture: Handle<Image> = asset_server.load(&def.sprite);
        let total_frames = def.sprite_columns * def.sprite_rows;
        let scale_x = 1.0 / def.sprite_columns as f32;
        let scale_y = 1.0 / def.sprite_rows as f32;

        let material = lit_materials.add(LitSpriteMaterial {
            sprite: texture,
            lightmap: fallback_lm.0.clone(),
            lightmap_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
            sprite_uv_rect: Vec4::new(scale_x, scale_y, 0.0, 0.0),
        });

        materials.insert(id, material.clone());

        if def.sprite_fps > 0.0 && total_frames > 1 {
            animated.push(AnimatedObjectType {
                object_id: id,
                material,
                timer: Timer::from_seconds(1.0 / def.sprite_fps, TimerMode::Repeating),
                current_frame: 0,
                total_frames,
                columns: def.sprite_columns,
                rows: def.sprite_rows,
            });
        }
    }

    commands.insert_resource(ObjectSpriteMaterials { materials, animated });
}
```

**Step 2: Register in plugin**

```rust
use crate::registry::AppState;

impl Plugin for ObjectPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ObjectRegistry::from_defs(vec![/* ... existing defs ... */]));
        app.add_systems(OnEnter(AppState::InGame), load_object_sprites);
    }
}
```

**Step 3: Re-export from mod.rs**

In `src/object/mod.rs`, add re-export:
```rust
pub use plugin::ObjectSpriteMaterials;
```

**Step 4: Run tests**

Run: `cargo test`
Expected: all tests pass (load_object_sprites only runs at InGame enter, not in tests)

**Step 5: Commit**

```bash
git add src/object/plugin.rs src/object/mod.rs
git commit -m "feat(object): add ObjectSpriteMaterials resource and sprite loading system"
```

---

### Task 4: Render objects on spawn

**Files:**
- Modify: `src/object/spawn.rs` — attach Mesh2d + material on entity spawn
- Modify: `src/world/chunk.rs` — pass ObjectSpriteMaterials to spawn_objects_for_chunk
- Modify: `src/interaction/block_action.rs` — attach Mesh2d + material when placing

**Step 1: Update spawn_objects_for_chunk signature and body**

In `src/object/spawn.rs`, add material and quad parameters:

```rust
use bevy::sprite_render::MeshMaterial2d;
use crate::world::lit_sprite::{LitSprite, LitSpriteMaterial, SharedLitQuad};
use super::plugin::ObjectSpriteMaterials;
```

Update `spawn_objects_for_chunk` signature:
```rust
pub fn spawn_objects_for_chunk(
    commands: &mut Commands,
    world_map: &WorldMap,
    object_registry: &ObjectRegistry,
    object_sprites: Option<&ObjectSpriteMaterials>,
    quad: Option<&SharedLitQuad>,
    data_chunk_x: i32,
    chunk_y: i32,
    display_chunk_x: i32,
    tile_size: f32,
    chunk_size: u32,
)
```

In the spawn call, add mesh and material if available:
```rust
let mut entity_commands = commands.spawn((
    PlacedObjectEntity { data_chunk: (data_chunk_x, chunk_y), object_index: idx as u16, object_id: obj.object_id },
    ObjectDisplayChunk { display_chunk: (display_chunk_x, chunk_y) },
    Transform::from_translation(Vec3::new(world_x + offset_x, world_y + offset_y, z))
        .with_scale(Vec3::new(
            def.size.0 as f32 * tile_size,
            def.size.1 as f32 * tile_size,
            1.0,
        )),
    Visibility::default(),
));

if let (Some(sprites), Some(q)) = (object_sprites, quad) {
    if let Some(mat_handle) = sprites.materials.get(&obj.object_id) {
        entity_commands.insert((
            LitSprite,
            Mesh2d(q.0.clone()),
            MeshMaterial2d(mat_handle.clone()),
        ));
    }
}
```

**Step 2: Update chunk_loading_system call site**

In `src/world/chunk.rs`, add parameters to chunk_loading_system:
```rust
object_sprites: Option<Res<ObjectSpriteMaterials>>,
quad: Option<Res<SharedLitQuad>>,
```

Update the spawn call:
```rust
spawn_objects_for_chunk(
    &mut commands,
    &world_map,
    obj_reg,
    object_sprites.as_deref(),
    quad.as_deref(),
    ctx_ref.config.wrap_chunk_x(display_cx),
    cy,
    display_cx,
    ctx_ref.config.tile_size,
    ctx_ref.config.chunk_size,
);
```

Note: `SharedLitQuad` is already available from the `lit_sprite` module. Import it.

**Step 3: Update block_action.rs entity spawn**

In `src/interaction/block_action.rs`, when spawning a newly placed object entity (around line 246-261), also attach mesh and material:

Add parameters to `block_interaction_system`:
```rust
object_sprites: Option<Res<ObjectSpriteMaterials>>,
quad: Res<SharedLitQuad>,
```

Update the entity spawn to include mesh+material:
```rust
let mut entity_cmd = commands.spawn((
    PlacedObjectEntity { ... },
    ObjectDisplayChunk { ... },
    Transform::from_translation(Vec3::new(world_x + offset_x, world_y + offset_y, 0.5))
        .with_scale(Vec3::new(
            def.size.0 as f32 * ctx_ref.config.tile_size,
            def.size.1 as f32 * ctx_ref.config.tile_size,
            1.0,
        )),
    Visibility::default(),
));

if let Some(ref sprites) = object_sprites {
    if let Some(mat_handle) = sprites.materials.get(&obj_id) {
        entity_cmd.insert((
            LitSprite,
            Mesh2d(quad.0.clone()),
            MeshMaterial2d(mat_handle.clone()),
        ));
    }
}
```

**Step 4: Run tests**

Run: `cargo test`
Expected: all tests pass (Optional resources handle test environment gracefully)

**Step 5: Commit**

```bash
git add src/object/spawn.rs src/world/chunk.rs src/interaction/block_action.rs
git commit -m "feat(object): render placed objects with LitSpriteMaterial on spawn"
```

---

### Task 5: Sprite sheet animation system

**Files:**
- Modify: `src/object/plugin.rs` — add animation system
- Test: `src/object/plugin.rs` — unit test for UV computation

**Step 1: Add animation system**

In `src/object/plugin.rs`:

```rust
/// Advance animation frames for all animated object types.
/// Updates the shared material's sprite_uv_rect so all instances animate in sync.
pub fn object_animation_system(
    time: Res<Time>,
    mut object_sprites: ResMut<ObjectSpriteMaterials>,
    mut lit_materials: ResMut<Assets<LitSpriteMaterial>>,
) {
    for anim in &mut object_sprites.animated {
        anim.timer.tick(time.delta());
        if anim.timer.just_finished() {
            anim.current_frame = (anim.current_frame + 1) % anim.total_frames;
            let col = anim.current_frame % anim.columns;
            let row = anim.current_frame / anim.columns;
            let scale_x = 1.0 / anim.columns as f32;
            let scale_y = 1.0 / anim.rows as f32;
            let offset_x = col as f32 * scale_x;
            let offset_y = row as f32 * scale_y;

            if let Some(mat) = lit_materials.get_mut(&anim.material) {
                mat.sprite_uv_rect = Vec4::new(scale_x, scale_y, offset_x, offset_y);
            }
        }
    }
}
```

**Step 2: Register system in plugin**

```rust
app.add_systems(Update, object_animation_system.in_set(GameSet::WorldUpdate));
```

Add import: `use crate::sets::GameSet;`

**Step 3: Add unit test for UV computation**

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn sprite_uv_rect_for_frame() {
        // 4 columns, 5 rows
        let columns = 4u32;
        let rows = 5u32;
        let scale_x = 1.0 / columns as f32; // 0.25
        let scale_y = 1.0 / rows as f32;    // 0.20

        // Frame 0: col=0, row=0 → offset (0.0, 0.0)
        let frame = 0u32;
        let col = frame % columns;
        let row = frame / columns;
        assert_eq!((col, row), (0, 0));
        assert!((col as f32 * scale_x - 0.0).abs() < f32::EPSILON);

        // Frame 3: col=3, row=0 → offset (0.75, 0.0)
        let frame = 3u32;
        let col = frame % columns;
        let row = frame / columns;
        assert_eq!((col, row), (3, 0));
        assert!((col as f32 * scale_x - 0.75).abs() < f32::EPSILON);

        // Frame 4: col=0, row=1 → offset (0.0, 0.2)
        let frame = 4u32;
        let col = frame % columns;
        let row = frame / columns;
        assert_eq!((col, row), (0, 1));
        assert!((row as f32 * scale_y - 0.2).abs() < f32::EPSILON);

        // Frame 19 (last): col=3, row=4 → offset (0.75, 0.8)
        let frame = 19u32;
        let col = frame % columns;
        let row = frame / columns;
        assert_eq!((col, row), (3, 4));
        assert!((col as f32 * scale_x - 0.75).abs() < f32::EPSILON);
        assert!((row as f32 * scale_y - 0.8).abs() < f32::EPSILON);
    }
}
```

**Step 4: Run tests**

Run: `cargo test`
Expected: all tests pass

**Step 5: Commit**

```bash
git add src/object/plugin.rs
git commit -m "feat(object): add sprite sheet animation system for placed objects"
```

---

### Task 6: Object light emission in RC lighting

**Files:**
- Modify: `src/world/rc_lighting.rs` — add object emission to emissive buffer

**Step 1: Add ObjectRegistry parameter to extract_lighting_data**

Add to system parameters (as Optional for backward compat):
```rust
object_registry: Option<Res<ObjectRegistry>>,
```

Add import: `use crate::object::registry::ObjectRegistry;`

**Step 2: Add object emission block after tile emission (around line 472)**

After the existing tile emission block and before the albedo line:

```rust
// Object-specific emissive (placed torches, etc.)
// Check occupancy grid for objects with light_emission.
if let Some(ref obj_reg) = object_registry {
    let wrapped_x = world_config.wrap_tile_x(tx);
    let (cx, cy) = tile_to_chunk(wrapped_x, ty, world_config.chunk_size);
    let (lx, ly) = tile_to_local(wrapped_x, ty, world_config.chunk_size);
    if let Some(chunk) = world_map.chunk(cx, cy) {
        let occ_idx = (ly * world_config.chunk_size + lx) as usize;
        if let Some(occ) = &chunk.occupancy.get(occ_idx).and_then(|o| o.as_ref()) {
            let (dcx, dcy) = occ.data_chunk;
            if let Some(data_chunk) = world_map.chunk(dcx, dcy) {
                if let Some(obj) = data_chunk.objects.get(occ.object_index as usize) {
                    if obj.object_id != ObjectId::NONE {
                        if let Some(def) = obj_reg.try_get(obj.object_id) {
                            let em = def.light_emission;
                            if em != [0, 0, 0] {
                                let flicker = flicker_multiplier(
                                    tx, ty, time.elapsed_secs(),
                                    def.flicker_speed, def.flicker_strength, def.flicker_min,
                                );
                                input.emissive[idx] = [
                                    em[0] as f32 / 255.0 * POINT_LIGHT_BOOST * flicker,
                                    em[1] as f32 / 255.0 * POINT_LIGHT_BOOST * flicker,
                                    em[2] as f32 / 255.0 * POINT_LIGHT_BOOST * flicker,
                                    1.0,
                                ];
                            }
                        }
                    }
                }
            }
        }
    }
}
```

Add import at top of file: `use crate::object::definition::ObjectId;`

**Step 3: Run tests**

Run: `cargo test`
Expected: all tests pass

**Step 4: Commit**

```bash
git add src/world/rc_lighting.rs
git commit -m "feat(lighting): integrate object light emission with RC lighting pipeline"
```

---

### Task 7: Torch migration (tile → object)

**Files:**
- Move: `torch.png` → `assets/objects/torch.png`
- Modify: `assets/world/tiles.registry.ron` — remove torch tile entry
- Modify: `src/item/plugin.rs` — change torch item to placeable_object
- Modify: `src/object/plugin.rs` — update torch_object def (FloorOrWall, drops, animation)
- Modify: `src/test_helpers.rs` — remove torch from test tile registry
- Modify: `src/registry/tile.rs` — fix torch test reference

**Step 1: Move sprite file**

```bash
mkdir -p assets/objects
cp torch.png assets/objects/torch.png
```

**Step 2: Remove torch from tiles.registry.ron**

Remove the torch line from `assets/world/tiles.registry.ron`, leaving 4 tiles (air, grass, dirt, stone).

**Step 3: Update torch ItemDef**

In `src/item/plugin.rs`, change the torch item:
```rust
ItemDef {
    id: "torch".into(),
    display_name: "Torch".into(),
    description: "A simple torch that emits warm light.".into(),
    max_stack: 999,
    rarity: Rarity::Common,
    item_type: ItemType::Placeable,
    icon: "items/torch.png".into(),
    placeable: None,
    placeable_object: Some("torch_object".into()),
    equipment_slot: None,
    stats: None,
},
```

Note: Change `item_type` to `ItemType::Placeable` if that variant exists, otherwise keep `ItemType::Block`.

**Step 4: Update torch ObjectDef**

In `src/object/plugin.rs`, update the torch_object definition:
```rust
ObjectDef {
    id: "torch_object".into(),
    display_name: "Torch".into(),
    size: (1, 1),
    sprite: "objects/torch.png".into(),
    solid_mask: vec![false],
    placement: PlacementRule::FloorOrWall,
    light_emission: [255, 170, 40],
    object_type: ObjectType::LightSource,
    drops: vec![DropDef { item_id: "torch".into(), min: 1, max: 1, chance: 1.0 }],
    sprite_columns: 4,
    sprite_rows: 5,
    sprite_fps: 10.0,
    flicker_speed: 3.0,
    flicker_strength: 0.5,
    flicker_min: 0.5,
},
```

Add import: `use crate::item::DropDef;`

**Step 5: Update test_helpers.rs**

In `src/test_helpers.rs`, remove the torch tile from the test tile registry. The torch was the 5th tile (index 4). Remove it so the test registry has 4 tiles: air, grass, dirt, stone.

Also update any test code that references `TileId(4)` for torch.

In `src/registry/tile.rs` tests, remove/update the torch-specific assertions (line ~250-254):
```rust
// Remove:
// assert_eq!(reg.light_emission(TileId(4)), [255, 170, 40]); // torch
```

**Step 6: Run tests**

Run: `cargo test`
Expected: all tests pass (some test count may decrease if torch-specific tile tests removed)

**Step 7: Commit**

```bash
git add assets/objects/torch.png assets/world/tiles.registry.ron src/item/plugin.rs src/object/plugin.rs src/test_helpers.rs src/registry/tile.rs
git commit -m "feat(torch): migrate torch from tile to placeable object with animation and lighting"
```
