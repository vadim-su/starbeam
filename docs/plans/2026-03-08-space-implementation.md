# Space & Ship World Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement space as a playable world — ship as a buildable tile-based world, capsule transport between planet and ship, autopilot navigation between celestial bodies with real-time travel, EVA in open space with zero-gravity and oxygen, flood-fill pressurization system, and space biomes for parallax backgrounds.

**Architecture:** Ship is a `CelestialAddress::Ship` world using the existing tilemap/chunk/persistence infrastructure. `ActiveWorld` gains a `wrap_x: bool` field to conditionally enable/disable horizontal wrap-around. A new "ship" planet type generates an empty world with a starter hull. Functional blocks (autopilot console, fuel tank, airlock) are new `ObjectType` variants. Pressurization uses flood-fill from world edges, cached per-chunk. Space biomes (`deep_space`, `orbit_*`) drive parallax backgrounds via a global biome override.

**Tech Stack:** Bevy 0.18, existing chunk/warp/parallax/object systems, RON assets, Perlin noise (not needed for empty ship gen)

---

## Task 1: Add `wrap_x` to ActiveWorld

Add a `wrap_x: bool` field to `ActiveWorld`. Make `wrap_tile_x` / `wrap_chunk_x` conditional. Update all callers.

**Files:**
- Modify: `src/registry/world.rs`
- Modify: `src/world/terrain_gen.rs` (surface_height cylindrical noise — only when wrap_x)
- Modify: `src/camera/follow.rs` (horizontal clamping when no wrap)
- Modify: `src/world/chunk.rs` (chunk_loading_system wrap logic)
- Modify: `src/parallax/scroll.rs` (parallax wrap)

**Step 1: Write tests for conditional wrap**

In `src/registry/world.rs`, add tests:

```rust
#[test]
fn wrap_tile_x_disabled() {
    let mut c = test_config();
    c.wrap_x = false;
    // Without wrap, coordinates should NOT be wrapped
    assert_eq!(c.wrap_tile_x(-1), -1);
    assert_eq!(c.wrap_tile_x(2048), 2048);
}

#[test]
fn wrap_tile_x_enabled() {
    let mut c = test_config();
    c.wrap_x = true;
    assert_eq!(c.wrap_tile_x(-1), 2047);
    assert_eq!(c.wrap_tile_x(2048), 0);
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib registry::world::tests -- --nocapture`
Expected: FAIL — `wrap_x` field doesn't exist

**Step 3: Add `wrap_x` field and update methods**

In `src/registry/world.rs`:

```rust
#[derive(Resource, Debug, Clone)]
pub struct ActiveWorld {
    pub address: CelestialAddress,
    pub seeds: CelestialSeeds,
    pub width_tiles: i32,
    pub height_tiles: i32,
    pub chunk_size: u32,
    pub tile_size: f32,
    pub chunk_load_radius: i32,
    pub seed: u32,
    pub planet_type: String,
    pub wrap_x: bool,
}

impl ActiveWorld {
    pub fn wrap_tile_x(&self, tile_x: i32) -> i32 {
        if self.wrap_x {
            tile_x.rem_euclid(self.width_tiles)
        } else {
            tile_x
        }
    }

    pub fn wrap_chunk_x(&self, chunk_x: i32) -> i32 {
        if self.wrap_x {
            chunk_x.rem_euclid(self.width_chunks())
        } else {
            chunk_x
        }
    }
}
```

Add `wrap_x: true` to `test_config()` in tests.

**Step 4: Fix all compilation errors**

Every place that constructs `ActiveWorld` needs `wrap_x` added. Key locations:
- `src/cosmos/warp.rs` — in `handle_warp()` where ActiveWorld is rebuilt
- `src/registry/loading.rs` — initial world construction
- `src/test_helpers.rs` — test fixtures

Set `wrap_x: true` for all existing planets (preserves current behavior).

**Step 5: Update terrain_gen for non-wrapping worlds**

In `src/world/terrain_gen.rs`, `surface_height()`:
- When `wc.wrap_x == true`: use existing cylindrical noise (angle-based)
- When `wc.wrap_x == false`: use flat Perlin (just `tile_x * frequency`)

Also update `generate_tile()` — the `wc.wrap_tile_x(tile_x)` call already uses the conditional method, so no change needed there. But for non-wrap worlds, tiles outside `[0, width)` should return AIR instead of wrapping.

