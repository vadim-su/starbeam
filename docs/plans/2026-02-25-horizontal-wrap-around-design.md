# Горизонтальный Wrap-Around — Дизайн

## Обзор

Горизонтальная закольцовка карты: при движении вправо за край мира (тайл 2047) игрок появляется слева (тайл 0) и наоборот. Бесшовный визуальный переход — у края видны чанки с другой стороны мира. Вертикально мир по-прежнему ограничен (дно — Stone, верх — Air/космос).

## Подход

**Wrap на уровне данных.** Все тайловые X-координаты нормализуются через `rem_euclid(WORLD_WIDTH_TILES)`. Игрок физически всегда в `[0, world_width_pixels)`. Камера следует за игроком. У краёв — загружаем чанки "с той стороны" на нужной display-позиции.

Альтернатива (призрачные дублирующие чанки) отклонена — больше кода, дублирование entity, усложняет block interaction.

## Затрагиваемые системы

7 систем: terrain_gen, WorldMap, chunk_loading, collision, camera_follow, block_interaction, player position.

## Детали

### 1. Хелпер `wrap_tile_x`

```rust
pub fn wrap_tile_x(tile_x: i32) -> i32 {
    tile_x.rem_euclid(WORLD_WIDTH_TILES)
}
```

Центральная функция нормализации. Все системы, работающие с тайловыми X-координатами, прогоняют их через неё.

### 2. Генерация террейна — цилиндрический noise

Сэмплируем Perlin по окружности в 2D-пространстве noise для бесшовного стыка:

```rust
let angle = tile_x as f64 / WORLD_WIDTH_TILES as f64 * 2.0 * PI;
let radius = WORLD_WIDTH_TILES as f64 * FREQUENCY / (2.0 * PI);
let nx = radius * angle.cos();
let ny = radius * angle.sin();
perlin.get([nx, ny])
```

Применяется к surface_height (surface noise) и cave noise (с добавлением tile_y для 2D cave sampling).

Меняет генерацию мира — существующие данные невалидны (у нас нет сохранений, не проблема).

### 3. WorldMap — wrap X, ограничен Y

```rust
pub fn get_tile(&mut self, tile_x: i32, tile_y: i32) -> TileType {
    if tile_y < 0 { return TileType::Stone; }
    if tile_y >= WORLD_HEIGHT_TILES { return TileType::Air; }
    let wrapped_x = wrap_tile_x(tile_x);
    // ... нормальный доступ по (wrapped_x, tile_y)
}
```

Аналогично set_tile, is_solid. Граница по X исчезает.

### 4. Chunk loading — display position ≠ data coords

chunk_loading_system оборачивает chunk X:

```rust
let wrapped_cx = cx.rem_euclid(WORLD_WIDTH_CHUNKS);
```

При спавне чанка: данные берутся из `(wrapped_cx, cy)`, но Transform ставится исходя из незавёрнутой позиции рядом с камерой. Ключ в LoadedChunks — `(display_cx, cy)`, не `(data_cx, cy)`, чтобы один data-чанк мог отображаться на двух позициях (у обоих краёв мира).

### 5. Телепорт игрока

Отдельная система `player_wrap_system`, после collision_system:

```rust
let world_w = WORLD_WIDTH_TILES as f32 * TILE_SIZE;
if pos.x < 0.0 { pos.x += world_w; }
if pos.x >= world_w { pos.x -= world_w; }
```

### 6. Камера

Горизонтальный кламп убирается — камера свободно следует за игроком по X. Вертикальный кламп остаётся. При телепорте игрока камера прыгает вместе (1 кадр, незаметно).

### 7. Block interaction — wrap расстояния

Проверка reach с учётом кольцевого расстояния:

```rust
let mut dx = (tile_x - player_tile_x).abs();
dx = dx.min(WORLD_WIDTH_TILES as f32 - dx);
```

Остальная логика (set_tile, обновление тайлмапа) работает через wrap автоматически.
