# Modular Character System — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Refactor the player from a single-entity sprite to a 4-part modular character (head, body, front_arm, back_arm) with synchronized animations and independent z-ordering.

**Architecture:** Player entity becomes a parent with physics/inventory/markers only. Four child entities (one per body part) each have their own `LitSpriteMaterial`, `Mesh2d`, and part-specific animation state. All parts share the same animation frame index and facing direction, synced by the parent's `AnimationState`. The existing frame-per-image loading approach is preserved (no spritesheet migration yet).

**Tech Stack:** Bevy (Rust), `LitSpriteMaterial` (custom Material2d), RON configs, existing physics/animation pipeline.

---

## Context for the implementor

### Current state
- Player is a **single entity** with one `LitSpriteMaterial` that swaps sprite textures per frame
- Animation frames are individual PNG files loaded via `CharacterAnimations` resource
- Facing is controlled via `Transform.scale.x` (negative = flip)
- Submerge tint updates the material directly on the `Player` entity
- Key files: `src/player/mod.rs`, `src/player/animation.rs`, `src/player/movement.rs`, `src/registry/player.rs`, `src/registry/assets.rs`, `src/registry/loading.rs`

### Target state
- Player entity has **4 child entities** (body parts), each with its own material
- All children sync animation frames from parent's `AnimationState`
- Each child has z-offset for layering: back_arm(-0.02), body(-0.01), head(0.0), front_arm(+0.01)
- Submerge tint propagates to all children
- For MVP: use existing full-body sprites on "body" part; head/arms use transparent placeholders
- Arm aiming (rotation toward cursor) is **deferred** to a future task

### Key invariants
- `LitSpriteMaterial` per child (not shared) — each part needs its own sprite texture
- `SharedLitQuad` mesh IS shared across all parts
- Physics components (`Velocity`, `Gravity`, `Grounded`, `TileCollider`) stay on parent
- `Transform.scale` on children controls part sprite size; parent `Transform.scale` should be `(1, 1, 1)`
- Facing flip: set `scale.x` on each child (not parent, to avoid double-scaling physics)

---

### Task 1: Define `CharacterPart` component and `PartType` enum

**Files:**
- Create: `src/player/parts.rs`
- Modify: `src/player/mod.rs:1` (add `pub mod parts;`)

**Step 1: Create the parts module**

```rust
// src/player/parts.rs
use bevy::prelude::*;

/// Which body part this child entity represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PartType {
    BackArm,
    Body,
    Head,
    FrontArm,
}

impl PartType {
    /// Z-offset for render ordering. Higher = in front.
    pub fn z_offset(self) -> f32 {
        match self {
            PartType::BackArm => -0.02,
            PartType::Body => -0.01,
            PartType::Head => 0.0,
            PartType::FrontArm => 0.01,
        }
    }

    /// All part types in spawn order.
    pub const ALL: [PartType; 4] = [
        PartType::BackArm,
        PartType::Body,
        PartType::Head,
        PartType::FrontArm,
    ];
}

/// Marker component on each body-part child entity.
#[derive(Component)]
pub struct CharacterPart(pub PartType);
```

**Step 2: Register the module**

In `src/player/mod.rs` line 1, add:
```rust
pub mod parts;
```

**Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles with no errors (new module is unused but that's fine)

**Step 4: Commit**

```bash
git add src/player/parts.rs src/player/mod.rs
git commit -m "feat(player): add CharacterPart component and PartType enum"
```

---

### Task 2: Update RON config and asset structs to support per-part sprites

**Files:**
- Modify: `src/registry/assets.rs:40-61` (add `parts` field to `CharacterDefAsset`)
- Modify: `src/registry/loading.rs:72-76` (add parts to `CharacterAnimConfig`)
- Modify: `assets/content/characters/adventurer/adventurer.character.ron`

**Step 1: Add part config structs to `assets.rs`**

Add after `AnimationDef` (after line 68):
```rust
/// Per-part sprite configuration within a character.
#[derive(Debug, Clone, Deserialize)]
pub struct PartDef {
    /// Relative path to the sprite directory (within the character folder).
    /// Contains animation subdirectories matching the character's animation names.
    pub sprite_dir: String,
    /// Pixel size of each frame for this part.
    pub frame_size: (u32, u32),
    /// Pixel offset from the parent entity's origin.
    #[serde(default)]
    pub offset: (f32, f32),
}

/// All body parts for a modular character.
#[derive(Debug, Clone, Deserialize)]
pub struct CharacterPartsDef {
    pub body: PartDef,
    #[serde(default)]
    pub head: Option<PartDef>,
    #[serde(default)]
    pub front_arm: Option<PartDef>,
    #[serde(default)]
    pub back_arm: Option<PartDef>,
}
```

Add `parts` field to `CharacterDefAsset` (optional, for backwards compat):
```rust
// Inside CharacterDefAsset struct, after `animations` field:
    #[serde(default)]
    pub parts: Option<CharacterPartsDef>,
```

**Step 2: Add parts to `CharacterAnimConfig`**

In `src/registry/loading.rs`, add to the `CharacterAnimConfig` struct:
```rust
pub struct CharacterAnimConfig {
    pub sprite_size: (u32, u32),
    pub animations: std::collections::HashMap<String, AnimationDef>,
    pub base_path: String,
    pub parts: Option<CharacterPartsDef>,  // <-- add this
}
```

Update the `commands.insert_resource(CharacterAnimConfig { ... })` call (~line 342) to include:
```rust
    parts: character.parts.clone(),
```

**Step 3: Update the RON config file**

Add a `parts` section to `adventurer.character.ron`. For MVP, body uses existing sprites; head/arms are optional (None):
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
  swim_impulse: 180.0,
  swim_gravity_factor: 0.3,
  swim_drag: 0.15,

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

  parts: Some((
    body: (
      sprite_dir: "sprites",
      frame_size: (44, 44),
      offset: (0.0, 0.0),
    ),
  )),
)
```

**Step 4: Update the RON roundtrip test**

In `src/registry/assets.rs`, update `ron_roundtrip_character` test to also check parts:
```rust
    assert!(asset.parts.is_some());
    let parts = asset.parts.as_ref().unwrap();
    assert_eq!(parts.body.frame_size, (44, 44));
```

**Step 5: Verify it compiles and test passes**

Run: `cargo test ron_roundtrip_character`
Expected: PASS

**Step 6: Commit**

```bash
git add src/registry/assets.rs src/registry/loading.rs assets/content/characters/adventurer/adventurer.character.ron
git commit -m "feat(registry): add per-part sprite config to character RON"
```

---

### Task 3: Load per-part animation frames in `CharacterAnimations`

**Files:**
- Modify: `src/player/animation.rs:12-18` (restructure `CharacterAnimations`)
- Modify: `src/player/animation.rs:39-65` (`load_character_animations`)

**Step 1: Restructure `CharacterAnimations` to support parts**

Replace the `CharacterAnimations` struct with a part-aware version:

```rust
use crate::player::parts::PartType;
use std::collections::HashMap;

/// Animation frames for a single body part.
#[derive(Debug, Default)]
pub struct PartAnimFrames {
    pub idle: Vec<Handle<Image>>,
    pub running: Vec<Handle<Image>>,
    pub jumping: Vec<Handle<Image>>,
}

/// Loaded animation frame handles for all body parts.
#[derive(Resource)]
pub struct CharacterAnimations {
    /// Frames per part type. Body is always present; others may be absent.
    pub parts: HashMap<PartType, PartAnimFrames>,
}

