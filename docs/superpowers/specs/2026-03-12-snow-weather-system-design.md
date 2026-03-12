# Snow & Weather System Design

## Overview

A weather system that adds snowfall as both a biome feature (snowy biomes with permanent snow) and a dynamic weather event (temperature-driven snowfall on any biome). Purely cosmetic — no gameplay impact on tiles or physics.

## Module Structure

New module: `src/weather/`

| File | Responsibility |
|---|---|
| `mod.rs` | `WeatherPlugin`, system registration, runs in `GameSet::WorldUpdate` + `AppState::InGame` |
| `wind.rs` | `Wind` resource, smooth direction/strength changes over time |
| `snow_particles.rs` | Dedicated snow particle pool, spawning, rendering |
| `snow_overlay.rs` | Snow cap overlays on exposed surface blocks |
| `weather_state.rs` | `WeatherState` resource, timers, temperature-based probability |

## Wind System (`wind.rs`)

### `Wind` Resource
- `direction: f32` — angle in radians
- `strength: f32` — 0.0 to 1.0
- `target_direction: f32` — lerp target
- `target_strength: f32` — lerp target
- `change_timer: Timer` — 5-15 seconds between target changes

### `update_wind()` System
- On timer expiry: pick new `target_direction` (±60° from current) and `target_strength`
- Each frame: lerp `direction` and `strength` toward targets with small coefficient (~0.02)
- Result: wind slowly drifts, creating natural feel

### Wind Effect on Particles
- Added to each particle's velocity:
  - X: `cos(direction) * strength * MAX_WIND_SPEED`
  - Y: `sin(direction) * strength * MAX_WIND_SPEED * 0.3` (horizontal dominates)

## Snow Particle Pool (`snow_particles.rs`)

### `SnowParticlePool` Resource
- Capacity: 1500 particles (separate from game `ParticlePool`)
- Same ring-buffer architecture as existing `ParticlePool`
- Dedicated `SnowMeshEntity` marker, `SharedSnowMaterial` resource, and `rebuild_snow_mesh()` system (mirrors `ParticlePool` render pipeline pattern from `src/particles/render.rs`)

### Particle Properties
- **Size:** random 1-4px
- **Fall speed:** constant downward velocity per particle (no gravity acceleration). Large flakes = 40-80 px/s (slow, close feel), small flakes = 80-120 px/s (fast, distant feel) — creates parallax depth illusion. Gravity scale set to 0.0; velocity stays constant.
- **Color:** white, alpha variation 0.6-1.0, larger flakes slightly more transparent
- **Wobble:** per-particle sine wave on X axis (random phase, small amplitude) layered on top of wind
- **Lifetime:** dies when hitting a solid tile, exiting below camera bottom, or after 8-12 seconds
- **Solid tile collision:** each frame, check if particle position overlaps a solid tile in the foreground layer. If so, kill the particle. This prevents snow from visually clipping through terrain.

### Spawning
- Spawn zone: strip above camera top (camera.top + 16..48px), full camera width
- Spawn rate: 30-80 particles/sec, scaled by `Snowing.intensity`
- Only active when `WeatherState` is `Snowing`
- On non-wrapping worlds near world edges, clamp spawn X range to valid world bounds

### Rendering
- Separate batched mesh (same technique as game particles: `SnowMeshEntity` + `rebuild_snow_mesh()`)
- Z = 3.0 (above tiles and objects, below liquid layer at 2.0... actually above liquid too, below UI). Note: existing Z layers are bg=-1, fg=0, overlays=0.05-0.15, objects=0.5, particles=1.0, liquid=2.0. Snow at Z=3.0 renders above all world content.
- Per-vertex colors with alpha

### Intensity Ramp
- When `Snowing` begins: intensity ramps from 0 to target over 5 seconds (spawn rate scales linearly)
- When `Snowing` ends: intensity ramps from current to 0 over 5 seconds before transitioning to `Clear`
- Avoids abrupt particle appearance/disappearance

## Snow Overlay (`snow_overlay.rs`)

### `SnowOverlay` Component
- `tile_x: i32`, `tile_y: i32`

### `SnowOverlayTexture` Resource
- Single procedurally generated 16x4px white texture with irregular bottom edge (pixel art style)
- Intentionally 16x4 (not 16x16) — covers only the top quarter of a tile as a thin snow cap
- Created once at plugin startup

### Overlay Placement
- Sprite positioned at block position + ~6px vertical offset
- Z = 0.05 (just above tile layer at 0.0, below crack overlay at 0.1 so cracks show through snow)

