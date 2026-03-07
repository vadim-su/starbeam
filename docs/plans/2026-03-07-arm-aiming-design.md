# Arm Aiming System Design

## Overview

Руки персонажа и направление (лево/право) следуют за курсором мыши. Когда в активном слоте хотбара есть предмет, руки вращаются к курсору вокруг плеча. Без предмета — обычная покадровая анимация.

## Поведение

- **Aiming mode ON** (предмет в активном слоте хотбара): front_arm и back_arm вращаются к курсору. Покадровая анимация рук останавливается, используется idle frame 0. Rotation через `Transform.rotation` вокруг Z-оси.
- **Aiming mode OFF** (пустой слот): обычная покадровая анимация рук (существующее поведение).
- **Facing direction**: курсор справа от игрока = лицом вправо, курсор слева = лицом влево. Перебивает текущую логику (по velocity) когда aiming активен.

## Компоненты

### `ArmAiming` (на front_arm и back_arm child entities)

```rust
#[derive(Component)]
pub struct ArmAiming {
    pub active: bool,
    pub angle: f32,          // радианы, текущий угол к курсору
    pub pivot: Vec2,         // смещение плеча от центра спрайта (px)
}
```

## Система: `arm_aiming_system`

1. Получает позицию курсора в мировых координатах (camera transform + window cursor)
2. Проверяет активный слот хотбара — если предмет есть, `active = true`
3. Вычисляет угол: `atan2(cursor.y - player.y, cursor.x - player.x)`
4. Устанавливает `Transform.rotation = Quat::from_rotation_z(angle)` на руках
5. Когда aiming неактивен — сбрасывает rotation в identity

### Facing override

Когда aiming активен, facing определяется позицией курсора (не velocity):
- `cursor.x > player.x` → face right
- `cursor.x < player.x` → face left

Это применяется ко ВСЕМ частям (head, body, front_arm, back_arm).

## Порядок систем

```
player_input → arm_aiming_system → animate_player
```

`animate_player` обновляет facing и frames. `arm_aiming_system`:
- Перезаписывает facing на всех частях когда aiming активен
- Устанавливает rotation на руках
- Устанавливает спрайт рук на idle frame 0 (override анимации)

## Pivot point

Плечо — `Vec2(0.0, 5.0)` (5px выше центра спрайта на 48x48 canvas). Захардкожено для начала, потом можно вынести в RON конфиг.

## Определение "предмет в руках"

Используем `Hotbar` компонент. Если `hotbar.active_slot()` содержит предмет (не пустой) — aiming mode включён.

## Файлы

- `src/player/parts.rs` — добавить `ArmAiming` компонент
- `src/player/aiming.rs` — новый модуль с `arm_aiming_system`
- `src/player/mod.rs` — регистрация модуля и системы
- `src/player/animation.rs` — пропускать frame update для рук когда aiming активен