**Step 6: Update camera for non-wrapping worlds**

In `src/camera/follow.rs`:
- When `!world_config.wrap_x`: clamp camera X to `[half_w, world_w - half_w]` (same pattern as Y clamping)
- When `world_config.wrap_x`: current behavior (no X clamp)

**Step 7: Run all tests**

Run: `cargo test --lib`
Expected: All pass

**Step 8: Commit**

```
feat(world): add wrap_x toggle to ActiveWorld
```

---

## Task 2: Ship planet type and empty terrain generator

Create a "ship" planet type that generates an empty world with a minimal starter hull.

**Files:**
- Create: `assets/worlds/planet_types/ship/ship.planet.ron`
- Modify: `src/world/terrain_gen.rs` — handle "ship" planet type (all AIR)
- Modify: `src/cosmos/generation.rs` — support Ship address in body generation

**Step 1: Create ship planet type asset**

```ron
// assets/worlds/planet_types/ship/ship.planet.ron
(
    id: "ship",
    primary_biome: "deep_space",
    secondary_biomes: [],
    layers: (
        surface: (
            primary_biome: Some("deep_space"),
            terrain_frequency: 0.0,
            terrain_amplitude: 0.0,
            depth_ratio: 1.0,
        ),
        underground: (
            primary_biome: Some("deep_space"),
            terrain_frequency: 0.0,
            terrain_amplitude: 0.0,
            depth_ratio: 0.0,
        ),
        deep_underground: (
            primary_biome: Some("deep_space"),
            terrain_frequency: 0.0,
            terrain_amplitude: 0.0,
            depth_ratio: 0.0,
        ),
        core: (
            primary_biome: Some("deep_space"),
            terrain_frequency: 0.0,
            terrain_amplitude: 0.0,
            depth_ratio: 0.0,
        ),
    ),
    region_width_min: 9999,
    region_width_max: 9999,
    primary_region_ratio: 1.0,
    size: Some((128, 64)),
    wrap_x: Some(false),
)
```

**Step 2: Add `wrap_x` to PlanetTypeAsset**

In `src/registry/assets.rs`, add to `PlanetTypeAsset`:

```rust
#[serde(default)]
pub wrap_x: Option<bool>,
```

Default is `None` which means `true` (planets wrap by default).

**Step 3: Wire wrap_x from PlanetTypeAsset into ActiveWorld**

In `src/cosmos/warp.rs` and `src/registry/loading.rs`, when constructing `ActiveWorld`, read `planet_config.wrap_x.unwrap_or(true)`.

**Step 4: Handle empty generation for ship**

In `src/world/terrain_gen.rs`, when `terrain_amplitude == 0.0` and `terrain_frequency == 0.0`, `surface_height` should return a value that makes the entire world AIR. Simplest: if amplitude is 0, return -1 (below any valid tile_y), so all tiles are "above surface" = AIR.

**Step 5: Generate starter hull**

Create a system that runs after initial chunk loading for ship worlds. Place a small rectangle of blocks (floor, walls, ceiling) at the center of the ship world, plus the three functional blocks (autopilot, fuel tank, airlock). This will be implemented in Task 6 after functional blocks exist.

**Step 6: Run tests**

Run: `cargo test --lib`
Expected: All pass

**Step 7: Commit**

```
feat(ship): add ship planet type with empty world generation
```

---

## Task 3: Deep space biome and space parallax

Create the `deep_space` biome with space-themed parallax backgrounds.

**Files:**
- Create: `assets/content/biomes/deep_space/deep_space.biome.ron`
- Create: `assets/content/biomes/deep_space/deep_space.parallax.ron`
- Create: `assets/content/biomes/deep_space/backgrounds/` (placeholder PNGs)
- Create: `assets/content/biomes/orbit_garden/` (same structure)
- Create: `assets/content/biomes/orbit_barren/` (same structure)

**Step 1: Create deep_space biome definition**

```ron
// assets/content/biomes/deep_space/deep_space.biome.ron
(
    id: "deep_space",
    surface_block: "stone",
    subsurface_block: "stone",
    subsurface_depth: 0,
    fill_block: "stone",
    cave_threshold: 0.0,
    parallax: Some("content/biomes/deep_space/deep_space.parallax.ron"),
)
```

Note: surface/fill blocks are irrelevant for ship worlds (all AIR), but the schema requires them.

