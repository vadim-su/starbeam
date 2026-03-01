# Object Rendering & Torch Migration Design

**Date:** 2026-03-01
**Status:** Approved
**Depends on:** Object Layer (implemented), RC Lighting (implemented)

## Goal

Render placed objects as visible sprites with animation support, integrate object light emission with the RC lighting system, and migrate the torch from a tile to a placeable object.

## Scope

**In scope:**
- Sprite rendering for placed objects via LitSpriteMaterial
- Sprite sheet animation (UV-based, shared material per type)
- Object light emission in RC lighting pipeline
- FloorOrWall placement rule
- Torch tile → object migration
- Flicker parameters on ObjectDef

**Out of scope:**
- RON loading for ObjectRegistry
- Container UI (open chest)
- Cross-chunk multi-tile rendering edge cases
- Per-instance animation phase (random offset)

## Architecture

### 1. Sprite Rendering

Extend `LitSpriteMaterial` with a `sprite_uv_rect: Vec4` uniform (scale_x, scale_y, offset_x, offset_y). Default `(1.0, 1.0, 0.0, 0.0)` = full texture, backward-compatible for player and dropped items.

Update `lit_sprite.wgsl` fragment shader to remap sprite UVs:
```wgsl
let uv = vec2<f32>(abs(in.uv.x), in.uv.y) * sprite_uv_rect.xy + sprite_uv_rect.zw;
```

On object spawn, attach `Mesh2d(shared_quad)` + `MeshMaterial2d<LitSpriteMaterial>` using a per-type shared material. Static objects (chest, table) use default UV rect with their full sprite texture. Animated objects use UV rect pointing to the current animation frame.

### 2. Sprite Sheet Animation

ObjectDef gains optional animation metadata:
```rust
pub sprite_columns: u32,  // default 1
pub sprite_rows: u32,     // default 1
pub sprite_fps: f32,      // default 0.0 = static (no animation)
```

Frame count = `sprite_columns * sprite_rows`. Frame UV size = `(1.0 / columns, 1.0 / rows)`. Frame index maps to column/row: `col = idx % columns`, `row = idx / columns`.

**Shared material per object type.** All instances of the same object type (e.g., all torches) share a single `Handle<LitSpriteMaterial>`. The animation system updates the material's `sprite_uv_rect` offset once per type per frame tick. This means all torches animate in sync — standard for pixel art games (Starbound, Terraria) and very efficient: 1 material update regardless of instance count.

New resource:
```rust
#[derive(Resource)]
pub struct ObjectSpriteMaterials {
    pub materials: HashMap<ObjectId, Handle<LitSpriteMaterial>>,
    pub timers: HashMap<ObjectId, (Timer, u32, u32)>, // (timer, current_frame, total_frames)
}
```

Animation system runs in `Update`, iterates only animated types (fps > 0), advances timer, computes new UV rect, writes to material asset.

### 3. Object Light Emission in RC Lighting

In `build_emissive_input` (rc_lighting.rs), after the existing tile emission block (line ~472), add an object emission block:

1. Check `chunk.occupancy[idx]` for the current tile
2. If occupied, look up the `PlacedObject` via `occ.data_chunk` and `occ.object_index`
3. If `obj.light_emission != [0,0,0]`, write to `input.emissive[idx]` with flicker and `POINT_LIGHT_BOOST`

ObjectDef gains flicker parameters (matching TileDef pattern):
```rust
pub flicker_speed: f32,     // default 0.0
pub flicker_strength: f32,  // default 0.0
pub flicker_min: f32,       // default 1.0
```

The existing `flicker_multiplier()` function is reused as-is.

### 4. PlacementRule: FloorOrWall

Add `FloorOrWall` variant to `PlacementRule` enum. In `can_place_object`, check both conditions — succeed if at least one passes:

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

### 5. Torch Migration

| Component | Before | After |
|-----------|--------|-------|
| tiles.registry.ron | Has "torch" tile entry | Remove "torch" tile |
| ItemDef (torch) | `placeable: Some("torch"), placeable_object: None` | `placeable: None, placeable_object: Some("torch_object")` |
| ObjectDef (torch_object) | `placement: Wall, drops: []` | `placement: FloorOrWall, drops: [DropDef(torch)]` |
| Sprite | N/A (invisible marker) | `assets/objects/torch.png` (128×128, 4×5 grid) |
| Animation | N/A | `columns: 4, rows: 5, fps: 10.0` |
| Light emission | Via tile `light_emission: (255, 170, 40)` | Via object `light_emission: [255, 170, 40]` |
| Flicker | Via tile `flicker_speed: 3.0` etc. | Via object `flicker_speed: 3.0, flicker_strength: 0.5, flicker_min: 0.5` |
| test_helpers.rs | Has torch as 5th tile | Remove torch tile, adjust tests |

### 6. Sprite File

`torch.png` (128×128, RGBA) → `assets/objects/torch.png`
- 4 columns × 5 rows = 20 frames
- Each frame: 32×32 pixels (= 1 tile)
- FPS: 10 (0.1s per frame, full cycle = 2.0s)

### 7. Data Flow

```
ObjectPlugin registers ObjectRegistry (with sprite metadata)
         ↓
ObjectPlugin loads sprite textures, creates ObjectSpriteMaterials
         ↓
spawn_objects_for_chunk → entity gets Mesh2d + MeshMaterial2d (shared per type)
         ↓
object_animation_system → updates sprite_uv_rect in shared materials
         ↓
build_emissive_input → reads object light_emission from occupancy grid
         ↓
RC lighting pipeline → propagates light from object emitters
         ↓
lit_sprite.wgsl → samples sprite_uv_rect sub-region × lightmap
```
