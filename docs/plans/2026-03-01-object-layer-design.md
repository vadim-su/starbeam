# Object Layer (Placeable Furniture) Design

**Date:** 2026-03-01
**Status:** Approved
**Related:** Inventory and Drops Design, Data-driven Registry Design

## Overview

Система размещаемых объектов (мебель, контейнеры, источники света) в мире. Объекты привязаны к тайловой сетке, могут занимать несколько тайлов, имеют индивидуальные настройки коллизий и правила размещения.

## Ключевые решения

- **Привязка к сетке** — объекты snap к тайловым координатам
- **Мульти-тайл** — объекты могут занимать произвольный прямоугольник (стол 3×2, дверь 1×3)
- **Якорь = bottom-left** — позиция объекта определяется нижним-левым тайлом
- **Per-object коллизии** — solid_mask определяет какие тайлы объекта блокируют движение
- **Гибридное хранение** — данные в ChunkData (для персистенции/генерации), Entity в рантайме (для взаимодействия/рендера)
- **Placement rules** — floor/wall/ceiling/any
- **MVP взаимодействия** — размещение/разрушение, контейнеры (сундуки), источники света

## Architecture

### Новые файлы

```
src/
├── object/
│   ├── mod.rs              # ObjectPlugin
│   ├── definition.rs       # ObjectId, ObjectDef, ObjectType, PlacementRule
│   ├── registry.rs         # ObjectRegistry (Resource)
│   ├── placement.rs        # can_place, place_object, remove_object
│   ├── spawn.rs            # spawn/despawn entities при загрузке чанков
│   └── interaction.rs      # контейнеры, источники света
```

### Изменения в существующих файлах

- `src/world/chunk.rs` — добавить `objects: Vec<PlacedObject>` и `occupancy` в ChunkData
- `src/physics.rs` — проверка коллизий с объектами через occupancy + solid_mask
- `src/interaction/block_action.rs` — размещение/разрушение объектов
- `src/registry/loading.rs` — загрузка ObjectRegistry из RON
- `src/main.rs` — подключение ObjectPlugin

---

## Object Definition

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ObjectId(pub u16);

impl ObjectId {
    pub const NONE: ObjectId = ObjectId(0);
}

#[derive(Debug, Clone, Deserialize)]
pub struct ObjectDef {
    pub id: String,                    // "wooden_table", "torch", "chest"
    pub display_name: String,
    pub size: UVec2,                   // (2,1) для стола, (1,2) для двери
    pub sprite: String,                // путь к спрайту
    pub solid_mask: Vec<bool>,         // какие тайлы solid (len = size.x * size.y)
    pub placement: PlacementRule,
    pub light_emission: [u8; 3],       // (0,0,0) для не-световых
    pub object_type: ObjectType,
    pub drops: Vec<DropDef>,           // что выпадает при разрушении
}
```

`solid_mask` — вектор длиной `size.x * size.y`, row-major (bottom row first). Стол 3×2:
```
Row 1 (top):    [false, false, false]  — столешница проходима
Row 0 (bottom): [true,  false, true ]  — ножки solid
solid_mask = [true, false, true, false, false, false]
```

### PlacementRule

```rust
#[derive(Debug, Clone, Deserialize)]
pub enum PlacementRule {
    Floor,      // solid тайл под каждым нижним тайлом
    Wall,       // solid тайл сбоку (слева или справа)
    Ceiling,    // solid тайл над каждым верхним тайлом
    Any,        // без ограничений
}
```

### ObjectType

```rust
#[derive(Debug, Clone, Deserialize)]
pub enum ObjectType {
    Decoration,
    Container { slots: u16 },
    LightSource,
}
```

---

## Chunk Storage

### PlacedObject

```rust
pub struct PlacedObject {
    pub object_id: ObjectId,
    pub local_x: u32,             // позиция якоря (bottom-left) в чанке
    pub local_y: u32,
    pub state: ObjectState,
}

#[derive(Default)]
pub enum ObjectState {
    #[default]
    Default,
    Container { contents: Vec<Option<InventorySlot>> },
}
```

### Occupancy Grid

Быстрый lookup "что стоит на этом тайле?":

```rust
pub struct OccupancyRef {
    pub object_index: u16,    // индекс в Vec<PlacedObject>
    pub is_anchor: bool,      // true для bottom-left тайла
}
```

`occupancy: Vec<Option<OccupancyRef>>` — размер `chunk_size²`, параллелен `fg.tiles`.

### ChunkData (изменения)

```rust
pub struct ChunkData {
    pub fg: TileLayer,
    pub bg: TileLayer,
    pub objects: Vec<PlacedObject>,              // NEW
    pub occupancy: Vec<Option<OccupancyRef>>,    // NEW
    pub damage: Vec<u8>,
}
```

---

## Runtime Entities

При загрузке чанка каждый PlacedObject спавнится как Entity:

```rust
#[derive(Component)]
pub struct PlacedObjectMarker {
    pub chunk: (i32, i32),     // data chunk coords
    pub index: u16,            // index in ChunkData.objects
}

// Компоненты на Entity:
(
    PlacedObjectMarker { chunk, index },
    Transform,            // world position из anchor tile
    Sprite,               // спрайт объекта (может быть больше 32×32)
    ObjectInteraction,    // тип взаимодействия (если есть)
)
```

Спрайт позиционируется относительно якоря: для объекта size (w, h) offset = `((w-1) * tile_size / 2, (h-1) * tile_size / 2)` чтобы спрайт покрывал все тайлы.

### Chunk Lifecycle

```
Загрузка чанка:
  generate/load ChunkData → rebuild occupancy grid → for each object → spawn Entity

Выгрузка чанка:
  Query Entity с PlacedObjectMarker для этого чанка →
    sync state (Container contents → ChunkData.objects[i].state) →
    despawn Entity