**Step 2: Create deep_space parallax config**

```ron
// assets/content/biomes/deep_space/deep_space.parallax.ron
(
    layers: [
        (
            name: "stars_far",
            image: "content/biomes/deep_space/backgrounds/stars_far.png",
            speed_x: 0.02,
            speed_y: 0.02,
            repeat_x: true,
            repeat_y: true,
            z_order: -100.0,
        ),
        (
            name: "stars_near",
            image: "content/biomes/deep_space/backgrounds/stars_near.png",
            speed_x: 0.05,
            speed_y: 0.05,
            repeat_x: true,
            repeat_y: true,
            z_order: -90.0,
        ),
    ],
)
```

**Step 3: Create placeholder background PNGs**

Generate simple star-field images (or place 1x1 black placeholder PNGs for now — art can come later).

**Step 4: Create orbit biomes**

Same pattern for `orbit_garden` and `orbit_barren` — different parallax layers showing the planet surface below.

**Step 5: Test loading**

Run the game, warp to ship world, verify parallax backgrounds load.

**Step 6: Commit**

```
feat(biomes): add deep_space and orbit biomes with parallax
```

---

## Task 4: Global biome override for ship worlds

Currently biomes are determined by horizontal position via `BiomeMap`. Ship worlds need a single global biome for the entire world that can change at runtime (when ship moves between locations).

**Files:**
- Modify: `src/parallax/transition.rs` — check for global biome override
- Create: `src/cosmos/ship_location.rs` — track ship's current location and drive biome changes
- Modify: `src/cosmos/mod.rs` — register new module

**Step 1: Add GlobalBiome resource**

```rust
// In src/cosmos/ship_location.rs
use bevy::prelude::*;
use crate::registry::biome::BiomeId;

/// When present, overrides biome detection for the entire world.
/// Used for ship worlds where the biome represents the ship's location.
#[derive(Resource, Debug)]
pub struct GlobalBiome {
    pub biome_id: BiomeId,
}

/// Tracks the ship's current orbital location.
#[derive(Resource, Debug, Clone)]
pub enum ShipLocation {
    Orbit(CelestialAddress),
    InTransit {
        from: CelestialAddress,
        to: CelestialAddress,
        progress: f32,
        duration: f32,
    },
}
```

**Step 2: Modify track_player_biome to respect GlobalBiome**

In `src/parallax/transition.rs`, `track_player_biome()`:

```rust
// At the top of the function, before biome_map lookup:
if let Some(global) = global_biome {
    // Use global biome instead of position-based lookup
    new_biome = global.biome_id.clone();
    // Skip biome_map.biome_at() entirely
}
```

Add `global_biome: Option<Res<GlobalBiome>>` parameter to the system.

**Step 3: Insert GlobalBiome during warp to ship**

In `src/cosmos/warp.rs`, when warping to a Ship address:
- Insert `GlobalBiome { biome_id: "deep_space" }` (or orbit biome based on ship location)
- When warping away from ship, remove `GlobalBiome` resource

**Step 4: Test**

Warp to ship → verify global biome drives parallax instead of position.

**Step 5: Commit**

```
feat(ship): add global biome override for ship worlds
```

---

## Task 5: Functional block types (Autopilot, Fuel Tank, Airlock)

Add three new `ObjectType` variants for ship-specific functional blocks.

**Files:**
- Modify: `src/object/definition.rs` — add ObjectType variants
- Create: `assets/content/objects/autopilot_console/autopilot_console.object.ron`
- Create: `assets/content/objects/fuel_tank/fuel_tank.object.ron`
- Create: `assets/content/objects/airlock/airlock.object.ron`
- Create placeholder sprites for each

**Step 1: Add ObjectType variants**

In `src/object/definition.rs`:

```rust
pub enum ObjectType {
    Decoration,
    Container { slots: u16 },
    LightSource,
    CraftingStation { station_id: String },
    AutopilotConsole,
    FuelTank { capacity: f32 },
    Airlock,
}
```

**Step 2: Create object RON definitions**

```ron
// assets/content/objects/autopilot_console/autopilot_console.object.ron
(
    id: "autopilot_console",
    display_name: "Autopilot Console",
    size: (2, 2),
    sprite: "content/objects/autopilot_console/autopilot_console.png",
    placement: Floor,
    object_type: AutopilotConsole,
    auto_item: Some((
        description: "Navigate between celestial bodies",
        max_stack: 1,
    )),
)
```

