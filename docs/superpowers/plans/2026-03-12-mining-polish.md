# Mining Polish Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add visual mining feedback (crack overlay + particles), tool durability, and pickaxe icons.

**Architecture:** Four independent features wired into the existing mining loop in `block_action.rs`. Crack overlay uses a standalone sprite entity positioned over the damaged tile. Mining particles use the existing `ParticlePool`. Durability adds fields to `HotbarSlot` and `Stack`, decremented on block break. Durability bar is a UI node child of each slot.

**Tech Stack:** Bevy 0.15, Rust, PixelLab MCP for icon generation.

---

## Task 1: Mining Particles

Spawn 2-4 debris particles per damage tick while mining, using `TileDef.albedo` as particle color.

**Files:**
- Modify: `src/interaction/block_action.rs:240-248` (add particle spawn on damage tick)
- Modify: `src/combat/block_damage.rs` (add `particle_timer` to `BlockDamageState`)

- [ ] **Step 1: Add particle_timer to BlockDamageState**

In `src/combat/block_damage.rs`, add a `particle_timer` field:

```rust
#[derive(Debug)]
pub struct BlockDamageState {
    pub accumulated: f32,
    pub regen_timer: f32,
    pub particle_timer: f32,
}
```

Update the `or_insert` in `block_action.rs:243` to include `particle_timer: 0.0`.

- [ ] **Step 2: Add ParticlePool to block_interaction_system**

Add `mut particle_pool: ResMut<ParticlePool>` to the system parameters in `block_action.rs`. Add it into one of the tuple params or as a standalone param.

- [ ] **Step 3: Spawn particles on damage tick**

After `state.accumulated += mining_power * dt` (line 247), add particle spawning:

```rust
state.particle_timer += dt;
if state.particle_timer >= 0.15 {
    state.particle_timer = 0.0;
    let tile_center = Vec2::new(
        tile_x as f32 * ctx_ref.config.tile_size + ctx_ref.config.tile_size / 2.0,
        tile_y as f32 * ctx_ref.config.tile_size + ctx_ref.config.tile_size / 2.0,
    );
    let albedo = ctx_ref.tile_registry.albedo(current);
    let color = [
        albedo[0] as f32 / 255.0,
        albedo[1] as f32 / 255.0,
        albedo[2] as f32 / 255.0,
        1.0,
    ];
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let count = rng.gen_range(2..=4);
    for _ in 0..count {
        let vx = rng.gen_range(-30.0..30.0);
        let vy = rng.gen_range(20.0..60.0);
        particle_pool.spawn(
            tile_center,
            Vec2::new(vx, vy),
            0.4,   // lifetime
            1.5,   // size
            color,
            1.0,   // gravity_scale
            true,  // fade_out
        );
    }
}
```

- [ ] **Step 4: Verify compilation and test in-game**

Run: `cargo check`
Expected: compiles without errors

- [ ] **Step 5: Commit**

```bash
git add src/interaction/block_action.rs src/combat/block_damage.rs
git commit -m "feat(mining): add debris particles while mining"
```

---

## Task 2: Crack Overlay

Spawn a sprite entity showing crack stage (1-4) over damaged tiles. Despawn when damage regens to 0.

**Files:**
- Create: `src/interaction/crack_overlay.rs` (overlay system)
- Modify: `src/interaction/mod.rs` (register system)
- Create: crack sprite atlas (procedural, in code)

- [ ] **Step 1: Create crack_overlay.rs with marker and system**