impl CharacterAnimations {
    /// Get frames for a specific part, falling back to empty slices.
    pub fn frames_for(&self, part: PartType, kind: AnimationKind) -> &[Handle<Image>] {
        self.parts
            .get(&part)
            .map(|p| match kind {
                AnimationKind::Idle => p.idle.as_slice(),
                AnimationKind::Running => p.running.as_slice(),
                AnimationKind::Jumping | AnimationKind::Swimming => p.jumping.as_slice(),
            })
            .unwrap_or(&[])
    }
}
```

**Step 2: Update `load_character_animations`**

Rewrite to load frames per part:

```rust
pub fn load_character_animations(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    anim_config: Res<CharacterAnimConfig>,
) {
    let base = &anim_config.base_path;
    let mut parts_map = HashMap::new();

    // Helper: load animation frames from a sprite directory
    let load_part = |sprite_dir: &str| -> PartAnimFrames {
        let load_anim = |anim_name: &str| -> Vec<Handle<Image>> {
            anim_config
                .animations
                .get(anim_name)
                .map(|def| {
                    def.frames
                        .iter()
                        .map(|frame| {
                            // Replace "sprites/" prefix with the part's sprite_dir
                            let part_frame = frame.replacen("sprites/", &format!("{}/", sprite_dir), 1);
                            asset_server.load(format!("{base}{part_frame}"))
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        PartAnimFrames {
            idle: load_anim("staying"),
            running: load_anim("running"),
            jumping: load_anim("jumping"),
        }
    };

    if let Some(ref parts_def) = anim_config.parts {
        // Modular mode: load per-part
        parts_map.insert(PartType::Body, load_part(&parts_def.body.sprite_dir));
        if let Some(ref head) = parts_def.head {
            parts_map.insert(PartType::Head, load_part(&head.sprite_dir));
        }
        if let Some(ref front_arm) = parts_def.front_arm {
            parts_map.insert(PartType::FrontArm, load_part(&front_arm.sprite_dir));
        }
        if let Some(ref back_arm) = parts_def.back_arm {
            parts_map.insert(PartType::BackArm, load_part(&back_arm.sprite_dir));
        }
    } else {
        // Legacy mode: single-body sprites (backwards compat)
        let load_frames = |anim_name: &str| -> Vec<Handle<Image>> {
            anim_config
                .animations
                .get(anim_name)
                .map(|def| {
                    def.frames
                        .iter()
                        .map(|frame| asset_server.load(format!("{base}{frame}")))
                        .collect()
                })
                .unwrap_or_default()
        };
        parts_map.insert(PartType::Body, PartAnimFrames {
            idle: load_frames("staying"),
            running: load_frames("running"),
            jumping: load_frames("jumping"),
        });
    }

    commands.insert_resource(CharacterAnimations { parts: parts_map });
}
```

**Step 3: Update `frames_for_kind` and callers**

Remove the standalone `frames_for_kind` function (line 173-180). Update `animate_player` to use `animations.frames_for(PartType::Body, anim.kind)` instead. This is a temporary bridge — Task 5 will rewrite animation fully.

For now, replace every call to `frames_for_kind(&animations, ...)` with:
```rust
animations.frames_for(PartType::Body, anim.kind)
```

And delete the `frames_for_kind` function.

**Step 4: Verify it compiles and runs**

Run: `cargo check`
Then: `cargo run` — verify the player looks and animates identically to before.

**Step 5: Commit**

```bash
git add src/player/animation.rs
git commit -m "refactor(animation): restructure CharacterAnimations for per-part frames"
```

---

### Task 4: Refactor `spawn_player` to create child entities per part

**Files:**
- Modify: `src/player/mod.rs:54-132` (`spawn_player` function)

This is the core structural change. The player entity becomes a parent with physics/inventory only. Each body part becomes a child entity with its own material.

**Step 1: Refactor `spawn_player`**

```rust
use crate::player::parts::{CharacterPart, PartType};

fn spawn_player(
    mut commands: Commands,
    player_config: Res<PlayerConfig>,
    world_config: Res<ActiveWorld>,
    planet_config: Res<PlanetConfig>,
    noise_cache: Res<TerrainNoiseCache>,
    animations: Res<CharacterAnimations>,
    anim_config: Res<CharacterAnimConfig>,
    quad: Option<Res<SharedLitQuad>>,
    fallback_lm: Res<FallbackLightmap>,
    mut lit_materials: ResMut<Assets<LitSpriteMaterial>>,
    existing_player: Query<Entity, With<Player>>,
) {
    if existing_player.iter().next().is_some() {
        return;
    }

    let Some(quad) = quad else {
        warn!("SharedLitQuad not ready yet, deferring player spawn");
        return;
    };

    let spawn_tile_x = 0;
    let surface_y = terrain_gen::surface_height(
        &noise_cache,
        spawn_tile_x,
        &world_config,
        planet_config.layers.surface.terrain_frequency,
        planet_config.layers.surface.terrain_amplitude,
    );
    let spawn_pixel_x = spawn_tile_x as f32 * world_config.tile_size + world_config.tile_size / 2.0;
    let spawn_pixel_y =
        (surface_y + 5) as f32 * world_config.tile_size + player_config.height / 2.0;

    // Determine which parts to spawn
    let parts_to_spawn: Vec<PartType> = if anim_config.parts.is_some() {
        // Only spawn parts that have loaded animation frames
        PartType::ALL
            .iter()
            .copied()
            .filter(|pt| animations.parts.contains_key(pt))
            .collect()
    } else {
        vec![PartType::Body] // Legacy: body-only
    };

    // Spawn parent entity (physics + inventory, no rendering)
    let mut parent = commands.spawn((
        Player,
        {
            let mut inv = Inventory::new();
            inv.try_add_item("torch", 10, 999, crate::inventory::BagTarget::Main);
            inv.try_add_item("workbench", 1, 10, crate::inventory::BagTarget::Main);
            inv
        },
        Hotbar::new(),
        HandCraftState::default(),
        UnlockedRecipes::default(),
        Velocity::default(),
        Gravity(player_config.gravity),
        Grounded(false),
        Submerged::default(),
        TileCollider {
            width: player_config.width,
            height: player_config.height,
        },
        AnimationState {
            kind: AnimationKind::Idle,
            frame: 0,
            timer: Timer::from_seconds(0.15, TimerMode::Repeating),
            facing_right: true,
        },
        Transform::from_xyz(spawn_pixel_x, spawn_pixel_y, 1.0),
    ));

    // Spawn child entities for each body part
    parent.with_children(|builder| {
        for &part_type in &parts_to_spawn {
            let frames = animations.frames_for(part_type, AnimationKind::Idle);
            let sprite_handle = if !frames.is_empty() {
                frames[0].clone()
            } else {
                fallback_lm.0.clone() // transparent fallback
            };

            // Determine frame size for this part
            let (fw, fh) = if let Some(ref parts_def) = anim_config.parts {
                match part_type {
                    PartType::Body => parts_def.body.frame_size,
                    PartType::Head => parts_def.head.as_ref().map(|p| p.frame_size).unwrap_or(anim_config.sprite_size),
                    PartType::FrontArm => parts_def.front_arm.as_ref().map(|p| p.frame_size).unwrap_or(anim_config.sprite_size),
                    PartType::BackArm => parts_def.back_arm.as_ref().map(|p| p.frame_size).unwrap_or(anim_config.sprite_size),
                }
            } else {
                anim_config.sprite_size
            };

            // Determine offset for this part
            let (ox, oy) = if let Some(ref parts_def) = anim_config.parts {
                match part_type {
                    PartType::Body => parts_def.body.offset,
                    PartType::Head => parts_def.head.as_ref().map(|p| p.offset).unwrap_or((0.0, 0.0)),
                    PartType::FrontArm => parts_def.front_arm.as_ref().map(|p| p.offset).unwrap_or((0.0, 0.0)),
                    PartType::BackArm => parts_def.back_arm.as_ref().map(|p| p.offset).unwrap_or((0.0, 0.0)),
                }
            } else {
                (0.0, 0.0)
            };

            let material = lit_materials.add(LitSpriteMaterial {
                sprite: sprite_handle,
                lightmap: fallback_lm.0.clone(),
                lightmap_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
                sprite_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
                submerge_tint: Vec4::ZERO,
                highlight: Vec4::ZERO,
            });

            builder.spawn((
                CharacterPart(part_type),
                LitSprite,
                Mesh2d(quad.0.clone()),
                MeshMaterial2d(material),
                Transform::from_xyz(ox, oy, part_type.z_offset())
                    .with_scale(Vec3::new(fw as f32, fh as f32, 1.0)),
            ));
        }
    });
}
```

**Important:** Remove `LitSprite` from the parent entity — only children render now.

**Step 2: Verify it compiles**

Run: `cargo check`

**Step 3: Run the game and verify the player still renders**

Run: `cargo run`
Expected: Player appears with body sprite visible (head/arms not present in MVP config). Should look identical to before since body uses the same sprites at the same size.

**Step 4: Commit**

```bash
git add src/player/mod.rs
git commit -m "refactor(player): spawn body parts as child entities"
```

---

### Task 5: Rewrite animation system to sync child part sprites

**Files:**
- Modify: `src/player/animation.rs:72-171` (`animate_player`)

The animation system now needs to:
1. Determine animation state on the parent (unchanged logic)
2. Update sprite textures on ALL child `CharacterPart` entities

**Step 1: Rewrite `animate_player`**

```rust
pub fn animate_player(
    time: Res<Time>,
    animations: Res<CharacterAnimations>,
    player_config: Res<PlayerConfig>,
    mut materials: ResMut<Assets<LitSpriteMaterial>>,
    mut player_query: Query<
        (
            &mut AnimationState,
            &Velocity,
            &Grounded,
            Option<&Submerged>,
            &Children,
        ),
        With<Player>,
    >,
    mut part_query: Query<(
        &CharacterPart,
        &MeshMaterial2d<LitSpriteMaterial>,
        &mut Transform,
    )>,
) {
    for (mut anim, velocity, grounded, submerged, children) in &mut player_query {
        // --- Determine animation kind (unchanged logic) ---
        let is_swimming = submerged.is_some_and(|s| s.is_swimming());
        let new_kind = if is_swimming {
            AnimationKind::Swimming
        } else if !grounded.0 {
            AnimationKind::Jumping
        } else if velocity.x.abs() > VELOCITY_DEADZONE {
            AnimationKind::Running
        } else {
            AnimationKind::Idle
        };

        // Reset frame on state change
        let kind_changed = new_kind != anim.kind;
        if kind_changed {
            anim.kind = new_kind;
            anim.frame = 0;
            anim.timer.reset();
        }

        // Update facing direction
        if velocity.x > VELOCITY_DEADZONE {
            anim.facing_right = true;
        }
        if velocity.x < -VELOCITY_DEADZONE {
            anim.facing_right = false;
        }

        // Frame advancement
        let mut new_frame = anim.frame;
        match anim.kind {
            AnimationKind::Jumping | AnimationKind::Swimming => {
                // Velocity-based frame selection using body's jump frames as reference
                let body_frames = animations.frames_for(PartType::Body, anim.kind);
                if !body_frames.is_empty() {
                    let half = body_frames.len() / 2;
                    let jump_vel = player_config.jump_velocity;
                    new_frame = if velocity.y > 0.0 {
                        let t = 1.0 - (velocity.y / jump_vel).clamp(0.0, 1.0);
                        (t * half as f32) as usize
                    } else {
                        let t = (-velocity.y / jump_vel).clamp(0.0, 1.0);
                        half + (t * (body_frames.len() - 1 - half) as f32) as usize
                    };
                    new_frame = new_frame.min(body_frames.len() - 1);
                }
            }
            _ => {
                // Timer-based cycling
                anim.timer.tick(time.delta());
                if anim.timer.just_finished() {
                    let body_frames = animations.frames_for(PartType::Body, anim.kind);
                    if !body_frames.is_empty() {
                        new_frame = (anim.frame + 1) % body_frames.len();
                    }
                }
            }
        }

        let frame_changed = new_frame != anim.frame || kind_changed;
        anim.frame = new_frame;

        // Update all child part sprites
        if frame_changed {
            for &child in children.iter() {
                let Ok((part, mat_handle, _)) = part_query.get(child) else {
                    continue;
                };
                let frames = animations.frames_for(part.0, anim.kind);
                if !frames.is_empty() {
                    let idx = anim.frame.min(frames.len() - 1);
                    if let Some(mat) = materials.get_mut(&mat_handle.0) {
                        mat.sprite = frames[idx].clone();
                    }
                }
            }
        }

        // Update facing on all children
        for &child in children.iter() {
            let Ok((_, _, mut transform)) = part_query.get_mut(child) else {
                continue;
            };
            let abs_scale_x = transform.scale.x.abs();
            transform.scale.x = if anim.facing_right {
                abs_scale_x
            } else {
                -abs_scale_x
            };
        }
    }
}
```

**Step 2: Add missing imports at top of `animation.rs`**

```rust
use crate::player::parts::{CharacterPart, PartType};
```

Remove the `PLAYER_SPRITE_SIZE` import since it's no longer needed in animation.

**Step 3: Verify it compiles and runs**

Run: `cargo check`
Then: `cargo run` — player should animate exactly as before (running, jumping, idle, swimming).

**Step 4: Commit**

```bash
git add src/player/animation.rs
git commit -m "refactor(animation): sync frame updates across all character parts"
```

---

### Task 6: Update `update_submerge_tint` to propagate to children

**Files:**
- Modify: `src/player/mod.rs:182-209` (`update_submerge_tint`)

The submerge tint currently queries `Player` entities with `MeshMaterial2d`. Now `Player` no longer has a material — children do.

**Step 1: Rewrite `update_submerge_tint`**

```rust
fn update_submerge_tint(
    liquid_registry: Res<LiquidRegistry>,
    mut materials: ResMut<Assets<LitSpriteMaterial>>,
    player_query: Query<(&Submerged, &Children), With<Player>>,
    part_query: Query<&MeshMaterial2d<LitSpriteMaterial>, With<CharacterPart>>,
) {
    for (sub, children) in &player_query {
        let tint = if sub.ratio < 0.01 || sub.liquid_id.is_none() {
            Vec4::ZERO
        } else {
            let color = liquid_registry
                .get(sub.liquid_id)
                .map(|d| d.color)
                .unwrap_or([0.0; 4]);
            let max_c = color[0].max(color[1]).max(color[2]).max(0.01);
            let tint_r = color[0] / max_c;
            let tint_g = color[1] / max_c;
            let tint_b = color[2] / max_c;
            let strength = (sub.ratio * color[3]).min(0.5);
            Vec4::new(tint_r, tint_g, tint_b, strength)
        };

        for &child in children.iter() {
            let Ok(mat_handle) = part_query.get(child) else {
                continue;
            };
            if let Some(mat) = materials.get_mut(&mat_handle.0) {
                mat.submerge_tint = tint;
            }
        }
    }
}
```

Add import at top of `mod.rs`:
```rust
use crate::player::parts::CharacterPart;
```

**Step 2: Verify it compiles**

Run: `cargo check`

**Step 3: Test in-game**

Run: `cargo run` — walk the player into water, verify submersion tint still applies to the character sprite.

**Step 4: Commit**

```bash
git add src/player/mod.rs
git commit -m "fix(player): propagate submerge tint to child part entities"
```

---

### Task 7: Fix any remaining systems that query `Player` with `MeshMaterial2d`

**Files:**
- Check: `src/interaction/interactable.rs` (highlight system — only affects `CraftingStation`, not `Player`)
- Check: `src/world/` (lightmap update system)
- Check: `src/registry/loading.rs` (lightmap binding)

**Step 1: Search for all queries referencing `Player` + `MeshMaterial2d` or `LitSprite`**

Run: `grep -rn "With<Player>" src/ | grep -i "material\|lit_sprite\|MeshMaterial"` and fix any broken queries.

The key ones to check:
- Lightmap update system (likely in `src/world/`) — needs to update lightmap on children, not parent
- Any system that reads `LitSprite` marker — children now have it, parent does not

**Step 2: Fix lightmap update**

Find the system that sets `lightmap` and `lightmap_uv_rect` on `LitSpriteMaterial` and ensure it queries `LitSprite` (which is on children). It likely already works because it queries `LitSprite` entities generically, not specifically `Player`.

Verify by reading the lightmap update system and confirming it uses `With<LitSprite>` (not `With<Player>`).

**Step 3: Verify it compiles and runs end-to-end**

Run: `cargo run`
Expected: Player renders, animates, submerge tint works, lightmap applies correctly.

**Step 4: Commit (if any fixes were needed)**

```bash
git add -A
git commit -m "fix(player): update remaining systems for child-entity architecture"
```

---

### Task 8: Clean up dead code and remove `PLAYER_SPRITE_SIZE` from parent

**Files:**
- Modify: `src/player/mod.rs` (remove `PLAYER_SPRITE_SIZE` constant if no longer used by parent scale)
- Check: `src/player/animation.rs` (should no longer reference `PLAYER_SPRITE_SIZE`)

**Step 1: Audit usage of `PLAYER_SPRITE_SIZE`**

After the refactor, the parent entity has `Transform::from_xyz(...).with_scale(Vec3::ONE)` (or no explicit scale). The sprite size is now set per-child from `frame_size` in the config. Check if `PLAYER_SPRITE_SIZE` is still referenced anywhere.

If it's only used in `spawn_player` (which now uses `fw`/`fh` from config), remove the constant.

If `animation.rs` still uses it for anything, remove that usage too (flip now uses `transform.scale.x.abs()`).

**Step 2: Verify it compiles**

Run: `cargo check`

**Step 3: Commit**

```bash
git add src/player/mod.rs src/player/animation.rs
git commit -m "chore(player): remove PLAYER_SPRITE_SIZE constant, use config-driven sizes"
```

---

### Task 9: Smoke-test full gameplay loop

**No files to modify. Manual testing only.**

**Step 1:** Run `cargo run` and verify:
- [ ] Player spawns and is visible
- [ ] Idle animation works
- [ ] Running left/right works (sprite flips correctly)
- [ ] Jumping animation works (velocity-based frame selection)
- [ ] Swimming/submersion tint works
- [ ] Lightmap/lighting affects the player sprite
- [ ] Picking up items still works (inventory on parent entity)
- [ ] Interacting with crafting stations works
- [ ] Camera follows the player correctly
- [ ] Warping/respawning works (player teleports correctly)

**Step 2:** If any issues, fix and commit.

**Step 3: Final commit if all good**

```bash
git add -A
git commit -m "feat(player): modular character system MVP complete"
```

---

## Future tasks (not in this plan)

These are documented in the design doc but deferred:

1. **Create actual per-part sprites** — Use PixelLab for reference, manually split in Aseprite into head/body/front_arm/back_arm
2. **Arm aiming system** — `ArmAiming` component, rotate front_arm toward cursor, weapon as child entity
3. **Equipment overlays** — Additional child entities (helmet over head, armor over body) following same frame layout
4. **Spritesheet migration** — Switch from individual frame PNGs to horizontal strip spritesheets per part (uses `sprite_uv_rect` in shader)
