# Vertical Slice Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Реализовать vertical slice — одна планета с боевой системой, 3 типами врагов, NPC-торговцем, прогрессией инструментов и опасностями окружения.

**Architecture:** Модульная система на Bevy ECS. Новые модули: `combat/`, `enemy/`, `trader/`. Расширение существующих: `item/`, `interaction/`, `physics.rs`, `ui/`. AI врагов через `statig` крейт. Спрайты через PixelLab.

**Tech Stack:** Bevy 0.18, statig (FSM), bevy_egui (HUD), RON (data), PixelLab (sprites)

---

## Chunk 1: Health System & Block Destruction

### Task 1: Health Component

**Files:**
- Create: `src/combat/mod.rs`
- Create: `src/combat/health.rs`
- Modify: `src/main.rs` (add CombatPlugin)
- Modify: `Cargo.toml` (add statig dependency)

- [ ] **Step 1: Add statig dependency**

In `Cargo.toml`, add:
```toml
statig = "0.3"
```

Run: `cargo check`

- [ ] **Step 2: Create combat module with Health component**

Create `src/combat/health.rs`:
```rust
use bevy::prelude::*;

#[derive(Component, Debug)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Health {
    pub fn new(max: f32) -> Self {
        Self { current: max, max }
    }

    pub fn take_damage(&mut self, amount: f32) {
        self.current = (self.current - amount).max(0.0);
    }

    pub fn heal(&mut self, amount: f32) {
        self.current = (self.current + amount).min(self.max);
    }

    pub fn is_dead(&self) -> bool {
        self.current <= 0.0
    }

    pub fn ratio(&self) -> f32 {
        self.current / self.max
    }
}

#[derive(Component, Debug)]
pub struct InvincibilityTimer {
    pub remaining: f32,
}

impl InvincibilityTimer {
    pub fn new(duration: f32) -> Self {
        Self { remaining: duration }
    }
}
```

Create `src/combat/mod.rs`:
```rust
pub mod health;

use bevy::prelude::*;
use crate::sets::GameSet;

pub use health::*;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            tick_invincibility.in_set(GameSet::Physics),
        );
    }
}

fn tick_invincibility(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut InvincibilityTimer)>,
) {
    let dt = time.delta_secs();
    for (entity, mut timer) in &mut query {
        timer.remaining -= dt;
        if timer.remaining <= 0.0 {
            commands.entity(entity).remove::<InvincibilityTimer>();
        }
    }
}
```

- [ ] **Step 3: Register CombatPlugin in main.rs**

In `src/main.rs`, add:
```rust
mod combat;
// ...
.add_plugins(combat::CombatPlugin)
```

- [ ] **Step 4: Add Health to Player**

In `src/player/mod.rs`, add `Health::new(100.0)` to the player spawn bundle (where Oxygen is added).

- [ ] **Step 5: Verify compilation**

Run: `cargo check`

- [ ] **Step 6: Commit**

```bash
git add src/combat/ src/main.rs src/player/mod.rs Cargo.toml
git commit -m "feat(combat): add Health component and InvincibilityTimer"
```

---

### Task 2: Damage Event System

**Files:**
- Create: `src/combat/damage.rs`
- Modify: `src/combat/mod.rs`

- [ ] **Step 1: Create damage event and processing system**

Create `src/combat/damage.rs`:
```rust
use bevy::prelude::*;

use super::{Health, InvincibilityTimer};

const INVINCIBILITY_DURATION: f32 = 0.5;

#[derive(Message, Debug)]
pub struct DamageEvent {
    pub target: Entity,
    pub amount: f32,
    pub knockback: Vec2,
}

pub fn process_damage(
    mut commands: Commands,
    mut reader: bevy::ecs::message::MessageReader<DamageEvent>,
    mut query: Query<(&mut Health, Option<&InvincibilityTimer>)>,
) {
    for event in reader.read() {
        let Ok((mut health, invincibility)) = query.get_mut(event.target) else {
            continue;
        };
        if invincibility.is_some() {
            continue;
        }
        health.take_damage(event.amount);
        commands.entity(event.target).insert(
            InvincibilityTimer::new(INVINCIBILITY_DURATION),
        );
    }
}

pub fn apply_damage_knockback(
    mut reader: bevy::ecs::message::MessageReader<DamageEvent>,
    mut query: Query<(&mut crate::physics::Velocity, Option<&InvincibilityTimer>)>,
) {
    for event in reader.read() {
        let Ok((mut vel, invincibility)) = query.get_mut(event.target) else {
            continue;
        };
        if invincibility.is_some() {
            continue;
        }
        vel.x += event.knockback.x;
        vel.y += event.knockback.y;
    }
}
```

- [ ] **Step 2: Register in CombatPlugin**

Add to `src/combat/mod.rs`:
```rust
pub mod damage;
pub use damage::*;

// In Plugin::build:
app.add_message::<DamageEvent>()
    .add_systems(
        Update,
        (damage::process_damage, damage::apply_damage_knockback)
            .in_set(GameSet::Physics),
    );
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`

- [ ] **Step 4: Commit**

```bash
git add src/combat/
git commit -m "feat(combat): add DamageEvent with knockback and invincibility"
```

---

### Task 3: Player Death & Respawn

**Files:**
- Create: `src/combat/death.rs`
- Modify: `src/combat/mod.rs`

- [ ] **Step 1: Create death detection system**

Create `src/combat/death.rs`:
```rust
use bevy::prelude::*;

use super::Health;
use crate::player::Player;

#[derive(Message, Debug)]
pub struct PlayerDeathEvent;

pub fn detect_player_death(
    query: Query<&Health, With<Player>>,
    mut writer: bevy::ecs::message::MessageWriter<PlayerDeathEvent>,
) {
    for health in &query {
        if health.is_dead() {
            writer.write(PlayerDeathEvent);
        }
    }
}

pub fn handle_player_death(
    mut reader: bevy::ecs::message::MessageReader<PlayerDeathEvent>,
    mut query: Query<&mut Health, With<Player>>,
) {
    for _event in reader.read() {
        // Восстановить здоровье при респауне
        for mut health in &mut query {
            health.current = health.max;
        }
        // TODO: варп на корабль (использовать существующий WarpToShip)
        warn!("Player died! Respawning...");
    }
}
```

- [ ] **Step 2: Register in CombatPlugin**

Add to `src/combat/mod.rs`:
```rust
pub mod death;

// In Plugin::build:
app.add_message::<PlayerDeathEvent>()
    .add_systems(
        Update,
        (death::detect_player_death, death::handle_player_death)
            .chain()
            .in_set(GameSet::Physics),
    );
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`