```rust
use bevy::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::combat::block_damage::BlockDamageMap;
use crate::world::ctx::WorldCtx;

/// Marker for crack overlay entities.
#[derive(Component)]
pub struct CrackOverlay {
    pub tile_x: i32,
    pub tile_y: i32,
}

/// Resource holding the crack stage textures (4 stages).
#[derive(Resource)]
pub struct CrackTextures {
    pub stages: [Handle<Image>; 4],
}

/// Generate a 16x16 crack texture for a given stage (0-3).
fn generate_crack_image(stage: usize) -> Image {
    let size = 16u32;
    let mut data = vec![0u8; (size * size * 4) as usize];

    // Draw crack lines — more lines for higher stages
    let lines: &[(u32, u32, u32, u32)] = match stage {
        0 => &[(7, 3, 9, 7)],
        1 => &[(7, 3, 9, 7), (5, 8, 10, 12)],
        2 => &[(7, 3, 9, 7), (5, 8, 10, 12), (3, 2, 6, 6), (10, 10, 13, 14)],
        _ => &[
            (7, 3, 9, 7), (5, 8, 10, 12), (3, 2, 6, 6),
            (10, 10, 13, 14), (2, 10, 5, 14), (11, 3, 14, 7),
        ],
    };

    for &(x1, y1, x2, y2) in lines {
        let steps = ((x2 as i32 - x1 as i32).abs())
            .max((y2 as i32 - y1 as i32).abs()) as u32;
        for s in 0..=steps {
            let t = if steps == 0 { 0.0 } else { s as f32 / steps as f32 };
            let x = (x1 as f32 + t * (x2 as f32 - x1 as f32)).round() as u32;
            let y = (y1 as f32 + t * (y2 as f32 - y1 as f32)).round() as u32;
            if x < size && y < size {
                let idx = ((y * size + x) * 4) as usize;
                data[idx] = 0;
                data[idx + 1] = 0;
                data[idx + 2] = 0;
                data[idx + 3] = 160; // semi-transparent black
            }
        }
    }

    Image::new(
        Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

pub fn init_crack_textures(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let stages = [
        images.add(generate_crack_image(0)),
        images.add(generate_crack_image(1)),
        images.add(generate_crack_image(2)),
        images.add(generate_crack_image(3)),
    ];
    commands.insert_resource(CrackTextures { stages });
}

/// Sync crack overlay entities with BlockDamageMap.
pub fn update_crack_overlays(
    mut commands: Commands,
    damage_map: Res<BlockDamageMap>,
    ctx: WorldCtx,
    crack_textures: Res<CrackTextures>,
    mut overlays: Query<(Entity, &CrackOverlay, &mut Sprite)>,
) {
    let ctx_ref = ctx.as_ref();
    let tile_size = ctx_ref.config.tile_size;

    // Track which tiles have overlays
    let mut existing: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();

    // Update or despawn existing overlays
    for (entity, overlay, mut sprite) in &mut overlays {
        let key = (overlay.tile_x, overlay.tile_y);
        if let Some(state) = damage_map.damage.get(&key) {
            let tile_def = ctx_ref.tile_registry.get(
                ctx_ref
                    .world_map
                    .get_tile(key.0, key.1, crate::world::chunk::Layer::Fg, &ctx_ref)
                    .unwrap_or(crate::registry::tile::TileId::AIR),
            );
            let ratio = (state.accumulated / tile_def.hardness).clamp(0.0, 1.0);
            let stage = ((ratio * 4.0) as usize).min(3);
            sprite.image = crack_textures.stages[stage].clone();
            existing.insert(key);
        } else {
            commands.entity(entity).despawn();
        }
    }

    // Spawn new overlays for damaged tiles without one
    for (&(tx, ty), state) in &damage_map.damage {
        if existing.contains(&(tx, ty)) {
            continue;
        }
        let tile_id = ctx_ref
            .world_map
            .get_tile(tx, ty, crate::world::chunk::Layer::Fg, &ctx_ref)
            .unwrap_or(crate::registry::tile::TileId::AIR);
        if tile_id == crate::registry::tile::TileId::AIR {
            continue;
        }
        let tile_def = ctx_ref.tile_registry.get(tile_id);
        let ratio = (state.accumulated / tile_def.hardness).clamp(0.0, 1.0);
        let stage = ((ratio * 4.0) as usize).min(3);

        let world_x = tx as f32 * tile_size + tile_size / 2.0;
        let world_y = ty as f32 * tile_size + tile_size / 2.0;

        commands.spawn((
            CrackOverlay { tile_x: tx, tile_y: ty },
            Sprite {
                image: crack_textures.stages[stage].clone(),
                custom_size: Some(Vec2::splat(tile_size)),
                ..default()
            },
            Transform::from_translation(Vec3::new(world_x, world_y, 0.1)),
        ));
    }
}
```

Note: The exact API may need adjustment based on Bevy version — `Sprite` field names, whether `world_map` is accessible from `WorldCtx`. Adapt during implementation.

- [ ] **Step 2: Register in interaction/mod.rs**

Add `pub mod crack_overlay;` and register systems:
- `init_crack_textures` on `OnEnter(AppState::InGame)`
- `update_crack_overlays` in `Update` with `GameSet::WorldUpdate`

- [ ] **Step 3: Verify compilation and test**

Run: `cargo check`

- [ ] **Step 4: Commit**