```ron
// assets/content/objects/fuel_tank/fuel_tank.object.ron
(
    id: "fuel_tank",
    display_name: "Fuel Tank",
    size: (2, 3),
    sprite: "content/objects/fuel_tank/fuel_tank.png",
    placement: Floor,
    object_type: FuelTank(capacity: 100.0),
    auto_item: Some((
        description: "Stores fuel for interplanetary travel",
        max_stack: 1,
    )),
)
```

```ron
// assets/content/objects/airlock/airlock.object.ron
(
    id: "airlock",
    display_name: "Airlock",
    size: (2, 3),
    sprite: "content/objects/airlock/airlock.png",
    placement: Floor,
    object_type: Airlock,
    auto_item: Some((
        description: "Descend to planet surface or return to ship",
        max_stack: 1,
    )),
)
```

**Step 3: Create placeholder sprites**

Simple colored rectangles as placeholders (32x64 for 2x2, 32x96 for 2x3 at tile_size 8px — actually sprites use pixel art scale, check existing sprite sizes for reference).

**Step 4: Run tests**

Run: `cargo test --lib`
Expected: All pass (ObjectDef parsing, etc.)

**Step 5: Commit**

```
feat(objects): add autopilot console, fuel tank, and airlock blocks
```

---

## Task 6: Ship starter hull generation

When a ship world is first loaded, generate a small starter structure with functional blocks.

**Files:**
- Create: `src/cosmos/ship_hull.rs` — starter hull generation
- Modify: `src/cosmos/mod.rs` — register module
- Modify: `src/world/chunk.rs` or `src/registry/loading.rs` — trigger hull gen on first ship load

**Step 1: Design starter hull**

Small rectangle (e.g. 16 wide × 8 tall tiles) centered in the ship world:
- Bottom row: floor blocks (foreground)
- Top row: ceiling blocks
- Left/right columns: wall blocks
- Interior: AIR
- Background layer: filled with wall tiles (like Starbound ship interior)
- Place autopilot console at center
- Place fuel tank near left wall
- Place airlock at right wall

**Step 2: Implement hull generator**

```rust
// src/cosmos/ship_hull.rs
pub fn generate_starter_hull(
    world_map: &mut WorldMap,
    object_registry: &ObjectRegistry,
    ctx: &WorldCtxRef,
) {
    let center_x = ctx.config.width_tiles / 2;
    let center_y = ctx.config.height_tiles / 2;
    let hull_w = 16;
    let hull_h = 8;
    let left = center_x - hull_w / 2;
    let bottom = center_y - hull_h / 2;

    // Place hull blocks
    for x in left..left + hull_w {
        for y in bottom..bottom + hull_h {
            let is_border = x == left || x == left + hull_w - 1
                         || y == bottom || y == bottom + hull_h - 1;
            let tile = if is_border { stone_id } else { TileId::AIR };
            // Set foreground tile
            set_tile(world_map, x, y, Layer::Fg, tile, ctx);
            // Set background for entire hull area
            set_tile(world_map, x, y, Layer::Bg, stone_id, ctx);
        }
    }

    // Place functional blocks (using object placement system)
    // Autopilot at center, fuel tank left, airlock right
}
```

**Step 3: Trigger on first ship load**

Check if ship world has been generated before (check persistence). If not, run hull generator after chunk loading.

**Step 4: Set spawn point**

Player spawns at center of hull interior.

**Step 5: Test manually**

Create a ship world, verify hull appears, functional blocks are placed.

**Step 6: Commit**

```
feat(ship): generate starter hull with functional blocks
```

---

## Task 7: Capsule object and planet-to-ship warp

Implement the capsule as a placeable object on planets that warps the player to their ship.

**Files:**
- Create: `assets/content/objects/capsule/capsule.object.ron`
- Modify: `src/interaction/mod.rs` or relevant interaction file — handle capsule interaction
- Modify: `src/cosmos/warp.rs` — support warping to ship

**Step 1: Create capsule object**

```ron
// assets/content/objects/capsule/capsule.object.ron
(
    id: "capsule",
    display_name: "Launch Capsule",
    size: (2, 3),
    sprite: "content/objects/capsule/capsule.png",
    placement: Floor,
    object_type: Capsule,
    auto_item: Some((
        description: "Travel between planet surface and your ship",
        max_stack: 1,
    )),
)
```