- [ ] **Step 4: Commit**

```bash
git add src/combat/
git commit -m "feat(combat): add player death detection and respawn"
```

---

### Task 4: Health HUD Bar

**Files:**
- Create: `src/ui/game_ui/health_hud.rs`
- Modify: `src/ui/game_ui/mod.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Create health HUD**

Create `src/ui/game_ui/health_hud.rs` — аналог `oxygen_hud.rs`:
```rust
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::combat::Health;
use crate::player::Player;

pub fn draw_health_hud(
    mut contexts: EguiContexts,
    query: Query<&Health, With<Player>>,
) -> Result {
    let Ok(health) = query.single() else {
        return Ok(());
    };

    let ratio = health.ratio();
    let bar_color = if ratio > 0.5 {
        egui::Color32::from_rgb(50, 200, 50) // Green
    } else if ratio > 0.25 {
        egui::Color32::from_rgb(230, 200, 50) // Yellow
    } else {
        egui::Color32::from_rgb(220, 50, 50) // Red
    };

    let ctx = contexts.ctx_mut()?;
    // Позиционируем под oxygen bar (oxygen at y=10, health at y=32)
    egui::Area::new(egui::Id::new("health_hud"))
        .fixed_pos(egui::pos2(10.0, 32.0))
        .show(ctx, |ui| {
            let bar_width = 140.0_f32;
            let bar_height = 16.0_f32;

            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(bar_width, bar_height),
                egui::Sense::hover(),
            );

            let painter = ui.painter();
            let bg = egui::Color32::from_rgba_unmultiplied(20, 20, 30, 180);
            painter.rect_filled(rect, 2.0, bg);

            let fill_rect = egui::Rect::from_min_size(
                rect.min,
                egui::vec2(bar_width * ratio, bar_height),
            );
            painter.rect_filled(fill_rect, 2.0, bar_color);

            painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, egui::Color32::GRAY));

            let label = format!("HP {}/{}", health.current as i32, health.max as i32);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(11.0),
                egui::Color32::WHITE,
            );
        });

    Ok(())
}
```

- [ ] **Step 2: Register in UI systems**

In `src/ui/game_ui/mod.rs`, add: `pub mod health_hud;`

In `src/ui/mod.rs`, register the system in `EguiPrimaryContextPass` alongside `draw_oxygen_hud`:
```rust
game_ui::health_hud::draw_health_hud.run_if(in_state(AppState::InGame)),
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`

- [ ] **Step 4: Commit**

```bash
git add src/ui/game_ui/health_hud.rs src/ui/game_ui/mod.rs src/ui/mod.rs
git commit -m "feat(ui): add health HUD bar"
```

---

### Task 5: Block Destruction with HP and Regen

**Files:**
- Create: `src/combat/block_damage.rs`
- Modify: `src/combat/mod.rs`
- Modify: `src/interaction/block_action.rs`
- Modify: `src/world/chunk.rs` (ChunkData.damage уже заложен)

- [ ] **Step 1: Create block damage tracking resource**

Create `src/combat/block_damage.rs`:
```rust
use bevy::prelude::*;
use std::collections::HashMap;

/// Ключ: (tile_x, tile_y) в мировых координатах
#[derive(Resource, Default, Debug)]
pub struct BlockDamageMap {
    pub damage: HashMap<(i32, i32), BlockDamageState>,
}

#[derive(Debug)]
pub struct BlockDamageState {
    pub accumulated: f32,
    pub regen_timer: f32,
}

const REGEN_DELAY: f32 = 2.0; // Секунды до начала регенерации
const REGEN_RATE: f32 = 0.5;  // HP/сек регенерации

pub fn tick_block_damage_regen(
    time: Res<Time>,
    mut damage_map: ResMut<BlockDamageMap>,
) {
    let dt = time.delta_secs();
    damage_map.damage.retain(|_pos, state| {
        state.regen_timer += dt;
        if state.regen_timer >= REGEN_DELAY {
            state.accumulated -= REGEN_RATE * dt;
        }
        state.accumulated > 0.0
    });
}
```

- [ ] **Step 2: Modify block_action.rs to use damage accumulation**

В `src/interaction/block_action.rs` — вместо мгновенного разрушения, накапливать урон:
- Добавить `Res<BlockDamageMap>` и `Res<TileRegistry>` в параметры системы `block_interaction_system`
- При зажатой ЛКМ: определить target tile через cursor→world координаты (уже есть в системе)
- Получить `hardness` через `tile_registry.get(tile_id).hardness`
- Каждый кадр добавлять `mining_power * dt` в `BlockDamageMap` для целевого тайла
- Получить `mining_power` из `ItemStats` предмета в активном слоте хотбара через `ItemRegistry`
- Если нет инструмента в руке — использовать базовый `mining_power = 1.0`
- Сбрасывать `regen_timer = 0.0` при каждом ударе
- Когда `accumulated >= hardness` — вызвать существующую логику `break_block()` и удалить запись из map
- Заменить `mouse.just_pressed(MouseButton::Left)` на `mouse.pressed(MouseButton::Left)` для непрерывного копания

Примечание: `TileDef` уже имеет `hardness: f32` поле. `TileRegistry` уже доступен как `Res<TileRegistry>`.

- [ ] **Step 3: Register systems**

In `src/combat/mod.rs`:
```rust
pub mod block_damage;