```bash
git add src/interaction/crack_overlay.rs src/interaction/mod.rs
git commit -m "feat(mining): add crack overlay on damaged blocks"
```

---

## Task 3: Tool Durability Data Model

Add durability tracking to hotbar slots and inventory stacks.

**Files:**
- Modify: `src/inventory/hotbar.rs` (add durability fields to HotbarSlot)
- Modify: `src/inventory/components.rs` (add durability to Stack)
- Modify: `src/item/definition.rs` (add durability to ItemStats)
- Modify: `assets/content/items/stone_pickaxe/stone_pickaxe.item.ron`
- Modify: `assets/content/items/iron_pickaxe/iron_pickaxe.item.ron`
- Modify: `assets/content/items/advanced_pickaxe/advanced_pickaxe.item.ron`

- [ ] **Step 1: Add durability to ItemStats**

In `src/item/definition.rs`, add to `ItemStats`:

```rust
pub durability: Option<u32>,
```

- [ ] **Step 2: Add durability to HotbarSlot**

In `src/inventory/hotbar.rs`:

```rust
#[derive(Clone, Debug, Default)]
pub struct HotbarSlot {
    pub left_hand: Option<String>,
    pub right_hand: Option<String>,
    pub left_durability: Option<u32>,
    pub right_durability: Option<u32>,
}
```

Add helper methods:

```rust
impl HotbarSlot {
    /// Get durability for a hand. None = no durability tracking (not a tool).
    pub fn durability(&self, is_left: bool) -> Option<u32> {
        if is_left { self.left_durability } else { self.right_durability }
    }

    /// Set durability for a hand.
    pub fn set_durability(&mut self, is_left: bool, val: Option<u32>) {
        if is_left { self.left_durability = val; } else { self.right_durability = val; }
    }
}
```

- [ ] **Step 3: Add durability to Stack**

In `src/inventory/components.rs`:

```rust
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stack {
    pub item_id: String,
    pub count: u16,
    #[serde(default)]
    pub durability: Option<u32>,
}
```

- [ ] **Step 4: Update .item.ron files**

Add `durability: Some(100)` to stone_pickaxe stats, `Some(200)` to iron_pickaxe, `Some(400)` to advanced_pickaxe.

- [ ] **Step 5: Verify compilation**

Run: `cargo check`
Fix any issues from the new fields (Default derives, etc.)

- [ ] **Step 6: Commit**

```bash
git add src/item/definition.rs src/inventory/hotbar.rs src/inventory/components.rs assets/content/items/
git commit -m "feat(durability): add durability fields to items, hotbar, and inventory"
```

---

## Task 4: Tool Durability Mechanics

Decrement durability on block break, remove tool when it reaches 0.

**Files:**
- Modify: `src/interaction/block_action.rs:250-268` (decrement after block destroyed)

- [ ] **Step 1: Initialize durability when equipping tool**

Find where items are assigned to hotbar slots (likely in drag_drop or an equip system). When a tool with `stats.durability` is placed in a slot, set `left_durability` or `right_durability` to `stats.durability`.

- [ ] **Step 2: Decrement durability on block break**

In `block_action.rs`, after the block is destroyed (line 250-268), before `spawn_tile_drops`:

```rust
// Decrement tool durability
let active = hotbar.active_slot;
let slot = &mut hotbar.slots[active];
if let Some(ref mut dur) = slot.left_durability {
    *dur = dur.saturating_sub(1);
    if *dur == 0 {
        slot.left_hand = None;
        slot.left_durability = None;
    }
}
```

Note: `hotbar` is currently borrowed immutably from the query. Need to change the query to `&mut Hotbar` or split the borrow.

- [ ] **Step 3: Change player_query to allow mutable Hotbar**

Update the query signature from:
```rust
Query<(&Transform, &Hotbar, &mut Inventory), With<Player>>
```
to:
```rust
Query<(&Transform, &mut Hotbar, &mut Inventory), With<Player>>
```

And destructure as `(player_tf, mut hotbar, mut inventory)`.

- [ ] **Step 4: Verify compilation and test**

Run: `cargo check`

- [ ] **Step 5: Commit**

```bash
git add src/interaction/block_action.rs
git commit -m "feat(durability): decrement tool durability on block break, remove at 0"
```

---

## Task 5: Durability Bar UI

Render a thin colored bar at the bottom of item icons for tools with durability.