### `update_snow_overlays()` System
- Runs on 0.5 second timer (not every frame)
- Iterates loaded chunks in view
- Candidate: tile is solid AND tile above is air
- **Snowy biome (`snow_permanent: true`):** overlays spawned on chunk load, never removed
- **Weather snow:** overlays appear gradually (random chance per tick while snowing)
- **Melting:** when `temperature_modifier` is high enough (daytime in non-permanent biomes), overlays removed gradually (random chance per tick)
- **Tile modification handling:** subscribe to dirty-chunk events. When a chunk is marked dirty, re-scan affected tiles: remove overlays from destroyed blocks, add overlays to newly exposed surfaces. This piggybacks on the existing `ChunkDirty` mechanism used by mesh rebuilding.
- **Biome boundary transitions:** at boundaries between snowy and non-snowy biomes, apply a 4-tile linear falloff zone where snow overlay probability decreases from 100% to 0%. This softens the hard biome edge.

### Cleanup on Chunk Unload
- Snow overlay entities are parented to the chunk's foreground entity via Bevy's `Parent`/`Children` hierarchy. When `despawn_chunk()` despawns the chunk entity with `despawn_recursive()`, all child overlays are automatically cleaned up. No separate despawn logic needed.

### System Parameters
- Requires: `Res<WorldMap>`, `Res<LoadedChunks>`, `Res<BiomeMap>`, `Res<BiomeRegistry>`, `Res<WeatherState>`, `Res<WorldTime>`, camera `Query`
- To avoid Bevy query conflicts (see commit 4142eb4), overlay queries should use distinct component filters from other systems accessing the same entities.

### World Wrapping
- Use `data_chunk_x` (not `display_chunk_x`) for tile lookups in `WorldMap`
- Use `display_chunk_x` for sprite positioning
- Follows the same convention as `crack_overlay.rs` and `surface_objects.rs`

## Weather State (`weather_state.rs`)

### `WeatherKind` Enum
```rust
enum WeatherKind {
    Clear,
    Snowing {
        intensity: f32,       // 0.0-1.0, affects spawn rate
        target_intensity: f32, // what intensity ramps toward
        elapsed: f32,         // seconds since start
        duration: f32,        // total duration (30-120 sec)
    },
}
```

### `WeatherState` Resource
- `current: WeatherKind`
- `cooldown_timer: Timer` — 60-180 seconds between snowfalls
- `check_timer: Timer` — ~5 seconds between probability rolls

### `update_weather()` System
- Every `check_timer` tick, roll for snowfall:
  - Probability: `biome.snow_base_chance * (1.0 - world_time.temperature_modifier)`
  - Higher chance = colder temperature, snowy biomes
- On success: transition to `Snowing` with random duration 30-120s and target_intensity 0.5-1.0, intensity starts at 0.0 and ramps up
- When `elapsed >= duration`: begin ramp-down (target_intensity → 0.0), transition to `Clear` when intensity reaches ~0.0, then start cooldown
- **Biome determination:** uses the biome at camera center position. This is a deliberate simplification — weather is a single global state. When the camera straddles a biome boundary, the biome under camera center wins. This avoids complexity of per-region weather for the MVP.

## Biome Integration

### Data Pipeline
New fields must be added to three places:
1. **`BiomeAsset`** (`src/registry/assets.rs`): add `snow_base_chance: f32` and `snow_permanent: bool` with `#[serde(default)]`
2. **`BiomeDef`** (`src/registry/biome.rs`): add matching fields
3. **Loading code** (`src/registry/loading.rs`): map `BiomeAsset` fields to `BiomeDef` fields

The existing `weather: Option<Vec<String>>` field on `BiomeAsset` is reserved for a future general-purpose weather tag system (rain, storms, etc.). The `snow_base_chance` / `snow_permanent` fields are explicit typed fields for snow specifically, not part of the generic `weather` tags. Both can coexist — when the generic weather system is built later, it can reference these fields or subsume them.

### BiomeDef Extension
Two new fields with defaults:
- `snow_base_chance: f32` — default `0.0` (no snow for existing biomes)
- `snow_permanent: bool` — default `false`

### New Biome: Tundra
```ron
(
    id: "tundra",
    surface_block: "snow_dirt",
    subsurface_block: "frozen_dirt",
    subsurface_depth: 4,
    fill_block: "stone",
    cave_threshold: 0.35,
    snow_base_chance: 0.8,
    snow_permanent: true,
    parallax: Some("content/biomes/tundra/tundra.parallax.ron"),
)
```

### New Tiles
- **`snow_dirt`** — white/snowy top layer, dirt bottom (autotile variant needed)
- **`frozen_dirt`** — grey-blue frozen earth

### Tundra Parallax
- Pale blue/grey sky
- Snowy hills (far/near layers)
- Placeholder art initially, proper art as separate task

## Implementation Order
1. `Wind` resource and system
2. `SnowParticlePool` — pool, spawning, rendering (including `SnowMeshEntity`, `rebuild_snow_mesh()`)
3. `WeatherState` — state machine, temperature integration, intensity ramp
4. `SnowOverlay` — cap sprites, placement logic, chunk parenting, dirty-chunk integration
5. `BiomeDef` extension — `BiomeAsset` + `BiomeDef` + `loading.rs` mapping
6. Tundra biome — definition, placeholder tiles, parallax
7. Polish — tuning constants, visual feel, biome boundary transitions