// In Plugin::build:
app.init_resource::<block_damage::BlockDamageMap>()
    .add_systems(
        Update,
        block_damage::tick_block_damage_regen.in_set(GameSet::WorldUpdate),
    );
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check`

- [ ] **Step 5: Commit**

```bash
git add src/combat/block_damage.rs src/combat/mod.rs src/interaction/block_action.rs
git commit -m "feat(combat): add block damage accumulation with regen"
```

---

### Task 6: Block Destruction Visual Overlay

**Files:**
- Modify: `src/world/tile_renderer.rs` или create `src/combat/block_damage_visual.rs`

- [ ] **Step 1: Create visual overlay system**

Добавить систему которая рисует overlay трещин поверх повреждённых блоков:
- Читать `BlockDamageMap`
- Для каждого повреждённого блока: рассчитать стадию (0-3) по `accumulated / hardness`
- Отображать соответствующий спрайт overlay (crack_1.png, crack_2.png, crack_3.png)
- Использовать entity с `Sprite` + `Transform` на позиции блока, z=0.1

- [ ] **Step 2: Verify compilation**

Run: `cargo check`

- [ ] **Step 3: Commit**

```bash
git add -A src/combat/
git commit -m "feat(combat): add block damage visual overlay"
```

---

## Chunk 2: Tool Stats & Item Progression

### Task 7: Extend ItemStats with Mining Power

**Files:**
- Modify: `src/item/definition.rs`

- [ ] **Step 1: Add mining_power and attack_speed to ItemStats**

В `src/item/definition.rs`, расширить `ItemStats`:
```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ItemStats {
    pub damage: Option<f32>,
    pub defense: Option<f32>,
    pub speed_bonus: Option<f32>,
    pub health_bonus: Option<i32>,
    pub mining_power: Option<f32>,    // NEW
    pub attack_speed: Option<f32>,    // NEW
    pub knockback: Option<f32>,       // NEW
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check`

- [ ] **Step 3: Commit**

```bash
git add src/item/definition.rs
git commit -m "feat(item): add mining_power, attack_speed, knockback to ItemStats"
```

---

### Task 8: Tool & Weapon Item Definitions (RON)

**Files:**
- Create: `assets/content/items/stone_pickaxe/stone_pickaxe.item.ron`
- Create: `assets/content/items/iron_pickaxe/iron_pickaxe.item.ron`
- Create: `assets/content/items/advanced_pickaxe/advanced_pickaxe.item.ron`
- Create: `assets/content/items/stone_sword/stone_sword.item.ron`
- Create: `assets/content/items/iron_sword/iron_sword.item.ron`
- Create: `assets/content/items/bow/bow.item.ron`
- Create: `assets/content/items/arrow/arrow.item.ron`

- [ ] **Step 1: Create pickaxe item definitions**

Пример `stone_pickaxe.item.ron`:
```ron
(
    id: "stone_pickaxe",
    display_name: "Stone Pickaxe",
    description: "A basic pickaxe made of stone.",
    max_stack: 1,
    rarity: Common,
    item_type: Tool,
    stats: Some((
        damage: Some(3.0),
        mining_power: Some(2.0),
        attack_speed: Some(1.0),
        knockback: Some(2.0),
    )),
)
```

`iron_pickaxe.item.ron`: mining_power: 5.0, damage: 5.0
`advanced_pickaxe.item.ron`: mining_power: 10.0, damage: 7.0

- [ ] **Step 2: Create weapon item definitions**

`stone_sword.item.ron`: damage: 8.0, attack_speed: 1.2, knockback: 4.0
`iron_sword.item.ron`: damage: 15.0, attack_speed: 1.4, knockback: 5.0
`bow.item.ron`: damage: 10.0, attack_speed: 0.8 (расход arrow)
`arrow.item.ron`: item_type: Resource, max_stack: 999

- [ ] **Step 3: Create ore item definitions**

Create RON files for:
- `iron_ore.item.ron`: Resource, max_stack: 999
- `crystal.item.ron`: Resource, max_stack: 999
- `rare_ore.item.ron`: Resource, max_stack: 999

- [ ] **Step 4: Verify assets load**

Run: `cargo run` — проверить что предметы загружаются без ошибок в логе.

- [ ] **Step 5: Commit**

```bash
git add assets/content/items/
git commit -m "content: add tool, weapon, and ore item definitions"
```

---

### Task 9: Crafting Recipes for Tools

**Files:**
- Create/modify recipe RON files in `assets/content/recipes/`

- [ ] **Step 1: Add crafting recipes**

Рецепты для ручного крафта:
- stone_pickaxe: 5 stone + 3 wood
- stone_sword: 7 stone + 2 wood
- iron_pickaxe: 5 iron_ore + 3 wood (если уже есть материалы)
- iron_sword: 7 iron_ore + 2 wood
- bow: 3 wood + 5 fiber (или string)
- arrow x10: 1 wood + 1 stone

- [ ] **Step 2: Verify recipes load**

Run: `cargo run` — открыть crafting panel (C), проверить рецепты.

- [ ] **Step 3: Commit**

```bash
git add assets/content/recipes/
git commit -m "content: add tool and weapon crafting recipes"
```

---

### Task 10: Ore Tile Definitions

**Files:**
- Create: `assets/content/tiles/iron_ore/iron_ore.tile.ron`
- Create: `assets/content/tiles/crystal/crystal.tile.ron`
- Create: `assets/content/tiles/rare_ore/rare_ore.tile.ron`
- Modify: biome RON files to include ores in generation

- [ ] **Step 1: Create ore tile definitions**

`iron_ore.tile.ron`:
```ron
(
    id: "iron_ore",
    solid: true,
    hardness: 4.0,
    friction: 0.8,
    drops: [(item_id: "iron_ore", min: 1, max: 2, chance: 1.0)],
    light_emission: (0, 0, 0),
    light_opacity: 15,
    albedo: (139, 119, 101),
)
```

`crystal.tile.ron`: hardness: 6.0, light_emission: (80, 80, 200)
`rare_ore.tile.ron`: hardness: 10.0

- [ ] **Step 2: Modify terrain generation to place ores**

В `src/world/terrain_gen.rs`, добавить логику вкрапления руд в камень:
- Использовать дополнительный слой Perlin noise
- iron_ore: глубина от surface-10 до surface-40, threshold ~0.7
- crystal: глубина surface-30 до surface-60, threshold ~0.8
- rare_ore: глубина surface-50+, threshold ~0.85

- [ ] **Step 3: Verify compilation and generation**

Run: `cargo run` — копнуть вниз и проверить что руды появляются.

- [ ] **Step 4: Commit**

```bash
git add assets/content/tiles/ src/world/terrain_gen.rs
git commit -m "content: add ore tiles with depth-based generation"
```

---

## Chunk 3: Melee & Ranged Combat

### Task 11: Melee Attack System

**Files:**
- Create: `src/combat/melee.rs`
- Modify: `src/combat/mod.rs`

- [ ] **Step 1: Create melee attack system**

Create `src/combat/melee.rs`:
```rust
use bevy::prelude::*;

use crate::player::Player;
use crate::inventory::Hotbar;
use crate::item::ItemRegistry;
use crate::physics::{Velocity, TileCollider};

use super::DamageEvent;

#[derive(Component, Debug)]
pub struct MeleeAttack {
    pub damage: f32,
    pub knockback: f32,
    pub range: f32,       // В тайлах
    pub cooldown: f32,
    pub timer: f32,
}

/// Система: при ЛКМ, если предмет в руке имеет damage — создать hitbox
pub fn melee_attack_system(
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    item_registry: Res<ItemRegistry>,
    mut player_query: Query<(&Transform, &Hotbar, &mut MeleeAttack), With<Player>>,
    // Враги с Health и TileCollider
    target_query: Query<(Entity, &Transform, &TileCollider), Without<Player>>,
    mut damage_writer: bevy::ecs::message::MessageWriter<DamageEvent>,
) {
    let dt = time.delta_secs();

    for (player_tf, hotbar, mut attack) in &mut player_query {
        attack.timer -= dt;
        if attack.timer > 0.0 {
            continue;
        }

        if !mouse.just_pressed(MouseButton::Left) {
            continue;
        }

        // Получить stats из предмета в активной руке
        // Если нет оружия — пропустить или использовать базовые значения
        // Проверить все враги в радиусе range
        // Отправить DamageEvent для каждого попавшего

        attack.timer = attack.cooldown;

        let player_pos = player_tf.translation.truncate();
        for (target_entity, target_tf, _collider) in &target_query {
            let target_pos = target_tf.translation.truncate();
            let distance = player_pos.distance(target_pos);
            if distance <= attack.range * 32.0 {
                let direction = (target_pos - player_pos).normalize_or_zero();
                damage_writer.write(DamageEvent {
                    target: target_entity,
                    amount: attack.damage,
                    knockback: direction * attack.knockback,
                });
            }
        }
    }
}
```

Примечание: реальная реализация должна учитывать facing direction игрока и arm aiming. Хитбокс — конус/полукруг перед игроком, а не сфера.

- [ ] **Step 2: Add MeleeAttack to player spawn**

В `src/player/mod.rs`, добавить `MeleeAttack` компонент при спавне с дефолтными значениями.

- [ ] **Step 3: Register system**

В `src/combat/mod.rs`:
```rust
pub mod melee;
// In Plugin::build:
.add_systems(Update, melee::melee_attack_system.in_set(GameSet::Input))
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check`

- [ ] **Step 5: Commit**

```bash
git add src/combat/melee.rs src/combat/mod.rs src/player/mod.rs
git commit -m "feat(combat): add melee attack system with cooldown"
```

---

### Task 12: Ranged Attack System (Projectiles)

**Files:**
- Create: `src/combat/projectile.rs`
- Modify: `src/combat/mod.rs`

- [ ] **Step 1: Create projectile component and systems**

Create `src/combat/projectile.rs`:
```rust
use bevy::prelude::*;

use crate::physics::{Velocity, TileCollider, Gravity};

use super::DamageEvent;

#[derive(Component, Debug)]
pub struct Projectile {
    pub damage: f32,
    pub knockback: f32,
    pub lifetime: f32,
    pub owner: Entity,
}

/// Спавн снаряда
pub fn spawn_projectile(
    commands: &mut Commands,
    position: Vec2,
    direction: Vec2,
    speed: f32,
    damage: f32,
    knockback: f32,
    owner: Entity,
) -> Entity {
    commands.spawn((
        Projectile {
            damage,
            knockback,
            lifetime: 3.0,
            owner,
        },
        Velocity {
            x: direction.x * speed,
            y: direction.y * speed,
        },
        Transform::from_xyz(position.x, position.y, 0.5),
        Gravity(200.0), // Лёгкая гравитация для параболы
        TileCollider {
            width: 0.25,
            height: 0.25,
        },
    )).id()
}

/// Обновление lifetime и удаление
pub fn tick_projectiles(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Projectile)>,
) {
    let dt = time.delta_secs();
    for (entity, mut proj) in &mut query {
        proj.lifetime -= dt;
        if proj.lifetime <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

/// Проверка столкновений снарядов с целями
pub fn projectile_hit_detection(
    mut commands: Commands,
    projectile_query: Query<(Entity, &Transform, &Projectile, &TileCollider)>,
    target_query: Query<(Entity, &Transform, &TileCollider), Without<Projectile>>,
    mut damage_writer: bevy::ecs::message::MessageWriter<DamageEvent>,
) {
    for (proj_entity, proj_tf, proj, proj_col) in &projectile_query {
        let proj_pos = proj_tf.translation.truncate();

        for (target_entity, target_tf, target_col) in &target_query {
            if target_entity == proj.owner {
                continue;
            }
            let target_pos = target_tf.translation.truncate();

            // Простая AABB проверка
            let dx = (proj_pos.x - target_pos.x).abs();
            let dy = (proj_pos.y - target_pos.y).abs();
            let overlap_x = (proj_col.width + target_col.width) / 2.0 * 32.0;
            let overlap_y = (proj_col.height + target_col.height) / 2.0 * 32.0;

            if dx < overlap_x && dy < overlap_y {
                let direction = (target_pos - proj_pos).normalize_or_zero();
                damage_writer.write(DamageEvent {
                    target: target_entity,
                    amount: proj.damage,
                    knockback: direction * proj.knockback,
                });
                commands.entity(proj_entity).despawn();
                break;
            }
        }
    }
}
```

- [ ] **Step 2: Register projectile systems**

В `src/combat/mod.rs`:
```rust
pub mod projectile;
// In Plugin::build:
.add_systems(
    Update,
    (projectile::tick_projectiles, projectile::projectile_hit_detection)
        .in_set(GameSet::Physics),
)
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`

- [ ] **Step 4: Commit**

```bash
git add src/combat/projectile.rs src/combat/mod.rs
git commit -m "feat(combat): add projectile system with hit detection"
```

---

### Task 13: Player Ranged Attack Input

**Files:**
- Create: `src/combat/ranged.rs`
- Modify: `src/combat/mod.rs`

- [ ] **Step 1: Create ranged attack input system**

Create `src/combat/ranged.rs` — при ЛКМ с луком в руке:
- Проверить наличие стрел в инвентаре
- Расходовать 1 стрелу
- Получить направление из `ArmAiming` компонента (`src/player/parts.rs:44-53`) — вычислить вектор направления из угла поворота руки (совпадает с cursor→player вектором)
- Вызвать `spawn_projectile()` с параметрами из ItemStats лука
- Проверить `item_type == ItemType::Weapon` и наличие `stats.damage` для определения что предмет — дальнобойное оружие

- [ ] **Step 2: Register system**

Добавить в `GameSet::Input`, после melee (чтобы melee имел приоритет для мечей).

- [ ] **Step 3: Verify compilation**

Run: `cargo check`

- [ ] **Step 4: Commit**

```bash
git add src/combat/ranged.rs src/combat/mod.rs
git commit -m "feat(combat): add ranged attack with ammo consumption"
```

---

## Chunk 4: Enemy AI System

### Task 14: Enemy Component Definitions

**Files:**
- Create: `src/enemy/mod.rs`
- Create: `src/enemy/components.rs`

- [ ] **Step 1: Create enemy module with base components**

Create `src/enemy/components.rs`:
```rust
use bevy::prelude::*;

#[derive(Component, Debug)]
pub struct Enemy;

#[derive(Component, Debug)]
pub struct DetectionRange(pub f32); // В тайлах

#[derive(Component, Debug)]
pub struct AttackRange(pub f32); // В тайлах

#[derive(Component, Debug)]
pub struct AttackCooldown {
    pub duration: f32,
    pub timer: f32,
}

#[derive(Component, Debug)]
pub struct ContactDamage(pub f32);

#[derive(Component, Debug)]
pub struct PatrolAnchor(pub Vec2); // Стартовая позиция для возврата

#[derive(Component, Debug)]
pub struct MoveSpeed(pub f32);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyType {
    Slime,
    Shooter,
    Flyer,
}
```

Create `src/enemy/mod.rs`:
```rust
pub mod components;

use bevy::prelude::*;
use crate::sets::GameSet;

pub use components::*;

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        // Будет расширяться в следующих тасках
    }
}
```

- [ ] **Step 2: Register EnemyPlugin in main.rs**

В `src/main.rs`:
```rust
mod enemy;
// ...
.add_plugins(enemy::EnemyPlugin)
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`

- [ ] **Step 4: Commit**

```bash
git add src/enemy/ src/main.rs
git commit -m "feat(enemy): add enemy module with base components"
```

---

### Task 15: Enemy AI with statig

**Files:**
- Create: `src/enemy/ai.rs`
- Modify: `src/enemy/mod.rs`

- [ ] **Step 1: Create AI state machine using statig**

Create `src/enemy/ai.rs`:
```rust
use bevy::prelude::*;
use statig::prelude::*;