Add `Capsule` to `ObjectType` enum.

**Step 2: Handle capsule interaction**

When player interacts with capsule:
1. Store current planet address + capsule position in a resource (`CapsuleLocation`)
2. Fire `WarpToBody` event with ship address
3. On ship, set `ShipLocation::Orbit(current_planet_address)`

**Step 3: Handle airlock interaction**

When player interacts with airlock on ship:
1. Check ship is in orbit (not in transit)
2. Read `CapsuleLocation` for the orbited body
3. Fire `WarpToBody` back to planet
4. Spawn player near capsule position

**Step 4: Persist capsule locations**

Add capsule location tracking to Universe persistence so it survives save/load.

**Step 5: Test**

Place capsule on planet → interact → verify warp to ship → interact with airlock → verify return to capsule.

**Step 6: Commit**

```
feat(capsule): implement planet-to-ship transport via capsule and airlock
```

---

## Task 8: Autopilot navigation UI and real-time travel

Implement the autopilot console interaction — open star map, select destination, initiate travel.

**Files:**
- Modify: `src/ui/star_map.rs` — add "Navigate" button (in addition to existing "Warp")
- Modify: `src/cosmos/ship_location.rs` — travel state machine
- Create: `src/cosmos/fuel.rs` — fuel resource and consumption

**Step 1: Add fuel system**

```rust
// src/cosmos/fuel.rs
#[derive(Resource, Debug)]
pub struct ShipFuel {
    pub current: f32,
    pub capacity: f32,
}

impl ShipFuel {
    pub fn consume(&mut self, amount: f32) -> bool {
        if self.current >= amount {
            self.current -= amount;
            true
        } else {
            false
        }
    }
}
```

Fuel capacity = sum of all placed FuelTank objects' capacities.

**Step 2: Calculate fuel cost**

Cost = distance between orbits (orbit index difference) × base cost per orbit.

**Step 3: Modify star map UI for autopilot**

When opened from autopilot console (vs. F4 key), show:
- Fuel cost per destination
- "Navigate" button (grayed out if insufficient fuel)
- Current fuel level

**Step 4: Implement travel state machine**

```rust
pub fn tick_ship_travel(
    time: Res<Time>,
    mut location: ResMut<ShipLocation>,
    mut global_biome: ResMut<GlobalBiome>,
) {
    if let ShipLocation::InTransit { progress, duration, from, to, .. } = location.as_mut() {
        *progress += time.delta_secs() / *duration;
        if *progress >= 1.0 {
            // Arrived
            *location = ShipLocation::Orbit(to.clone());
            // Update global biome to orbit biome of destination
            global_biome.biome_id = orbit_biome_for(to);
        }
        // During transit: biome is deep_space
    }
}
```

**Step 5: Block airlock during transit**

In airlock interaction handler, check `ShipLocation` — if `InTransit`, show message "Cannot descend during travel".

**Step 6: Test**

Open autopilot → select destination → verify fuel consumed → verify travel timer → verify arrival.

**Step 7: Commit**

```
feat(navigation): implement autopilot console with fuel and real-time travel
```

---

## Task 9: Zero-gravity and EVA physics

Implement zero-gravity zones and jetpack movement for open space.

**Files:**
- Modify: `src/physics.rs` — conditional gravity based on pressurization
- Create: `src/cosmos/pressurization.rs` — flood-fill atmosphere detection
- Modify: `src/player/mod.rs` — jetpack input when in zero-g

**Step 1: Pressurization flood-fill**

```rust
// src/cosmos/pressurization.rs

/// Per-chunk cached pressurization data.
/// true = pressurized (inside sealed hull), false = vacuum
#[derive(Resource, Default)]
pub struct PressureMap {
    /// Maps chunk (cx, cy) to Vec<bool> (per-tile, same layout as ChunkData)
    chunks: HashMap<(i32, i32), Vec<bool>>,
    pub dirty: bool,
}
```

Algorithm:
1. Collect all edge tiles of the world (x=0, x=max, y=0, y=max)
2. BFS/flood-fill from edges through AIR tiles in foreground layer
3. All reached tiles = vacuum, unreached interior tiles = pressurized
4. Cache result, invalidate on block change

**Step 2: Write tests for flood-fill**