**Files:**
- Modify: `src/ui/game_ui/mod.rs:237-280` (add durability bar node to `spawn_slot_icon_children`)
- Modify: `src/ui/game_ui/slot_sync.rs` (update durability bar color/width)
- Modify: `src/ui/game_ui/components.rs` (add DurabilityBar marker)

- [ ] **Step 1: Add DurabilityBar marker component**

In `src/ui/game_ui/components.rs`:

```rust
#[derive(Component)]
pub struct DurabilityBar;
```

- [ ] **Step 2: Add durability bar node to spawn_slot_icon_children**

In `src/ui/game_ui/mod.rs`, inside `spawn_slot_icon_children`, after the count text node, add:

```rust
// Durability bar
parent.spawn((
    DurabilityBar,
    Node {
        position_type: PositionType::Absolute,
        bottom: Val::Px(1.0),
        left: Val::Px(1.0),
        width: Val::Percent(0.0), // updated dynamically
        height: Val::Px(2.0),
        ..default()
    },
    BackgroundColor(Color::srgb(0.0, 1.0, 0.0)),
    Visibility::Hidden,
    Pickable::IGNORE,
));
```

- [ ] **Step 3: Update durability bar in slot_sync**

In `src/ui/game_ui/slot_sync.rs`, in `update_slot_icons`, when iterating children, add a branch for `DurabilityBar`:

```rust
// Add DurabilityBar to the query imports
// In the children loop, after count update:
if let Ok((mut bar_node, mut bar_bg, mut bar_vis)) = durability_bar_query.get_mut(child) {
    if let Some((current, max)) = durability_info {
        if current < max {
            let ratio = current as f32 / max as f32;
            bar_node.width = Val::Percent(ratio * 90.0); // 90% max to leave margins
            bar_bg.0 = if ratio > 0.5 {
                Color::srgb(0.0, 1.0, 0.0)
            } else if ratio > 0.25 {
                Color::srgb(1.0, 1.0, 0.0)
            } else {
                Color::srgb(1.0, 0.0, 0.0)
            };
            *bar_vis = Visibility::Inherited;
        } else {
            *bar_vis = Visibility::Hidden;
        }
    } else {
        *bar_vis = Visibility::Hidden;
    }
}
```

The `durability_info` is resolved from:
- Hotbar slots: `(slot.left_durability, item_stats.durability)`
- Inventory bags: `(stack.durability, item_stats.durability)`

- [ ] **Step 4: Verify compilation and test**

Run: `cargo check`

- [ ] **Step 5: Commit**

```bash
git add src/ui/game_ui/components.rs src/ui/game_ui/mod.rs src/ui/game_ui/slot_sync.rs
git commit -m "feat(ui): add durability bar on tool icons in hotbar and inventory"
```

---

## Task 6: Pickaxe Icons (PixelLab)

Generate 16x16 pixel art icons for all three pickaxe tiers via PixelLab.

**Files:**
- Create: `assets/content/items/stone_pickaxe/icon.png`
- Create: `assets/content/items/iron_pickaxe/icon.png`
- Create: `assets/content/items/advanced_pickaxe/icon.png`
- Modify: `assets/content/items/*/pickaxe.item.ron` (add `icon` field)

- [ ] **Step 1: Generate stone pickaxe icon**

Use PixelLab `create_map_object` or equivalent tool:
- Style: 16x16 pixel art item icon
- Description: "A stone pickaxe with grey stone head and brown wooden handle, pixel art item icon"
- Save to `assets/content/items/stone_pickaxe/icon.png`

- [ ] **Step 2: Generate iron pickaxe icon**

- Description: "An iron pickaxe with dark grey metallic head and brown wooden handle, pixel art item icon"
- Save to `assets/content/items/iron_pickaxe/icon.png`

- [ ] **Step 3: Generate advanced pickaxe icon**

- Description: "An advanced crystal pickaxe with glowing blue-purple crystal head and silver metallic handle, pixel art item icon"
- Save to `assets/content/items/advanced_pickaxe/icon.png`

- [ ] **Step 4: Add icon paths to .item.ron files**

Add `icon: Some("content/items/stone_pickaxe/icon.png")` (etc.) to each .item.ron file.

- [ ] **Step 5: Verify icons load in-game**

Run: `cargo run` and check hotbar icons display correctly.

- [ ] **Step 6: Commit**

```bash
git add assets/content/items/
git commit -m "art: add pickaxe icons for all three tiers via PixelLab"
```