use crate::player::Player;
use crate::physics::Velocity;

use super::{Enemy, DetectionRange, AttackRange, PatrolAnchor, MoveSpeed};

// Events для стейт-машины
#[derive(Debug)]
pub enum AiInput {
    PlayerInRange(Vec2),   // Позиция игрока
    PlayerOutOfRange,
    InAttackRange(Vec2),
    ReachedAnchor,
    Tick(f32),             // delta time
}

// States
#[derive(Debug, Clone)]
pub enum State {
    Idle,
    Patrol { direction: f32 },
    Chase { target_pos: Vec2 },
    Attack { target_pos: Vec2 },
    Return,
}

// Shared state для машины
#[derive(Debug)]
pub struct EnemyAi {
    pub patrol_timer: f32,
}

#[state_machine(
    initial = "State::Idle",
    state(derive(Debug, Clone)),
    on_transition = "Self::on_transition",
)]
impl EnemyAi {
    #[state]
    fn idle(&mut self, event: &AiInput) -> Response<State> {
        match event {
            AiInput::PlayerInRange(pos) => Transition(State::Chase { target_pos: *pos }),
            AiInput::Tick(dt) => {
                self.patrol_timer -= dt;
                if self.patrol_timer <= 0.0 {
                    self.patrol_timer = 3.0;
                    let dir = if rand::random::<bool>() { 1.0 } else { -1.0 };
                    Transition(State::Patrol { direction: dir })
                } else {
                    Super
                }
            }
            _ => Super,
        }
    }