```rust
#[test]
fn sealed_room_is_pressurized() {
    // Create small world with a sealed box of blocks
    // Interior should be pressurized, exterior vacuum
}

#[test]
fn breached_room_is_vacuum() {
    // Same box but with one tile removed from wall
    // Interior should now be vacuum
}
```

**Step 3: Apply gravity conditionally**

In `src/physics.rs`, `apply_gravity()`:
- Check player's tile position against PressureMap
- If pressurized: normal gravity
- If vacuum: gravity = 0 (or very small)

**Step 4: Jetpack movement**

In `src/player/mod.rs`, when player is in vacuum:
- WASD applies impulse in corresponding direction (not just left/right + jump)
- W = up impulse, S = down impulse
- Small friction for playability (not realistic but fun)

**Step 5: Run only on ship worlds**

PressureMap system only runs when current address is a Ship. On planets, skip pressurization entirely (all tiles are "pressurized" implicitly).

**Step 6: Test**

Build sealed room on ship → verify gravity inside → break wall → verify zero-g.

**Step 7: Commit**

```
feat(physics): add pressurization flood-fill and zero-gravity EVA
```

---

## Task 10: Oxygen system

Implement oxygen consumption in vacuum and damage when depleted.

**Files:**
- Create: `src/player/oxygen.rs` — oxygen resource, drain/refill logic
- Modify: `src/player/mod.rs` — register oxygen systems
- Modify: `src/ui/` — oxygen HUD indicator

**Step 1: Oxygen component**

```rust
#[derive(Component, Debug)]
pub struct Oxygen {
    pub current: f32,
    pub max: f32,
    pub drain_rate: f32,  // per second in vacuum
    pub refill_rate: f32, // per second in atmosphere
}

impl Default for Oxygen {
    fn default() -> Self {
        Self {
            current: 100.0,
            max: 100.0,
            drain_rate: 5.0,
            refill_rate: 20.0,
        }
    }
}
```

**Step 2: Oxygen tick system**

```rust
fn tick_oxygen(
    time: Res<Time>,
    pressure_map: Option<Res<PressureMap>>,
    mut query: Query<(&Transform, &mut Oxygen, &mut Health), With<Player>>,
    world_config: Res<ActiveWorld>,
) {
    // If no pressure map (planet world), refill and return
    // If pressurized tile: refill
    // If vacuum: drain
    // If current <= 0: apply damage
}
```

**Step 3: HUD display**

Add oxygen bar to UI (only visible on ship worlds or when oxygen < max).

**Step 4: Test**

Verify drain in vacuum, refill in atmosphere, damage at zero.

**Step 5: Commit**

```
feat(player): add oxygen system with vacuum drain and damage
```

---

## Task 11: Integration and polish

Final integration pass — ensure all systems work together.

**Files:**
- Various — bug fixes and integration
- Modify: `src/main.rs` — register new plugins if needed

**Step 1: Add ship to starter universe**

In `src/cosmos/generation.rs`, when generating a system, also generate a Ship body for the player.

**Step 2: Give player a starter capsule**

Add capsule to starter inventory in `src/player/mod.rs`.

**Step 3: Full flow test**

1. Start game → spawn on planet
2. Place capsule → interact → warp to ship
3. Walk around ship interior (gravity works)
4. Exit through hull breach → zero-g, oxygen drains
5. Return inside → gravity, oxygen refills
6. Use autopilot → select destination → travel starts
7. Wait for arrival → airlock → descend to new planet

**Step 4: Commit**

```
feat(space): integrate ship, capsule, navigation, EVA, and oxygen systems
```

---

## Dependency Order

```
Task 1 (wrap_x) ─────────────────────────────┐
Task 2 (ship planet type) ────────────────────┤
Task 3 (space biomes) ───────────────────────┤
                                              ├─→ Task 6 (starter hull) ──┐
Task 4 (global biome) ───────────────────────┤                           │
Task 5 (functional blocks) ──────────────────┘                           │
                                                                          ├─→ Task 11
Task 7 (capsule warp) ───── needs Task 5, 6 ────────────────────────────┤
Task 8 (autopilot nav) ──── needs Task 4, 5, 6 ─────────────────────────┤
Task 9 (zero-gravity EVA) ── independent ────────────────────────────────┤
Task 10 (oxygen) ──────────── needs Task 9 ─────────────────────────────┘
```

Tasks 1-5 can be parallelized (partially). Tasks 6-10 have dependencies. Task 11 is integration.
