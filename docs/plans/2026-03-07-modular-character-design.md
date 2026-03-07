# Modular Character System Design

## Overview

Модульная система персонажа в стиле Starbound: персонаж собирается из отдельных частей тела (head, body, front_arm, back_arm), каждая часть — отдельный спрайтлист. Гибридная анимация: тело и голова покадрово, руки покадрово + вращение при держании оружия.

## Размеры

- Блок (тайл): 8x8 пиксель-арт пикселей (рендерится как 32x32)
- Персонаж: 16x32 px (2 блока шириной, 4 блока высотой)
- Canvas фрейма: ~24x40 px (запас на анимацию)
- Коллайдер: 16x32 px

## Анатомия — 4 слоя

| Слой | Файл | Описание | Z-order |
|------|-------|----------|---------|
| back_arm | `back_arm.png` | Задняя рука (за телом) | -0.02 |
| body | `body.png` | Торс + ноги (единый спрайт) | -0.01 |
| head | `head.png` | Голова + волосы | 0.0 |
| front_arm | `front_arm.png` | Передняя рука (перед телом) | +0.01 |

## Анимации

Все части синхронизированы покадрово:

- **Idle**: 2-4 фрейма (легкое покачивание/дыхание)
- **Run**: 6-8 фреймов (цикл бега)
- **Jump**: 3-5 фреймов (взлет, пик, падение)

### Вращение рук (гибрид)

При держании оружия/инструмента:
- front_arm переключается на специальный "holding" фрейм
- Фрейм вращается вокруг pivot-точки (плечо) в сторону курсора
- Оружие — child entity руки, наследует её вращение
- back_arm может оставаться в покадровой анимации или тоже переключаться на holding

## Архитектура в Bevy

### Entity hierarchy

```
Player (entity) — маркер Player, физика, коллайдер
├── PlayerBody (child) — body spritesheet, LitSpriteMaterial
├── PlayerHead (child) — head spritesheet, LitSpriteMaterial
├── PlayerBackArm (child) — back_arm spritesheet, LitSpriteMaterial
└── PlayerFrontArm (child) — front_arm spritesheet, LitSpriteMaterial
    └── HeldItem (child) — оружие/инструмент
```

### Компоненты

- `CharacterPart(PartType)` — маркер с типом: Head, Body, FrontArm, BackArm
- `PartAnimation` — текущая анимация и фрейм для конкретной части
- `ArmAiming` — на front_arm: целевой угол (к курсору), pivot offset, активен ли aiming
- `CharacterAppearance` — на Player: какие спрайтлисты использовать для каждой части

### Системы

1. **animation_state_system** — определяет AnimationState (Idle/Run/Jump) по velocity/grounded (уже есть)
2. **part_animation_sync** — синхронизирует фреймы всех частей по текущему AnimationState
3. **arm_aiming_system** — при активном ArmAiming вращает front_arm к курсору, иначе использует покадровую анимацию
4. **part_flip_system** — зеркалит все части при смене направления

### Рендеринг

- Все части используют `LitSpriteMaterial` (поддержка radiance cascades)
- Z-ordering через `Transform.translation.z`
- Parent-child relationship для позиционирования

## Спрайтлисты

Формат каждого спрайтлиста — горизонтальный strip:

```
body.png: [idle_0][idle_1][idle_2][idle_3][run_0][run_1]...[jump_0]...
head.png: [idle_0][idle_1][idle_2][idle_3][run_0][run_1]...[jump_0]...
front_arm.png: [idle_0][idle_1]...[run_0]...[jump_0]...[holding_0]
back_arm.png:  [idle_0][idle_1]...[run_0]...[jump_0]...
```

Все спрайтлисты используют одинаковый размер фрейма и одинаковый порядок анимаций.

## RON конфигурация

Расширение `adventurer.character.ron`:

```ron
CharacterParts(
    body: (
        spritesheet: "sprites/body.png",
        frame_size: (24, 40),
        offset: (0, 0),
    ),
    head: (
        spritesheet: "sprites/head.png",
        frame_size: (24, 40),
        offset: (0, 8),
    ),
    front_arm: (
        spritesheet: "sprites/front_arm.png",
        frame_size: (24, 40),
        offset: (2, 2),
        pivot: (4, 2),  // shoulder pivot for rotation
    ),
    back_arm: (
        spritesheet: "sprites/back_arm.png",
        frame_size: (24, 40),
        offset: (-2, 2),
        pivot: (4, 2),
    ),
)
```

## Создание ассетов

1. **PixelLab как референс** — генерируем цельного персонажа (side view, ~48px) с анимациями walk/idle/jump для определения стиля, пропорций и палитры
2. **Ручная разбивка** — по референсу рисуем 4 отдельных спрайтлиста в Aseprite/Pixelorama
3. **Anchor points** — задаем в RON-конфигурации

## Экипировка (будущее)

- Дополнительные child entities поверх соответствующих частей
- Helmet → поверх head, chest armor → поверх body
- Тот же набор фреймов анимаций, другие спрайтлисты
- Armor spritesheet следует тем же правилам позиционирования

## Миграция с текущей системы

Текущая система: один entity с цельными 44x44 спрайтами.
Новая система: 4 child entities с отдельными спрайтлистами.

Изменения затронут:
- `src/player/mod.rs` — spawn_player: создание 4 child entities
- `src/player/animation.rs` — синхронизация частей вместо одного спрайта
- `src/registry/player.rs` — CharacterParts в PlayerConfig
- `assets/content/characters/adventurer/` — новая структура спрайтов