    #[state]
    fn patrol(&mut self, event: &AiInput) -> Response<State> {
        match event {
            AiInput::PlayerInRange(pos) => Transition(State::Chase { target_pos: *pos }),
            AiInput::Tick(dt) => {
                self.patrol_timer -= dt;
                if self.patrol_timer <= 0.0 {
                    Transition(State::Idle)
                } else {
                    Super
                }
            }
            _ => Super,
        }
    }

    #[state]
    fn chase(&mut self, event: &AiInput) -> Response<State> {
        match event {
            AiInput::PlayerOutOfRange => Transition(State::Return),
            AiInput::InAttackRange(pos) => Transition(State::Attack { target_pos: *pos }),
            AiInput::PlayerInRange(pos) => {
                // Обновляем target
                Transition(State::Chase { target_pos: *pos })
            }
            _ => Super,
        }
    }

    #[state]
    fn attack(&mut self, event: &AiInput) -> Response<State> {
        match event {
            AiInput::PlayerOutOfRange => Transition(State::Chase { target_pos: Vec2::ZERO }),
            AiInput::PlayerInRange(pos) => Transition(State::Attack { target_pos: *pos }),
            _ => Super,
        }
    }

    #[state]
    fn r#return(&mut self, event: &AiInput) -> Response<State> {
        match event {
            AiInput::PlayerInRange(pos) => Transition(State::Chase { target_pos: *pos }),
            AiInput::ReachedAnchor => Transition(State::Idle),
            _ => Super,
        }
    }

    fn on_transition(&mut self, _source: &State, _target: &State) {
        // Можно логировать переходы
    }
}

/// Bevy-компонент, хранящий statig машину
#[derive(Component)]
pub struct AiStateMachine {
    pub machine: statig::blocking::InitializedStateMachine<EnemyAi>,
}

