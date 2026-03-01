# Day/Night Cycle — Design

**Date:** 2026-03-01
**Approach:** C — CPU-модуляция sun_color + ambient_min в шейдере

## Summary

Смена дня и ночи через модуляцию цвета/интенсивности солнца в RC lighting pipeline + тинтинг параллакс-фона. Архитектура готова для будущих геймплей-эффектов (мобы, температура, NPC-расписание), но реализуется только визуальная часть.

## Design Decisions

| Вопрос | Решение |
|--------|---------|
| Фазы | 4: рассвет → день → закат → ночь, плавная интерполяция |
| Длительность | Настраиваемая per-world, дефолт ~15 мин |
| Цвета/интенсивность | В конфиге мира (RON) |
| Подземелье | Не затрагивается — солнце не проникает |
| Звёзды/луна | Не реализуются на этом этапе |
| Загрузка мира | Пока стартуем с рассвета, структура готова для offline-прогресса |
| Геймплей-эффекты | Архитектура заложена (events, multipliers), не реализованы |

## Reference: Starbound

- Цикл 10–20 мин, зависит от планеты
- Ночь длиннее дня (~60/40)
- Время per-planet, видно в starchart

---

## Section 1: WorldTime Resource

### Ресурс

```rust
#[derive(Resource)]
pub struct WorldTime {
    /// Нормализованное время: 0.0 = полночь, 0.25 = рассвет, 0.5 = полдень, 0.75 = закат
    pub time_of_day: f32,
    /// Текущая фаза
    pub phase: DayPhase,
    /// Прогресс внутри текущей фазы 0.0..1.0
    pub phase_progress: f32,
    /// Вычисленный цвет солнца (из конфига, интерполяция)
    pub sun_color: Vec3,
    /// Интенсивность солнца 0.0..1.0
    pub sun_intensity: f32,
    /// Минимальный ambient (лунный свет) 0.0..1.0
    pub ambient_min: f32,
    /// Множитель опасности (для будущего спавна мобов)
    pub danger_multiplier: f32,
    /// Множитель температуры (для будущих систем)
    pub temperature_modifier: f32,
}

pub enum DayPhase { Dawn, Day, Sunset, Night }
```

### Конфиг (day_night.config.ron)

```ron
DayNightConfig(
    cycle_duration_secs: 900.0,
    phase_ratios: (dawn: 0.10, day: 0.40, sunset: 0.10, night: 0.40),
    sun_colors: {
        dawn:    (1.0, 0.65, 0.35),
        day:     (1.0, 0.98, 0.90),
        sunset:  (1.0, 0.50, 0.25),
        night:   (0.15, 0.15, 0.35),
    },
    sun_intensities: { dawn: 0.6, day: 1.0, sunset: 0.5, night: 0.0 },
    ambient_min: { dawn: 0.08, day: 0.0, sunset: 0.06, night: 0.04 },
    danger_multipliers: { dawn: 0.5, day: 0.0, sunset: 0.5, night: 1.0 },
    temperature_modifiers: { dawn: -0.1, day: 0.0, sunset: -0.05, night: -0.2 },
)
```

### Система tick_world_time

Каждый кадр: `time_of_day += dt / cycle_duration`, вычисляет phase/phase_progress, интерполирует sun_color/intensity/ambient_min между соседними фазами.

### Сохранение (готово для offline-прогресса)

```rust
#[derive(Serialize, Deserialize)]
pub struct WorldTimeSave {
    pub time_of_day: f32,
    pub last_saved_unix: u64,
}
```

При загрузке: пока `time_of_day = 0.25` (рассвет). В будущем — загрузка из save + расчёт elapsed.

---

## Section 2: RC Lighting Integration

### CPU-сторона (rc_lighting.rs)

`extract_lighting_data()` — заменяем `const SUN_COLOR` на динамическое значение:
```rust
// Было:
const SUN_COLOR: [f32; 3] = [1.0, 0.98, 0.9];
// Стало:
let sun = world_time.sun_color * world_time.sun_intensity;
```

Логика расстановки солнечных эмиттеров не меняется. Ночью `sun_intensity = 0.0` → эмиттеры нулевые → RC каскады естественно затухают.

### GPU-сторона

**RcUniformsGpu** — добавляем:
```rust
pub sun_color: Vec3,
pub ambient_min: f32,
```

**radiance_cascades.wgsl** — sky escape из uniform:
```wgsl
// Было: return vec3<f32>(1.0, 0.98, 0.9);
// Стало: return uniforms.sun_color;
```

**rc_finalize.wgsl** — ambient minimum:
```wgsl
let raw = textureSampleLevel(...).rgb * BRIGHTNESS;
let irradiance = max(raw, vec3<f32>(uniforms.ambient_min));
```

### Порядок систем

```
tick_world_time (GameSet::WorldUpdate)
  ↓
extract_lighting_data (.after(GameSet::Camera)) — читает WorldTime
  ↓
prepare_rc_textures (Render) — загружает uniforms
  ↓
RcComputeNode — каскады с новым sun_color
```

---

## Section 3: Parallax Tinting

### Sky colors в конфиге

```ron
sky_colors: {
    dawn:    (0.95, 0.55, 0.35, 1.0),
    day:     (1.0,  1.0,  1.0,  1.0),
    sunset:  (0.90, 0.40, 0.30, 1.0),
    night:   (0.08, 0.08, 0.18, 1.0),
},
```

### Маркер-компонент

```rust
#[derive(Component)]
pub struct ParallaxSkyLayer;
```

Ставится при спавне когда `layer.name == "sky"`.

### Система tint_parallax_layers

- Sky-слой: полный тинт `sprite.color = sky_tint` (RGB)
- Far/near hills: мягкий тинт `Color::WHITE.lerp(sky_tint, 0.5)`
- Alpha остаётся за системой biome transition — конфликтов нет (RGB vs alpha)

Порядок: `tint_parallax_layers` → `parallax_transition` (transition перезаписывает alpha).

---

## Section 4: Extensibility

### Event-система

```rust
#[derive(Event)]
pub struct DayPhaseChanged {
    pub previous: DayPhase,
    pub current: DayPhase,
    pub time_of_day: f32,
}
```

`tick_world_time` отправляет при смене фазы. Будущие подписчики:
- Спавн мобов: `Night → ночные мобы`
- NPC: `Dawn → идут в магазин`
- Звуки: `Night → сверчки`

### WorldTime как шина данных

`danger_multiplier`, `temperature_modifier` вычисляются каждый кадр, но пока никем не читаются. Zero-cost если не используется.

### Конфиг per-world

Каждый мир имеет свой `day_night.config.ron`. Разные планеты — разные циклы.

### Debug UI

- Текущая фаза + time_of_day
- Слайдер ручной установки времени
- Чекбокс паузы времени

---

## Files to Create/Modify

**Create:**
- `src/world/day_night.rs` — WorldTime, DayNightConfig, DayPhase, DayPhaseChanged, tick_world_time, tint_parallax_layers
- `assets/world/day_night.config.ron` — дефолтный конфиг

**Modify:**
- `src/world/mod.rs` — регистрация модуля, плагина, систем
- `src/world/rc_lighting.rs` — заменить const SUN_COLOR на WorldTime
- `src/world/rc_pipeline.rs` — добавить sun_color/ambient_min в RcUniformsGpu
- `assets/shaders/radiance_cascades.wgsl` — sky escape из uniform
- `assets/shaders/rc_finalize.wgsl` — ambient_min clamp
- `src/parallax/spawn.rs` — добавить ParallaxSkyLayer маркер
- `src/ui/debug.rs` (или аналог) — debug-панель времени