```

---

## Placement Validation

```rust
fn can_place_object(
    world_map: &WorldMap,
    object_def: &ObjectDef,
    anchor_x: i32,
    anchor_y: i32,
    ctx: &WorldCtxRef,
) -> bool {
    let w = object_def.size.x as i32;
    let h = object_def.size.y as i32;

    // 1. Все тайлы в области свободны (fg == AIR, occupancy == None)
    for dy in 0..h {
        for dx in 0..w {
            let tx = anchor_x + dx;
            let ty = anchor_y + dy;
            if world_map.get_tile(tx, ty, Layer::Fg, ctx) != Some(TileId::AIR) {
                return false;
            }
            if get_occupancy(world_map, tx, ty, ctx).is_some() {
                return false;
            }
        }
    }

    // 2. PlacementRule
    match object_def.placement {
        PlacementRule::Floor => {
            // solid тайл под каждым нижним тайлом
            for dx in 0..w {
                if !world_map.is_solid(anchor_x + dx, anchor_y - 1, ctx) {
                    return false;
                }
            }
        }
        PlacementRule::Wall => {
            // solid тайл слева от якоря ИЛИ справа от правого края
            let left_solid = world_map.is_solid(anchor_x - 1, anchor_y, ctx);
            let right_solid = world_map.is_solid(anchor_x + w, anchor_y, ctx);
            if !left_solid && !right_solid {
                return false;
            }
        }
        PlacementRule::Ceiling => {
            // solid тайл над каждым верхним тайлом
            for dx in 0..w {
                if !world_map.is_solid(anchor_x + dx, anchor_y + h, ctx) {
                    return false;
                }
            }
        }
        PlacementRule::Any => {}
    }

    true
}
```

---

## Collision Integration

Добавляется в существующую проверку солидности:

```rust
fn is_solid_at(
    world_map: &WorldMap,
    object_registry: &ObjectRegistry,
    tile_x: i32,
    tile_y: i32,
    ctx: &WorldCtxRef,
) -> bool {
    // Существующая проверка fg-тайлов
    if world_map.is_solid(tile_x, tile_y, ctx) {
        return true;
    }

    // Проверка объектов через occupancy
    if let Some(occ) = get_occupancy(world_map, tile_x, tile_y, ctx) {
        let chunk = get_chunk_for_tile(world_map, tile_x, tile_y, ctx);
        let obj = &chunk.objects[occ.object_index as usize];
        let def = object_registry.get(obj.object_id);

        // Вычислить позицию внутри объекта
        let obj_world_x = chunk_base_x + obj.local_x as i32;
        let obj_world_y = chunk_base_y + obj.local_y as i32;
        let rel_x = (tile_x - obj_world_x) as usize;
        let rel_y = (tile_y - obj_world_y) as usize;
        let mask_idx = rel_y * def.size.x as usize + rel_x;

        return def.solid_mask[mask_idx];
    }

    false
}
```

---

## Light Sources

Объекты с `light_emission != [0, 0, 0]`:
- При спавне Entity → регистрируются в RC-освещении как точечные источники
- При деспавне → убираются из системы освещения
- Позиция источника = центр объекта

---

## Object Registry

Аналогичен TileRegistry, загружается из RON:

```rust
#[derive(Resource)]
pub struct ObjectRegistry {
    defs: Vec<ObjectDef>,
    name_to_id: HashMap<String, ObjectId>,
}
```

### Пример RON

```ron
(
    objects: [
        (
            id: "torch",
            display_name: "Torch",
            size: (1, 1),
            sprite: "objects/torch.png",
            solid_mask: [false],
            placement: Wall,
            light_emission: (240, 180, 80),
            object_type: LightSource,
            drops: [(item_id: "torch", min: 1, max: 1, chance: 1.0)],
        ),
        (
            id: "wooden_chest",
            display_name: "Wooden Chest",
            size: (2, 1),
            sprite: "objects/wooden_chest.png",
            solid_mask: [true, true],
            placement: Floor,
            light_emission: (0, 0, 0),
            object_type: Container(slots: 16),
            drops: [(item_id: "wooden_chest", min: 1, max: 1, chance: 1.0)],
        ),
        (
            id: "wooden_table",
            display_name: "Wooden Table",
            size: (3, 2),
            sprite: "objects/wooden_table.png",
            solid_mask: [true, false, true, false, false, false],
            placement: Floor,
            light_emission: (0, 0, 0),
            object_type: Decoration,
            drops: [(item_id: "wooden_table", min: 1, max: 1, chance: 1.0)],
        ),
    ],
)
```

---

## Cross-Chunk Objects

Мульти-тайловый объект может пересекать границу чанков. Подход:

- Объект хранится в чанке **якоря** (bottom-left тайл)
- Occupancy записывается во **все** чанки, которые объект занимает
- OccupancyRef ссылается на `(chunk_coord, object_index)` для кросс-чанковых случаев
- При деспавне чанка якоря — деспавнится Entity, occupancy в соседних чанках очищается

---

## Implementation Phases

| Phase | What | Est. |
|-------|------|------|
| 1. ObjectDef + Registry | definition.rs, registry.rs, RON loading | 1 day |
| 2. ChunkData extension | PlacedObject, occupancy grid, ChunkData changes | 1 day |
| 3. Spawn/Despawn | Entity lifecycle привязанный к чанкам | 1-2 days |
| 4. Placement | can_place, place_object, remove_object + interaction | 1-2 days |
| 5. Collisions | Интеграция с физикой через occupancy + solid_mask | 1 day |
| 6. Containers | ObjectState::Container, UI для сундуков | 2 days |
| 7. Light sources | Интеграция с RC-освещением | 1 day |

**Total:** ~8-10 days