impl AiStateMachine {
    pub fn new() -> Self {
        let ai = EnemyAi { patrol_timer: 2.0 };
        Self {
            machine: ai.state_machine().init(),
        }
    }
}
```

Примечание: точный API `statig` может отличаться — сверить с документацией крейта при реализации.

- [ ] **Step 2: Create AI tick system**

Добавить в `src/enemy/ai.rs` систему `enemy_ai_tick`:
- Для каждого Enemy с AiStateMachine:
  - Найти ближайшего Player
  - Рассчитать дистанцию
  - Отправить соответствующий AiInput в машину
  - На основе текущего State — применить движение через Velocity

- [ ] **Step 3: Register in EnemyPlugin**

```rust
app.add_systems(
    Update,
    ai::enemy_ai_tick.in_set(GameSet::Input),
);
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check`

- [ ] **Step 5: Commit**

```bash
git add src/enemy/ai.rs src/enemy/mod.rs
git commit -m "feat(enemy): add AI state machine with statig"
```

---

### Task 16: Slime Enemy Behavior

**Files:**
- Create: `src/enemy/slime.rs`
- Modify: `src/enemy/mod.rs`

- [ ] **Step 1: Create slime-specific behavior**

Create `src/enemy/slime.rs`:
- Патрулирование: ходит по платформе влево-вправо
- Chase: прыгает к игроку (задать vel.y при jump, vel.x в сторону игрока)
- Attack: контактный урон через `ContactDamage`
- Параметры: detection_range=8, move_speed=60, contact_damage=10, health=30

- [ ] **Step 2: Create contact damage system**

Добавить систему `contact_damage_system`:
- Для каждого Enemy с `ContactDamage` — проверить AABB overlap с Player
- При пересечении — отправить DamageEvent

- [ ] **Step 3: Verify compilation**

Run: `cargo check`

- [ ] **Step 4: Commit**

```bash
git add src/enemy/slime.rs src/enemy/mod.rs
git commit -m "feat(enemy): add slime behavior with contact damage"
```

---

### Task 17: Shooter Enemy Behavior

**Files:**
- Create: `src/enemy/shooter.rs`
- Modify: `src/enemy/mod.rs`

- [ ] **Step 1: Create shooter-specific behavior**

Create `src/enemy/shooter.rs`:
- Patrol: медленно ходит
- Alert/Chase: останавливается, поворачивается к игроку
- Attack: стреляет снарядом через `projectile::spawn_projectile()`
- AttackCooldown: 2.0 секунды между выстрелами
- Параметры: detection_range=12, attack_range=10, move_speed=30, projectile_damage=8, health=25

- [ ] **Step 2: Verify compilation**

Run: `cargo check`

- [ ] **Step 3: Commit**

```bash
git add src/enemy/shooter.rs src/enemy/mod.rs
git commit -m "feat(enemy): add shooter behavior with ranged attacks"
```

---

### Task 18: Flyer Enemy Behavior

**Files:**
- Create: `src/enemy/flyer.rs`
- Modify: `src/enemy/mod.rs`

- [ ] **Step 1: Create flyer-specific behavior**

Create `src/enemy/flyer.rs`:
- Без гравитации (Gravity(0.0))
- Patrol: плавает в зоне вокруг anchor
- Chase: летит напрямую к игроку (через воздух)
- Attack: контактный урон
- Параметры: detection_range=10, move_speed=80, contact_damage=7, health=20

- [ ] **Step 2: Verify compilation**

Run: `cargo check`

- [ ] **Step 3: Commit**

```bash
git add src/enemy/flyer.rs src/enemy/mod.rs
git commit -m "feat(enemy): add flyer behavior with aerial movement"
```

---

### Task 19: Enemy Death & Loot Drop

**Files:**
- Create: `src/enemy/loot.rs`
- Modify: `src/enemy/mod.rs`

- [ ] **Step 1: Create loot table component and death system**

Create `src/enemy/loot.rs`:
```rust
use bevy::prelude::*;
use serde::Deserialize;

use crate::combat::Health;
use crate::item::dropped::spawn_dropped_item;

use super::Enemy;

#[derive(Component, Debug, Clone, Deserialize)]
pub struct LootTable {
    pub drops: Vec<LootDrop>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LootDrop {
    pub item_id: String,
    pub min: u16,
    pub max: u16,
    pub chance: f32,
}

pub fn enemy_death_system(
    mut commands: Commands,
    query: Query<(Entity, &Transform, &Health, Option<&LootTable>), With<Enemy>>,
) {
    for (entity, transform, health, loot) in &query {
        if !health.is_dead() {
            continue;
        }

        // Дроп лута
        if let Some(loot_table) = loot {
            let pos = transform.translation.truncate();
            for drop in &loot_table.drops {
                if rand::random::<f32>() <= drop.chance {
                    let count = if drop.min == drop.max {
                        drop.min
                    } else {
                        rand::random::<u16>() % (drop.max - drop.min + 1) + drop.min
                    };
                    // spawn_dropped_item(commands, pos, &drop.item_id, count);
                }
            }
        }

        commands.entity(entity).despawn_recursive();
    }
}
```

- [ ] **Step 2: Register system**

В `src/enemy/mod.rs`, добавить в `GameSet::WorldUpdate`.

- [ ] **Step 3: Verify compilation**

Run: `cargo check`

- [ ] **Step 4: Commit**

```bash
git add src/enemy/loot.rs src/enemy/mod.rs
git commit -m "feat(enemy): add loot table and death system"
```

---

## Chunk 5: Mob Spawning & Environmental Hazards

### Task 20: Mob Spawn System

**Files:**
- Create: `src/enemy/spawner.rs`
- Modify: `src/enemy/mod.rs`

- [ ] **Step 1: Create spawn zone system**

Create `src/enemy/spawner.rs`:
```rust
use bevy::prelude::*;

use crate::player::Player;
use crate::world::WorldContext;
use crate::combat::Health;
use crate::physics::*;

use super::*;

#[derive(Resource, Debug)]
pub struct MobSpawnConfig {
    pub max_mobs: usize,
    pub spawn_radius_min: f32,  // Минимум от игрока (за экраном)
    pub spawn_radius_max: f32,  // Максимум от игрока
    pub spawn_interval: f32,    // Секунды между попытками спавна
    pub timer: f32,
}

impl Default for MobSpawnConfig {
    fn default() -> Self {
        Self {
            max_mobs: 15,
            spawn_radius_min: 20.0, // тайлов
            spawn_radius_max: 35.0,
            spawn_interval: 5.0,
            timer: 0.0,
        }
    }
}

pub fn mob_spawn_system(
    mut commands: Commands,
    time: Res<Time>,
    mut config: ResMut<MobSpawnConfig>,
    player_query: Query<&Transform, With<Player>>,
    enemy_query: Query<(), With<Enemy>>,
    // world_context для определения поверхности/пещеры
) {
    config.timer -= time.delta_secs();
    if config.timer > 0.0 {
        return;
    }
    config.timer = config.spawn_interval;

    let enemy_count = enemy_query.iter().count();
    if enemy_count >= config.max_mobs {
        return;
    }

    let Ok(player_tf) = player_query.single() else {
        return;
    };

    let player_pos = player_tf.translation.truncate();

    // Выбрать случайную позицию за экраном
    // Определить тип моба по глубине (поверхность vs пещера)
    // Спавнить соответствующий тип
    // На поверхности ночью: увеличить spawn rate
}

fn spawn_slime(commands: &mut Commands, pos: Vec2) {
    commands.spawn((
        Enemy,
        EnemyType::Slime,
        Health::new(30.0),
        Velocity::default(),
        Gravity(400.0),
        TileCollider { width: 0.8, height: 0.8 },
        Transform::from_xyz(pos.x, pos.y, 1.0),
        DetectionRange(8.0),
        AttackRange(1.5),
        ContactDamage(10.0),
        MoveSpeed(60.0),
        PatrolAnchor(pos),
        AttackCooldown { duration: 1.0, timer: 0.0 },
        LootTable { drops: vec![
            LootDrop { item_id: "slime_gel".into(), min: 1, max: 2, chance: 0.8 },
        ]},
        // AiStateMachine::new(),
    ));
}

// Аналогичные fn spawn_shooter, fn spawn_flyer
```

- [ ] **Step 2: Register spawner**

В `src/enemy/mod.rs`:
```rust
pub mod spawner;
// In Plugin::build:
app.init_resource::<spawner::MobSpawnConfig>()
    .add_systems(Update, spawner::mob_spawn_system.in_set(GameSet::WorldUpdate));
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`

- [ ] **Step 4: Commit**

```bash
git add src/enemy/spawner.rs src/enemy/mod.rs
git commit -m "feat(enemy): add mob spawning system with depth-based types"
```

---

### Task 21: Fall Damage

**Files:**
- Create: `src/combat/fall_damage.rs`
- Modify: `src/combat/mod.rs`

- [ ] **Step 1: Create fall damage system**

Create `src/combat/fall_damage.rs`:
```rust
use bevy::prelude::*;

use crate::player::Player;
use crate::physics::{Velocity, Grounded};

use super::DamageEvent;

const FALL_DAMAGE_THRESHOLD: f32 = -600.0; // Пиксели/с (вниз = отрицательная)
const FALL_DAMAGE_FACTOR: f32 = 0.02;      // Урон за единицу скорости сверх порога

#[derive(Component, Debug)]
pub struct FallTracker {
    pub prev_vel_y: f32,
    pub was_grounded: bool,
}

impl Default for FallTracker {
    fn default() -> Self {
        Self { prev_vel_y: 0.0, was_grounded: true }
    }
}

pub fn fall_damage_system(
    mut query: Query<(Entity, &Velocity, &Grounded, &mut FallTracker), With<Player>>,
    mut damage_writer: bevy::ecs::message::MessageWriter<DamageEvent>,
) {
    for (entity, vel, grounded, mut tracker) in &mut query {
        // Только что приземлился
        if grounded.0 && !tracker.was_grounded {
            let impact_speed = tracker.prev_vel_y; // Отрицательная при падении
            if impact_speed < FALL_DAMAGE_THRESHOLD {
                let excess = FALL_DAMAGE_THRESHOLD - impact_speed;
                let damage = excess * FALL_DAMAGE_FACTOR;
                damage_writer.write(DamageEvent {
                    target: entity,
                    amount: damage,
                    knockback: Vec2::ZERO,
                });
            }
        }

        tracker.prev_vel_y = vel.y;
        tracker.was_grounded = grounded.0;
    }
}
```

- [ ] **Step 2: Add FallTracker to player spawn, register system**

- [ ] **Step 3: Verify compilation**

Run: `cargo check`

- [ ] **Step 4: Commit**

```bash
git add src/combat/fall_damage.rs src/combat/mod.rs src/player/mod.rs
git commit -m "feat(combat): add fall damage system"
```

---

### Task 22: Lava Damage

**Files:**
- Create: `src/combat/liquid_damage.rs`
- Modify: `src/combat/mod.rs`

- [ ] **Step 1: Create liquid damage system**

Create `src/combat/liquid_damage.rs`:
- Использовать существующий `Submerged` компонент
- Проверить тип жидкости через `LiquidRegistry`
- Если `LiquidDef.damage_on_contact > 0` — отправить DamageEvent (dps * dt)
- Лава уже имеет поле `damage_on_contact` в `LiquidDef`

- [ ] **Step 2: Register system**

В GameSet::Physics.

- [ ] **Step 3: Verify compilation**

Run: `cargo check`

- [ ] **Step 4: Commit**

```bash
git add src/combat/liquid_damage.rs src/combat/mod.rs
git commit -m "feat(combat): add liquid damage (lava) system"
```

---

### Task 23: Oxygen Depletion Damage

**Files:**
- Modify: `src/player/oxygen.rs`

- [ ] **Step 1: Add health drain when oxygen depleted**

В `src/player/oxygen.rs`, в системе `tick_oxygen`:
- Когда `oxygen.current <= 0.0` — отправить DamageEvent с dps (например 10.0/с)
- Нужно добавить `MessageWriter<DamageEvent>` параметр в систему

- [ ] **Step 2: Verify compilation**

Run: `cargo check`

- [ ] **Step 3: Commit**

```bash
git add src/player/oxygen.rs
git commit -m "feat(combat): add health drain on oxygen depletion"
```

---

## Chunk 6: NPC Trader & Trading UI

### Task 24: Trader Component & Data

**Files:**
- Create: `src/trader/mod.rs`
- Create: `src/trader/components.rs`
- Create: `assets/content/npcs/merchant/merchant.trader.ron`

- [ ] **Step 1: Create trader module**

Create `src/trader/components.rs`:
```rust
use bevy::prelude::*;
use serde::Deserialize;

#[derive(Component, Debug)]
pub struct Trader;

#[derive(Component, Debug, Clone, Deserialize)]
pub struct TradeOffers {
    pub offers: Vec<TradeOffer>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TradeOffer {
    pub cost: Vec<(String, u16)>,    // (item_id, count)
    pub result: (String, u16),       // (item_id, count)
}
```

Create `src/trader/mod.rs`:
```rust
pub mod components;

use bevy::prelude::*;
use crate::sets::GameSet;

pub use components::*;

pub struct TraderPlugin;

impl Plugin for TraderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OpenTrader>();
    }
}

#[derive(Resource, Default)]
pub struct OpenTrader(pub Option<Entity>);
```

- [ ] **Step 2: Create merchant RON data**

Create `assets/content/npcs/merchant/merchant.trader.ron`:
```ron
(
    offers: [
        (cost: [("iron_ore", 10)], result: ("iron_sword", 1)),
        (cost: [("stone", 20)], result: ("stone_pickaxe", 1)),
        (cost: [("crystal", 5)], result: ("iron_pickaxe", 1)),
        (cost: [("slime_gel", 10)], result: ("arrow", 20)),
    ],
)
```

- [ ] **Step 3: Register TraderPlugin in main.rs**

```rust
mod trader;
.add_plugins(trader::TraderPlugin)
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check`

- [ ] **Step 5: Commit**

```bash
git add src/trader/ src/main.rs assets/content/npcs/
git commit -m "feat(trader): add trader components and data"
```

---

### Task 25: Trader Interaction

**Files:**
- Modify: `src/interaction/interactable.rs`
- Modify: `src/trader/mod.rs`

- [ ] **Step 1: Add Trader to interactable detection**

В `src/interaction/interactable.rs`:
- Добавить `Query<(Entity, &Transform), With<Trader>>` в `detect_nearby_interactable`
- При нажатии E рядом с Trader — установить `OpenTrader(Some(entity))`

- [ ] **Step 2: Verify compilation**

Run: `cargo check`

- [ ] **Step 3: Commit**

```bash
git add src/interaction/interactable.rs src/trader/mod.rs
git commit -m "feat(trader): integrate trader with interaction system"
```

---

### Task 26: Trading UI Panel

**Files:**
- Create: `src/ui/game_ui/trade_panel.rs`
- Modify: `src/ui/game_ui/mod.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Create trade panel UI**

Create `src/ui/game_ui/trade_panel.rs`:
- Использовать `window::spawn_window_frame()` для окна
- Структура:
  - Заголовок: "Trader"
  - Список предложений: вертикальный список
  - Каждое предложение: строка с иконками cost → result + кнопка "Trade"
  - Кнопка серая если не хватает ресурсов
- Управляется ресурсом `OpenTrader`
- Спавн/деспавн по аналогии с `crafting_panel.rs`

- [ ] **Step 2: Add trade execution system**

При нажатии "Trade":
- Проверить наличие всех cost items в инвентаре
- Убрать cost items
- Добавить result item

- [ ] **Step 3: Register in UI systems**

В `src/ui/game_ui/mod.rs`: `pub mod trade_panel;`
В `src/ui/mod.rs`: зарегистрировать системы панели.

- [ ] **Step 4: Verify compilation**

Run: `cargo check`

- [ ] **Step 5: Commit**

```bash
git add src/ui/game_ui/trade_panel.rs src/ui/game_ui/mod.rs src/ui/mod.rs
git commit -m "feat(ui): add trading panel with buy/sell functionality"
```

---

### Task 27: Trader Spawn on Planet

**Files:**
- Modify: `src/world/terrain_gen.rs` или create `src/trader/spawn.rs`

- [ ] **Step 1: Add scripted trader spawn point**

Для vertical slice — заскриптованная точка:
- При генерации мира: выбрать позицию на поверхности (surface_height + 2 тайла)
- Примерно в центре карты
- Спавнить entity с Trader, TradeOffers, Transform, спрайт

- [ ] **Step 2: Verify in game**

Run: `cargo run` — найти торговца на поверхности.

- [ ] **Step 3: Commit**

```bash
git add src/trader/
git commit -m "feat(trader): add scripted trader spawn on planet"
```

---

## Chunk 7: Sprite Generation & Polish

### Task 28: Generate Enemy Sprites with PixelLab

**Files:**
- Assets: `assets/sprites/enemies/slime/`, `assets/sprites/enemies/shooter/`, `assets/sprites/enemies/flyer/`
- Assets: `assets/sprites/npcs/merchant/`

- [ ] **Step 1: Generate slime sprites**

Через PixelLab MCP:
- Создать character "Slime" — зелёный слайм, pixel art, 32x32
- Анимации: idle (2 кадра), move/hop (4 кадра), death (3 кадра)

- [ ] **Step 2: Generate shooter sprites**

- Создать character "Shooter" — враждебный стрелок, pixel art, 32x32
- Анимации: idle (2 кадра), walk (4 кадра), shoot (3 кадра), death (3 кадра)

- [ ] **Step 3: Generate flyer sprites**

- Создать character "Flyer" — летающее существо, pixel art, 32x32
- Анимации: idle/fly (4 кадра), attack (2 кадра), death (3 кадра)

- [ ] **Step 4: Generate merchant NPC sprite**

- Создать character "Merchant" — дружелюбный торговец, pixel art, 32x32
- Статичный спрайт (idle только)

- [ ] **Step 5: Generate crack overlay sprites**

- 4 стадии разрушения блока: crack_1.png через crack_4.png
- 32x32, полупрозрачные трещины поверх тайла

- [ ] **Step 6: Generate projectile sprites**

- arrow.png: 16x16 стрела
- enemy_projectile.png: 16x16 вражеский снаряд

- [ ] **Step 7: Commit all sprites**

```bash
git add assets/sprites/
git commit -m "art: add enemy, NPC, and projectile sprites via PixelLab"
```

---

### Task 29: Wire Sprites to Entities

**Files:**
- Modify: `src/enemy/spawner.rs` — добавить спрайты при спавне
- Modify: `src/trader/spawn.rs` — спрайт торговца
- Modify: `src/combat/projectile.rs` — спрайт снаряда
- Modify: `src/combat/block_damage_visual.rs` — crack overlay спрайты

- [ ] **Step 1: Add sprite loading and assignment**

Для каждого типа enemy — загрузить spritesheet, задать TextureAtlas.
Для торговца — загрузить одиночный спрайт.
Для снарядов — загрузить маленький спрайт.

- [ ] **Step 2: Add invincibility flash visual**

Система мигания: когда есть `InvincibilityTimer` — чередовать Visibility::Visible/Hidden каждые 0.1с.

- [ ] **Step 3: Verify visuals in game**

Run: `cargo run`

- [ ] **Step 4: Commit**

```bash
git add src/enemy/ src/trader/ src/combat/
git commit -m "feat: wire sprites to enemies, trader, and projectiles"
```

---

### Task 30: Integration Testing & Balancing

- [ ] **Step 1: Playtest loop**

Запустить игру и проверить:
- [ ] Слайм спавнится, патрулирует, атакует при приближении
- [ ] Стрелок стреляет снарядами
- [ ] Летающий преследует в воздухе
- [ ] Урон и нокбэк работают
- [ ] Здоровье отображается на HUD
- [ ] Смерть врагов → дроп лута
- [ ] Смерть игрока → респаун
- [ ] Падение наносит урон
- [ ] Лава наносит урон
- [ ] Кислород 0 → урон
- [ ] Блоки разрушаются с визуальными стадиями
- [ ] Блоки регенерируют HP при прекращении копания
- [ ] Кирки разных уровней копают с разной скоростью
- [ ] Руды появляются на глубине
- [ ] Торговец доступен для взаимодействия
- [ ] Торговля работает (обмен ресурсов)

- [ ] **Step 2: Balance pass**

Подобрать числа:
- HP врагов, урон оружия, скорость атаки
- Скорость копания vs hardness блоков
- Цены торговца
- Spawn rate мобов

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "feat: vertical slice complete — combat, enemies, trader, tools, hazards"
```
