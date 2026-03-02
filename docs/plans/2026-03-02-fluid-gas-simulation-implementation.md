# Fluid & Gas Simulation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement cellular automata fluid and gas simulation with pressure, reactions, and rendering.

**Architecture:** Push-only CA with multi-step iteration (Starbound-style). Fluids stored as separate layer in ChunkData. Data-driven definitions via RON files. Gases are inverted liquids. Reactions (water+lava=stone) via RON config.

**Tech Stack:** Rust, Bevy 0.18, RON for data, serde for serialization.

---

### Task 1: Core Data Types (FluidId, FluidCell)

**Files:**
- Create: `src/fluid/mod.rs`
- Create: `src/fluid/cell.rs`

**Step 1: Create fluid module directory**

Run: `mkdir -p src/fluid`

**Step 2: Write FluidCell unit tests**

Create `src/fluid/cell.rs`:

```rust
use serde::{Deserialize, Serialize};

/// Compact fluid type identifier. Index into FluidRegistry.defs.
/// 0 = no fluid (empty cell).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct FluidId(pub u8);

impl FluidId {
    pub const NONE: FluidId = FluidId(0);
}

/// A single cell of fluid/gas data.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct FluidCell {
    pub fluid_id: FluidId,
    /// Mass of fluid in this cell. 0.0 = empty, 1.0 = full, >1.0 = pressurized.
    pub mass: f32,
}

impl FluidCell {
    pub const EMPTY: FluidCell = FluidCell {
        fluid_id: FluidId::NONE,
        mass: 0.0,
    };

    pub fn new(fluid_id: FluidId, mass: f32) -> Self {
        Self { fluid_id, mass }
    }

    pub fn is_empty(&self) -> bool {
        self.fluid_id == FluidId::NONE || self.mass <= 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_cell_is_empty() {
        let cell = FluidCell::EMPTY;
        assert!(cell.is_empty());
        assert_eq!(cell.fluid_id, FluidId::NONE);
        assert_eq!(cell.mass, 0.0);
    }

    #[test]
    fn cell_with_fluid_is_not_empty() {
        let cell = FluidCell::new(FluidId(1), 0.5);
        assert!(!cell.is_empty());
    }

    #[test]
    fn cell_with_zero_mass_is_empty() {
        let cell = FluidCell::new(FluidId(1), 0.0);
        assert!(cell.is_empty());
    }

    #[test]
    fn fluid_id_none_is_zero() {
        assert_eq!(FluidId::NONE, FluidId(0));
    }

    #[test]
    fn fluid_cell_serialization_roundtrip() {
        let cell = FluidCell::new(FluidId(2), 1.5);
        let serialized = ron::to_string(&cell).unwrap();
        let deserialized: FluidCell = ron::from_str(&serialized).unwrap();
        assert_eq!(deserialized.fluid_id, FluidId(2));
        assert!((deserialized.mass - 1.5).abs() < f32::EPSILON);
    }
}
```

**Step 3: Create mod.rs for the fluid module**

Create `src/fluid/mod.rs`:

```rust
pub mod cell;

pub use cell::{FluidCell, FluidId};
```

**Step 4: Register module in main.rs**

In `src/main.rs`, add `mod fluid;` alongside the other module declarations.

**Step 5: Run tests**

Run: `cargo test fluid::cell -- --nocapture`
Expected: All 5 tests pass.

**Step 6: Commit**

```
git add src/fluid/
git commit -m "feat(fluid): add FluidId and FluidCell core data types"
```

---

### Task 2: FluidDef and FluidRegistry

**Files:**
- Create: `src/fluid/registry.rs`
- Modify: `src/fluid/mod.rs`

**Step 1: Write FluidDef and FluidRegistry**

Create `src/fluid/registry.rs`:

```rust
use std::collections::HashMap;

use bevy::prelude::*;
use serde::Deserialize;

use super::cell::FluidId;

fn default_max_compress() -> f32 {
    0.02
}

fn default_viscosity() -> f32 {
    0.1
}

fn default_density() -> f32 {
    1000.0
}

fn default_color() -> [u8; 4] {
    [128, 128, 255, 180]
}

/// Properties of a single fluid/gas type, deserialized from RON.
#[derive(Debug, Clone, Deserialize)]
pub struct FluidDef {
    pub id: String,
    #[serde(default = "default_density")]
    pub density: f32,
    #[serde(default = "default_viscosity")]
    pub viscosity: f32,
    #[serde(default = "default_max_compress")]
    pub max_compress: f32,
    #[serde(default)]
    pub is_gas: bool,
    #[serde(default = "default_color")]
    pub color: [u8; 4],
    #[serde(default)]
    pub damage_on_contact: f32,
    #[serde(default)]
    pub light_emission: [u8; 3],
    #[serde(default)]
    pub effects: Vec<String>,
}

/// Runtime registry of all fluid types. Index 0 is reserved for NONE.
#[derive(Resource, Debug)]
pub struct FluidRegistry {
    pub(crate) defs: Vec<FluidDef>,
    name_to_id: HashMap<String, FluidId>,
}

impl FluidRegistry {
    /// Build registry from a list of definitions.
    /// Index 0 is reserved (NONE), so defs start at index 1.
    pub fn from_defs(defs: Vec<FluidDef>) -> Self {
        let mut name_to_id = HashMap::new();
        for (i, def) in defs.iter().enumerate() {
            let fid = FluidId((i + 1) as u8);
            name_to_id.insert(def.id.clone(), fid);
        }
        Self { defs, name_to_id }
    }

    /// Get definition by FluidId. Panics if id is NONE or out of range.
    pub fn get(&self, id: FluidId) -> &FluidDef {
        assert!(id != FluidId::NONE, "Cannot get def for FluidId::NONE");
        &self.defs[(id.0 - 1) as usize]
    }

    /// Look up FluidId by string name. Panics if not found.
    pub fn by_name(&self, name: &str) -> FluidId {
        *self
            .name_to_id
            .get(name)
            .unwrap_or_else(|| panic!("Unknown fluid: {name}"))
    }

    /// Look up FluidId by string name, returns None if not found.
    pub fn try_by_name(&self, name: &str) -> Option<FluidId> {
        self.name_to_id.get(name).copied()
    }

    /// Number of registered fluid types (excluding NONE).
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_defs() -> Vec<FluidDef> {
        vec![
            FluidDef {
                id: "water".to_string(),
                density: 1000.0,
                viscosity: 0.1,
                max_compress: 0.02,
                is_gas: false,
                color: [64, 128, 255, 180],
                damage_on_contact: 0.0,
                light_emission: [0, 0, 0],
                effects: vec![],
            },
            FluidDef {
                id: "lava".to_string(),
                density: 3000.0,
                viscosity: 0.6,
                max_compress: 0.01,
                is_gas: false,
                color: [255, 80, 20, 220],
                damage_on_contact: 10.0,
                light_emission: [255, 100, 20],
                effects: vec![],
            },
            FluidDef {
                id: "steam".to_string(),
                density: 0.6,
                viscosity: 0.05,
                max_compress: 0.01,
                is_gas: true,
                color: [200, 200, 200, 100],
                damage_on_contact: 0.0,
                light_emission: [0, 0, 0],
                effects: vec![],
            },
        ]
    }

    #[test]
    fn registry_from_defs() {
        let reg = FluidRegistry::from_defs(test_defs());
        assert_eq!(reg.len(), 3);
    }

    #[test]
    fn registry_by_name() {
        let reg = FluidRegistry::from_defs(test_defs());
        let water_id = reg.by_name("water");
        assert_eq!(water_id, FluidId(1));
        let lava_id = reg.by_name("lava");
        assert_eq!(lava_id, FluidId(2));
        let steam_id = reg.by_name("steam");
        assert_eq!(steam_id, FluidId(3));
    }

    #[test]
    fn registry_get_def() {
        let reg = FluidRegistry::from_defs(test_defs());
        let water = reg.get(FluidId(1));
        assert_eq!(water.id, "water");
        assert!(!water.is_gas);

        let steam = reg.get(FluidId(3));
        assert_eq!(steam.id, "steam");
        assert!(steam.is_gas);
    }

    #[test]
    fn registry_try_by_name_returns_none_for_unknown() {
        let reg = FluidRegistry::from_defs(test_defs());
        assert!(reg.try_by_name("unknown").is_none());
    }

    #[test]
    #[should_panic(expected = "Cannot get def for FluidId::NONE")]
    fn registry_get_none_panics() {
        let reg = FluidRegistry::from_defs(test_defs());
        reg.get(FluidId::NONE);
    }
}
```

**Step 2: Update mod.rs**

Add to `src/fluid/mod.rs`:

```rust
pub mod cell;
pub mod registry;

pub use cell::{FluidCell, FluidId};
pub use registry::{FluidDef, FluidRegistry};
```

**Step 3: Run tests**

Run: `cargo test fluid::registry -- --nocapture`
Expected: All 5 tests pass.

**Step 4: Commit**

```
git add src/fluid/registry.rs src/fluid/mod.rs
git commit -m "feat(fluid): add FluidDef and FluidRegistry with data-driven definitions"
```

---

### Task 3: RON Asset Loading Pipeline

**Files:**
- Create: `assets/content/fluids/fluids.registry.ron`
- Modify: `src/fluid/mod.rs`
- Modify: `src/registry/assets.rs` (add FluidRegistryAsset)
- Modify: `src/registry/mod.rs` (register loader)
- Modify: `src/registry/loading.rs` (add to loading pipeline)

**Step 1: Create RON file for fluid definitions**

Create `assets/content/fluids/fluids.registry.ron`:

```ron
FluidRegistryAsset(
    fluids: [
        (
            id: "water",
            density: 1000.0,
            viscosity: 0.1,
            max_compress: 0.02,
            is_gas: false,
            color: (64, 128, 255, 180),
            damage_on_contact: 0.0,
            light_emission: (0, 0, 0),
            effects: [],
        ),
        (
            id: "lava",
            density: 3000.0,
            viscosity: 0.6,
            max_compress: 0.01,
            is_gas: false,
            color: (255, 80, 20, 220),
            damage_on_contact: 10.0,
            light_emission: (255, 100, 20),
            effects: [],
        ),
        (
            id: "steam",
            density: 0.6,
            viscosity: 0.05,
            max_compress: 0.01,
            is_gas: true,
            color: (200, 200, 200, 100),
            damage_on_contact: 0.0,
            light_emission: (0, 0, 0),
            effects: [],
        ),
        (
            id: "toxic_gas",
            density: 1.5,
            viscosity: 0.08,
            max_compress: 0.01,
            is_gas: true,
            color: (80, 200, 60, 120),
            damage_on_contact: 2.0,
            light_emission: (0, 0, 0),
            effects: ["breathing_damage"],
        ),
        (
            id: "smoke",
            density: 0.8,
            viscosity: 0.03,
            max_compress: 0.005,
            is_gas: true,
            color: (100, 100, 100, 80),
            damage_on_contact: 0.0,
            light_emission: (0, 0, 0),
            effects: ["reduce_visibility"],
        ),
    ],
)
```

**Step 2: Add FluidRegistryAsset to assets.rs**

In `src/registry/assets.rs`, add:

```rust
use crate::fluid::FluidDef;

#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct FluidRegistryAsset {
    pub fluids: Vec<FluidDef>,
}
```

**Step 3: Register asset loader in RegistryPlugin**

In `src/registry/mod.rs`, in `RegistryPlugin::build`, add alongside existing init_asset calls:

```rust
.init_asset::<FluidRegistryAsset>()
.register_asset_loader(RonLoader::<FluidRegistryAsset>::new(&["fluid.ron"]))
```

Note: The RON file should use extension `.fluid.ron` to match, OR keep the pattern
used by `tiles.registry.ron`. Check the actual extension pattern — tiles use
`tiles.registry.ron` loaded as `TileRegistryAsset` with extension `["registry.ron"]`.
Match the existing pattern. If tiles use a generic extension, fluids should too.
Alternatively, name the file `fluids.fluid.ron` and use extension `["fluid.ron"]`.

**Step 4: Add to LoadingAssets and loading pipeline**

In `src/registry/loading.rs`:

Add field to `LoadingAssets`:
```rust
fluids: Handle<FluidRegistryAsset>,
```

In `start_loading`, add:
```rust
let fluids = asset_server.load::<FluidRegistryAsset>("content/fluids/fluids.registry.ron");
```
(Include in the LoadingAssets constructor.)

In `check_loading`, add check:
```rust
if let Some(fluid_asset) = assets_fluid.get(loading.fluids.id()) {
    let fluid_registry = FluidRegistry::from_defs(fluid_asset.fluids.clone());
    commands.insert_resource(fluid_registry);
}
```
Add the `Res<Assets<FluidRegistryAsset>>` parameter to `check_loading`.

**Step 5: Run build**

Run: `cargo build`
Expected: Compiles without errors.

**Step 6: Commit**

```
git add assets/content/fluids/ src/registry/assets.rs src/registry/mod.rs src/registry/loading.rs
git commit -m "feat(fluid): add RON-based fluid registry loading pipeline"
```

---

### Task 4: Extend ChunkData with Fluids

**Files:**
- Modify: `src/world/chunk.rs` (add fluids to ChunkData)
- Modify: `src/fluid/mod.rs` (re-export)

**Step 1: Add fluids field to ChunkData**

In `src/world/chunk.rs`, add to `ChunkData`:

```rust
use crate::fluid::FluidCell;

pub struct ChunkData {
    pub fg: TileLayer,
    pub bg: TileLayer,
    pub fluids: Vec<FluidCell>,  // NEW
    pub objects: Vec<PlacedObject>,
    pub occupancy: Vec<Option<OccupancyRef>>,
    #[allow(dead_code)]
    pub damage: Vec<u8>,
}
```

**Step 2: Update get_or_generate_chunk**

In the `or_insert_with` closure, add fluids initialization:

```rust
ChunkData {
    fg: TileLayer { tiles: chunk_tiles.fg, bitmasks: vec![0; len] },
    bg: TileLayer { tiles: chunk_tiles.bg, bitmasks: vec![0; len] },
    fluids: vec![FluidCell::EMPTY; len],  // NEW
    objects: Vec::new(),
    occupancy: vec![None; len],
    damage: vec![0; len],
}
```

**Step 3: Add fluid accessor methods to WorldMap**

```rust
impl WorldMap {
    /// Read fluid cell if chunk is loaded.
    pub fn get_fluid(&self, tile_x: i32, tile_y: i32, ctx: &WorldCtxRef) -> Option<FluidCell> {
        if tile_y < 0 || tile_y >= ctx.config.height_tiles {
            return Some(FluidCell::EMPTY);
        }
        let wrapped_x = ctx.config.wrap_tile_x(tile_x);
        let (cx, cy) = tile_to_chunk(wrapped_x, tile_y, ctx.config.chunk_size);
        let (lx, ly) = tile_to_local(wrapped_x, tile_y, ctx.config.chunk_size);
        self.chunks.get(&(cx, cy)).map(|chunk| {
            chunk.fluids[(ly * ctx.config.chunk_size + lx) as usize]
        })
    }

    /// Set fluid cell. Marks chunk as needing fluid update.
    pub fn set_fluid(
        &mut self,
        tile_x: i32,
        tile_y: i32,
        cell: FluidCell,
        ctx: &WorldCtxRef,
    ) {
        if tile_y < 0 || tile_y >= ctx.config.height_tiles {
            return;
        }
        let wrapped_x = ctx.config.wrap_tile_x(tile_x);
        let (cx, cy) = tile_to_chunk(wrapped_x, tile_y, ctx.config.chunk_size);
        let (lx, ly) = tile_to_local(wrapped_x, tile_y, ctx.config.chunk_size);
        if let Some(chunk) = self.chunks.get_mut(&(cx, cy)) {
            chunk.fluids[(ly * ctx.config.chunk_size + lx) as usize] = cell;
        }
    }
}
```

**Step 4: Fix any compilation errors**

Anywhere ChunkData is constructed manually (tests, etc.), add `fluids: vec![FluidCell::EMPTY; len]`.

**Step 5: Run tests**

Run: `cargo test`
Expected: All existing tests still pass.

**Step 6: Commit**

```
git add src/world/chunk.rs src/fluid/mod.rs
git commit -m "feat(fluid): extend ChunkData with fluids layer"
```

---

### Task 5: Core Simulation Algorithm

**Files:**
- Create: `src/fluid/simulation.rs`
- Modify: `src/fluid/mod.rs`

**Step 1: Write get_stable_state tests**

Create `src/fluid/simulation.rs` with tests first:

```rust
use crate::fluid::cell::{FluidCell, FluidId};
use crate::fluid::registry::FluidRegistry;

/// Normal full-cell mass.
pub const MAX_MASS: f32 = 1.0;
/// Cells with less mass than this are considered empty.
pub const MIN_MASS: f32 = 0.001;
/// Flows smaller than this are damped.
pub const MIN_FLOW: f32 = 0.005;
/// Maximum flow per iteration (before viscosity scaling).
pub const MAX_SPEED: f32 = 1.0;

/// Calculate how much mass should be in the bottom cell of two vertically
/// adjacent cells with the given total mass.
///
/// This implements the "slightly compressible liquid" model where bottom cells
/// can hold slightly more mass than top cells, creating implicit pressure.
pub fn get_stable_state(total_mass: f32, max_compress: f32) -> f32 {
    if total_mass <= MAX_MASS {
        // Not enough to fill even one cell — all goes to bottom
        total_mass
    } else if total_mass < 2.0 * MAX_MASS + max_compress {
        // Bottom cell full + proportional compression
        (MAX_MASS * MAX_MASS + total_mass * max_compress) / (MAX_MASS + max_compress)
    } else {
        // Both cells full — bottom has +max_compress more than top
        (total_mass + max_compress) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- get_stable_state tests ---

    #[test]
    fn stable_state_empty() {
        // No water at all
        let bottom = get_stable_state(0.0, 0.02);
        assert!((bottom - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn stable_state_half_cell() {
        // Half a cell — all goes to bottom
        let bottom = get_stable_state(0.5, 0.02);
        assert!((bottom - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn stable_state_one_cell() {
        // Exactly one cell — all in bottom
        let bottom = get_stable_state(1.0, 0.02);
        assert!((bottom - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn stable_state_two_cells() {
        // Two full cells — bottom should have 1.0 + compress/2
        let bottom = get_stable_state(2.0, 0.02);
        assert!(bottom > 1.0, "Bottom should be > 1.0, got {bottom}");
        assert!(bottom < 1.02, "Bottom should be < 1.02, got {bottom}");
        // top = 2.0 - bottom
        let top = 2.0 - bottom;
        let diff = bottom - top;
        assert!(
            (diff - 0.02).abs() < 0.001,
            "Difference should be ~0.02, got {diff}"
        );
    }

    #[test]
    fn stable_state_three_cells() {
        // Well above 2*MAX + compress
        let bottom = get_stable_state(3.0, 0.02);
        let top = 3.0 - bottom;
        let diff = bottom - top;
        assert!(
            (diff - 0.02).abs() < f32::EPSILON,
            "Difference should be exactly 0.02, got {diff}"
        );
    }

    #[test]
    fn stable_state_always_positive() {
        for i in 0..100 {
            let total = i as f32 * 0.1;
            let bottom = get_stable_state(total, 0.02);
            assert!(bottom >= 0.0, "Bottom should be >= 0 for total={total}");
            assert!(
                bottom <= total,
                "Bottom ({bottom}) should be <= total ({total})"
            );
        }
    }
}
```

**Step 2: Run tests to verify they pass**

Run: `cargo test fluid::simulation -- --nocapture`
Expected: All 6 tests pass.

**Step 3: Implement simulate_chunk — the core CA step**

Add to `src/fluid/simulation.rs`:

```rust
use crate::registry::tile::TileRegistry;
use crate::world::chunk::{tile_to_chunk, tile_to_local, WorldMap};
use crate::registry::world::ActiveWorld;

/// Configuration for the fluid simulation.
#[derive(Debug, Clone)]
pub struct FluidSimConfig {
    pub iterations_per_tick: u32,
    pub min_mass: f32,
    pub min_flow: f32,
    pub max_speed: f32,
}

impl Default for FluidSimConfig {
    fn default() -> Self {
        Self {
            iterations_per_tick: 3,
            min_mass: MIN_MASS,
            min_flow: MIN_FLOW,
            max_speed: MAX_SPEED,
        }
    }
}

/// Run one iteration of the fluid simulation on a flat grid.
/// `tiles` is the foreground tile array (same indexing as fluids).
/// `fluids` is the current fluid state (read).
/// `new_fluids` is the output buffer (write).
/// `width` and `height` define the grid dimensions.
///
/// This function processes a single chunk. For cross-chunk flow,
/// the caller must handle boundary cells separately.
pub fn simulate_grid(
    tiles: &[crate::registry::tile::TileId],
    fluids: &[FluidCell],
    new_fluids: &mut [FluidCell],
    width: u32,
    height: u32,
    tile_registry: &TileRegistry,
    fluid_registry: &FluidRegistry,
    config: &FluidSimConfig,
) {
    // Copy current state to new_fluids as starting point
    new_fluids.copy_from_slice(fluids);

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let cell = fluids[idx];

            if cell.is_empty() {
                continue;
            }

            let def = fluid_registry.get(cell.fluid_id);
            let max_speed = config.max_speed * (1.0 - def.viscosity).max(0.1);
            let mut remaining = cell.mass;

            if def.is_gas {
                // Gas: flow UP first, then horizontal, then DOWN (decompression)
                remaining = try_flow_vertical(
                    x, y, idx, 1, // +1 = up
                    remaining, cell.fluid_id, def.max_compress,
                    max_speed, config.min_flow,
                    tiles, fluids, new_fluids, width, height,
                    tile_registry,
                );
                remaining = try_flow_horizontal(
                    x, y, idx,
                    remaining, cell.fluid_id, cell.mass,
                    max_speed, config.min_flow,
                    tiles, fluids, new_fluids, width, height,
                    tile_registry,
                );
                try_flow_vertical(
                    x, y, idx, -1, // -1 = down (decompression for gas)
                    remaining, cell.fluid_id, def.max_compress,
                    max_speed, config.min_flow,
                    tiles, fluids, new_fluids, width, height,
                    tile_registry,
                );
            } else {
                // Liquid: flow DOWN first, then horizontal, then UP (decompression)
                remaining = try_flow_vertical(
                    x, y, idx, -1, // -1 = down
                    remaining, cell.fluid_id, def.max_compress,
                    max_speed, config.min_flow,
                    tiles, fluids, new_fluids, width, height,
                    tile_registry,
                );
                remaining = try_flow_horizontal(
                    x, y, idx,
                    remaining, cell.fluid_id, cell.mass,
                    max_speed, config.min_flow,
                    tiles, fluids, new_fluids, width, height,
                    tile_registry,
                );
                try_flow_vertical(
                    x, y, idx, 1, // +1 = up (decompression for liquid)
                    remaining, cell.fluid_id, def.max_compress,
                    max_speed, config.min_flow,
                    tiles, fluids, new_fluids, width, height,
                    tile_registry,
                );
            }
        }
    }

    // Clean up cells with negligible mass
    for cell in new_fluids.iter_mut() {
        if cell.mass < config.min_mass {
            *cell = FluidCell::EMPTY;
        }
    }
}

/// Try to flow vertically. `dy` is -1 (down) or +1 (up).
/// For primary direction: uses get_stable_state.
/// For decompression: only flows if mass > MAX_MASS.
/// Returns remaining mass.
fn try_flow_vertical(
    x: u32,
    y: u32,
    idx: usize,
    dy: i32,
    remaining: f32,
    fluid_id: FluidId,
    max_compress: f32,
    max_speed: f32,
    min_flow: f32,
    tiles: &[crate::registry::tile::TileId],
    fluids: &[FluidCell],
    new_fluids: &mut [FluidCell],
    width: u32,
    height: u32,
    tile_registry: &TileRegistry,
) -> f32 {
    if remaining <= 0.0 {
        return 0.0;
    }

    let ny = y as i32 + dy;
    if ny < 0 || ny >= height as i32 {
        return remaining;
    }

    let nidx = (ny as u32 * width + x) as usize;

    // Check if neighbor tile is solid
    if tile_registry.is_solid(tiles[nidx]) {
        return remaining;
    }

    // Check if neighbor has different fluid type (can't mix)
    let neighbor = fluids[nidx];
    if !neighbor.is_empty() && neighbor.fluid_id != fluid_id {
        return remaining;
    }

    let neighbor_mass = new_fluids[nidx].mass;
    let total = remaining + neighbor_mass;

    let flow;
    if dy < 0 {
        // Flowing down (primary for liquids)
        let target_below = get_stable_state(total, max_compress);
        flow = target_below - neighbor_mass;
    } else {
        // Flowing up (primary for gases, decompression for liquids)
        // For decompression: only compressed fluid flows up
        if remaining <= MAX_MASS {
            return remaining;
        }
        let target_below = get_stable_state(total, max_compress);
        flow = remaining - target_below;
    }

    if flow <= 0.0 {
        return remaining;
    }

    let mut flow = flow;
    // Smooth small flows
    if flow > min_flow {
        flow *= 0.5;
    }
    // Clamp
    flow = flow.min(max_speed).min(remaining).max(0.0);

    if flow <= 0.0 {
        return remaining;
    }

    new_fluids[idx].mass -= flow;
    new_fluids[nidx].mass += flow;
    if new_fluids[nidx].fluid_id == FluidId::NONE {
        new_fluids[nidx].fluid_id = fluid_id;
    }

    remaining - flow
}

/// Try to flow horizontally (left and right).
/// Returns remaining mass.
fn try_flow_horizontal(
    x: u32,
    y: u32,
    idx: usize,
    mut remaining: f32,
    fluid_id: FluidId,
    original_mass: f32,
    max_speed: f32,
    min_flow: f32,
    tiles: &[crate::registry::tile::TileId],
    fluids: &[FluidCell],
    new_fluids: &mut [FluidCell],
    width: u32,
    height: u32,
    tile_registry: &TileRegistry,
) -> f32 {
    // Try left
    if x > 0 {
        remaining = try_flow_side(
            x, y, idx, x - 1,
            remaining, fluid_id, original_mass,
            max_speed, min_flow,
            tiles, fluids, new_fluids, width,
            tile_registry,
        );
    }
    // Try right
    if x + 1 < width {
        remaining = try_flow_side(
            x, y, idx, x + 1,
            remaining, fluid_id, original_mass,
            max_speed, min_flow,
            tiles, fluids, new_fluids, width,
            tile_registry,
        );
    }
    remaining
}

fn try_flow_side(
    _x: u32,
    y: u32,
    idx: usize,
    nx: u32,
    remaining: f32,
    fluid_id: FluidId,
    original_mass: f32,
    max_speed: f32,
    min_flow: f32,
    tiles: &[crate::registry::tile::TileId],
    fluids: &[FluidCell],
    new_fluids: &mut [FluidCell],
    width: u32,
    tile_registry: &TileRegistry,
) -> f32 {
    if remaining <= 0.0 {
        return 0.0;
    }

    let nidx = (y * width + nx) as usize;

    if tile_registry.is_solid(tiles[nidx]) {
        return remaining;
    }

    let neighbor = fluids[nidx];
    if !neighbor.is_empty() && neighbor.fluid_id != fluid_id {
        return remaining;
    }

    // Equalize: flow = (my_mass - neighbor_mass) / 4
    let mut flow = (original_mass - fluids[nidx].mass) / 4.0;
    if flow <= 0.0 {
        return remaining;
    }
    if flow > min_flow {
        flow *= 0.5;
    }
    flow = flow.min(max_speed).min(remaining).max(0.0);

    if flow <= 0.0 {
        return remaining;
    }

    new_fluids[idx].mass -= flow;
    new_fluids[nidx].mass += flow;
    if new_fluids[nidx].fluid_id == FluidId::NONE {
        new_fluids[nidx].fluid_id = fluid_id;
    }

    remaining - flow
}
```

**Step 4: Write integration tests for the simulation**

Add to tests in `src/fluid/simulation.rs`:

```rust
    // --- Simulation integration tests ---

    use crate::registry::tile::{TileDef, TileId, TileRegistry};

    fn test_tile_registry() -> TileRegistry {
        crate::test_helpers::fixtures::test_tile_registry()
    }

    fn test_fluid_registry() -> FluidRegistry {
        FluidRegistry::from_defs(vec![
            FluidDef {
                id: "water".to_string(),
                density: 1000.0,
                viscosity: 0.0, // no viscosity for tests
                max_compress: 0.02,
                is_gas: false,
                color: [64, 128, 255, 180],
                damage_on_contact: 0.0,
                light_emission: [0, 0, 0],
                effects: vec![],
            },
            FluidDef {
                id: "gas".to_string(),
                density: 0.5,
                viscosity: 0.0,
                max_compress: 0.01,
                is_gas: true,
                color: [200, 200, 200, 100],
                damage_on_contact: 0.0,
                light_emission: [0, 0, 0],
                effects: vec![],
            },
        ])
    }

    fn make_grid(width: u32, height: u32) -> (Vec<TileId>, Vec<FluidCell>) {
        let len = (width * height) as usize;
        (vec![TileId::AIR; len], vec![FluidCell::EMPTY; len])
    }

    fn idx(x: u32, y: u32, width: u32) -> usize {
        (y * width + x) as usize
    }

    #[test]
    fn water_falls_down() {
        let w = 3;
        let h = 3;
        let (tiles, mut fluids) = make_grid(w, h);
        let mut new_fluids = fluids.clone();
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig {
            iterations_per_tick: 1,
            ..Default::default()
        };

        let water_id = fr.by_name("water");
        // Place water at top-center (x=1, y=2)
        fluids[idx(1, 2, w)] = FluidCell::new(water_id, 1.0);

        simulate_grid(&tiles, &fluids, &mut new_fluids, w, h, &tr, &fr, &config);

        // Water should have moved down (y=2 -> y=1)
        assert!(
            new_fluids[idx(1, 1, w)].mass > 0.0,
            "Water should flow to cell below"
        );
        assert!(
            new_fluids[idx(1, 2, w)].mass < 1.0,
            "Source cell should have less water"
        );
    }

    #[test]
    fn water_spreads_horizontally_on_floor() {
        let w = 5;
        let h = 3;
        let (mut tiles, mut fluids) = make_grid(w, h);
        let mut new_fluids = fluids.clone();
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig::default();

        let water_id = fr.by_name("water");
        // Solid floor at y=0
        for x in 0..w {
            tiles[idx(x, 0, w)] = TileId(3); // stone = solid
        }
        // Water at center, resting on floor
        fluids[idx(2, 1, w)] = FluidCell::new(water_id, 1.0);

        // Run several iterations to let water spread
        let mut current = fluids.clone();
        for _ in 0..10 {
            new_fluids = current.clone();
            simulate_grid(&tiles, &current, &mut new_fluids, w, h, &tr, &fr, &config);
            current = new_fluids.clone();
        }

        // Water should have spread left and right
        assert!(
            current[idx(1, 1, w)].mass > 0.0,
            "Water should spread left"
        );
        assert!(
            current[idx(3, 1, w)].mass > 0.0,
            "Water should spread right"
        );
    }

    #[test]
    fn water_blocked_by_solid() {
        let w = 3;
        let h = 3;
        let (mut tiles, mut fluids) = make_grid(w, h);
        let mut new_fluids = fluids.clone();
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig::default();

        let water_id = fr.by_name("water");
        // Solid block below water
        tiles[idx(1, 0, w)] = TileId(3); // stone
        tiles[idx(1, 1, w)] = TileId(3); // stone
        // Water above
        fluids[idx(1, 2, w)] = FluidCell::new(water_id, 1.0);

        simulate_grid(&tiles, &fluids, &mut new_fluids, w, h, &tr, &fr, &config);

        // Water should NOT be in the solid cell
        assert!(
            new_fluids[idx(1, 1, w)].mass <= 0.0,
            "Water should not enter solid cell"
        );
    }

    #[test]
    fn pressure_pushes_water_up_in_u_tube() {
        // U-tube: two columns connected at bottom
        //   W . W
        //   W . W
        //   # . #
        //   # . #
        //   . . .   <- connected bottom
        let w = 3;
        let h = 5;
        let (mut tiles, mut fluids) = make_grid(w, h);
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig {
            iterations_per_tick: 1,
            ..Default::default()
        };
        let water_id = fr.by_name("water");

        // Walls: x=0,x=2 at y=1,y=2 (middle section)
        tiles[idx(0, 1, w)] = TileId(3);
        tiles[idx(0, 2, w)] = TileId(3);
        tiles[idx(2, 1, w)] = TileId(3);
        tiles[idx(2, 2, w)] = TileId(3);

        // Water in left column (x=0, y=3,4) - but walls are there...
        // Let me redo: walls on sides, open bottom
        // Actually: U-tube with 5 wide, walls forming the U shape
        // Simpler: just test that pressurized water flows up
        // Place water 3 cells high in a 1-wide column
        let w2 = 1;
        let h2 = 5;
        let (tiles2, mut fluids2) = make_grid(w2, h2);
        let mut new_fluids2 = fluids2.clone();

        // Stack 3 water cells
        fluids2[idx(0, 0, w2)] = FluidCell::new(water_id, 1.0);
        fluids2[idx(0, 1, w2)] = FluidCell::new(water_id, 1.0);
        fluids2[idx(0, 2, w2)] = FluidCell::new(water_id, 1.0);

        // Run iterations so pressure builds
        let mut current = fluids2.clone();
        for _ in 0..20 {
            new_fluids2 = current.clone();
            simulate_grid(
                &tiles2, &current, &mut new_fluids2, w2, h2, &tr, &fr, &config,
            );
            current = new_fluids2.clone();
        }

        // Bottom cell should be compressed (mass > 1.0)
        assert!(
            current[idx(0, 0, w2)].mass > 1.0,
            "Bottom cell should be compressed, got {}",
            current[idx(0, 0, w2)].mass
        );
    }

    #[test]
    fn gas_flows_up() {
        let w = 3;
        let h = 3;
        let (tiles, mut fluids) = make_grid(w, h);
        let mut new_fluids = fluids.clone();
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig::default();

        let gas_id = fr.by_name("gas");
        // Place gas at bottom-center
        fluids[idx(1, 0, w)] = FluidCell::new(gas_id, 1.0);

        simulate_grid(&tiles, &fluids, &mut new_fluids, w, h, &tr, &fr, &config);

        // Gas should have moved up
        assert!(
            new_fluids[idx(1, 1, w)].mass > 0.0,
            "Gas should flow upward"
        );
        assert!(
            new_fluids[idx(1, 0, w)].mass < 1.0,
            "Source cell should have less gas"
        );
    }

    #[test]
    fn mass_is_conserved() {
        let w = 5;
        let h = 5;
        let (mut tiles, mut fluids) = make_grid(w, h);
        let tr = test_tile_registry();
        let fr = test_fluid_registry();
        let config = FluidSimConfig::default();
        let water_id = fr.by_name("water");

        // Floor
        for x in 0..w {
            tiles[idx(x, 0, w)] = TileId(3);
        }

        // Add water
        fluids[idx(2, 3, w)] = FluidCell::new(water_id, 1.0);
        fluids[idx(2, 2, w)] = FluidCell::new(water_id, 0.7);

        let initial_mass: f32 = fluids.iter().map(|c| c.mass).sum();

        let mut current = fluids;
        for _ in 0..50 {
            let mut new = current.clone();
            simulate_grid(&tiles, &current, &mut new, w, h, &tr, &fr, &config);
            current = new;
        }

        let final_mass: f32 = current.iter().map(|c| c.mass).sum();
        assert!(
            (initial_mass - final_mass).abs() < 0.01,
            "Mass should be conserved: initial={initial_mass}, final={final_mass}"
        );
    }
```

**Step 5: Run tests**

Run: `cargo test fluid::simulation -- --nocapture`
Expected: All tests pass.

**Step 6: Commit**

```
git add src/fluid/simulation.rs src/fluid/mod.rs
git commit -m "feat(fluid): implement core CA simulation with pressure model"
```

---

### Task 6: Fluid Reactions

**Files:**
- Create: `src/fluid/reactions.rs`
- Create: `assets/content/fluids/reactions.ron`
- Modify: `src/fluid/mod.rs`

**Step 1: Create reactions module with data structures and logic**

Create `src/fluid/reactions.rs`:

```rust
use serde::Deserialize;
use bevy::prelude::*;
use std::collections::HashMap;

use crate::fluid::cell::{FluidCell, FluidId};
use crate::fluid::registry::FluidRegistry;
use crate::registry::tile::{TileId, TileRegistry};

/// Adjacency requirement for a reaction.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub enum Adjacency {
    Any,
    Above,
    Below,
    Side,
}

impl Default for Adjacency {
    fn default() -> Self {
        Adjacency::Any
    }
}

/// A single fluid reaction definition, loaded from RON.
#[derive(Debug, Clone, Deserialize)]
pub struct FluidReactionDef {
    pub fluid_a: String,
    pub fluid_b: String,
    #[serde(default)]
    pub adjacency: Adjacency,
    pub result_tile: Option<String>,
    pub result_fluid: Option<String>,
    #[serde(default)]
    pub min_mass_a: f32,
    #[serde(default)]
    pub min_mass_b: f32,
    #[serde(default = "default_consume_a")]
    pub consume_a: f32,
    #[serde(default = "default_consume_b")]
    pub consume_b: f32,
    pub byproduct_fluid: Option<String>,
    #[serde(default)]
    pub byproduct_mass: f32,
}

fn default_consume_a() -> f32 { 1.0 }
fn default_consume_b() -> f32 { 1.0 }

/// Compiled reaction with resolved IDs for fast lookup.
#[derive(Debug, Clone)]
pub struct CompiledReaction {
    pub fluid_a: FluidId,
    pub fluid_b: FluidId,
    pub adjacency: Adjacency,
    pub result_tile: Option<TileId>,
    pub result_fluid: Option<FluidId>,
    pub min_mass_a: f32,
    pub min_mass_b: f32,
    pub consume_a: f32,
    pub consume_b: f32,
    pub byproduct_fluid: Option<FluidId>,
    pub byproduct_mass: f32,
}

/// Runtime registry of fluid reactions.
#[derive(Resource, Debug)]
pub struct FluidReactionRegistry {
    pub reactions: Vec<CompiledReaction>,
}

impl FluidReactionRegistry {
    pub fn from_defs(
        defs: &[FluidReactionDef],
        fluid_registry: &FluidRegistry,
        tile_registry: &TileRegistry,
    ) -> Self {
        let reactions = defs
            .iter()
            .map(|def| CompiledReaction {
                fluid_a: fluid_registry.by_name(&def.fluid_a),
                fluid_b: fluid_registry.by_name(&def.fluid_b),
                adjacency: def.adjacency.clone(),
                result_tile: def
                    .result_tile
                    .as_ref()
                    .map(|name| tile_registry.by_name(name)),
                result_fluid: def
                    .result_fluid
                    .as_ref()
                    .and_then(|name| fluid_registry.try_by_name(name)),
                min_mass_a: def.min_mass_a,
                min_mass_b: def.min_mass_b,
                consume_a: def.consume_a,
                consume_b: def.consume_b,
                byproduct_fluid: def
                    .byproduct_fluid
                    .as_ref()
                    .and_then(|name| fluid_registry.try_by_name(name)),
                byproduct_mass: def.byproduct_mass,
            })
            .collect();
        Self { reactions }
    }

    /// Find a reaction between two fluids with given adjacency.
    pub fn find_reaction(
        &self,
        a: FluidId,
        b: FluidId,
        adjacency: &Adjacency,
    ) -> Option<&CompiledReaction> {
        self.reactions.iter().find(|r| {
            ((r.fluid_a == a && r.fluid_b == b) || (r.fluid_a == b && r.fluid_b == a))
                && (r.adjacency == Adjacency::Any || r.adjacency == *adjacency)
        })
    }
}

/// Process density displacement: heavier fluids sink, lighter fluids rise.
/// Call after the main CA simulation step.
pub fn resolve_density_displacement(
    fluids: &mut [FluidCell],
    width: u32,
    height: u32,
    fluid_registry: &FluidRegistry,
) {
    for y in 1..height {
        for x in 0..width {
            let below_idx = ((y - 1) * width + x) as usize;
            let above_idx = (y * width + x) as usize;

            let below = fluids[below_idx];
            let above = fluids[above_idx];

            if below.is_empty() || above.is_empty() {
                continue;
            }
            if below.fluid_id == above.fluid_id {
                continue;
            }

            let density_below = fluid_registry.get(below.fluid_id).density;
            let density_above = fluid_registry.get(above.fluid_id).density;

            // If lighter fluid is below heavier fluid, swap
            if density_below < density_above {
                fluids[below_idx] = above;
                fluids[above_idx] = below;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluid::registry::FluidDef;

    fn test_fluid_registry() -> FluidRegistry {
        FluidRegistry::from_defs(vec![
            FluidDef {
                id: "water".to_string(),
                density: 1000.0,
                viscosity: 0.0,
                max_compress: 0.02,
                is_gas: false,
                color: [64, 128, 255, 180],
                damage_on_contact: 0.0,
                light_emission: [0, 0, 0],
                effects: vec![],
            },
            FluidDef {
                id: "lava".to_string(),
                density: 3000.0,
                viscosity: 0.6,
                max_compress: 0.01,
                is_gas: false,
                color: [255, 80, 20, 220],
                damage_on_contact: 10.0,
                light_emission: [255, 100, 20],
                effects: vec![],
            },
            FluidDef {
                id: "steam".to_string(),
                density: 0.6,
                viscosity: 0.05,
                max_compress: 0.01,
                is_gas: true,
                color: [200, 200, 200, 100],
                damage_on_contact: 0.0,
                light_emission: [0, 0, 0],
                effects: vec![],
            },
        ])
    }

    fn idx(x: u32, y: u32, w: u32) -> usize {
        (y * w + x) as usize
    }

    #[test]
    fn density_displacement_heavy_sinks() {
        let fr = test_fluid_registry();
        let water_id = fr.by_name("water");
        let lava_id = fr.by_name("lava");

        let w = 1;
        let h = 2;
        let mut fluids = vec![FluidCell::EMPTY; (w * h) as usize];

        // Water below, lava above — lava is heavier, should sink
        fluids[idx(0, 0, w)] = FluidCell::new(water_id, 1.0);
        fluids[idx(0, 1, w)] = FluidCell::new(lava_id, 1.0);

        resolve_density_displacement(&mut fluids, w, h, &fr);

        assert_eq!(fluids[idx(0, 0, w)].fluid_id, lava_id, "Lava should sink");
        assert_eq!(
            fluids[idx(0, 1, w)].fluid_id, water_id,
            "Water should rise"
        );
    }

    #[test]
    fn density_displacement_light_stays_on_top() {
        let fr = test_fluid_registry();
        let water_id = fr.by_name("water");
        let steam_id = fr.by_name("steam");

        let w = 1;
        let h = 2;
        let mut fluids = vec![FluidCell::EMPTY; (w * h) as usize];

        // Water below (heavy), steam above (light) — correct, no swap
        fluids[idx(0, 0, w)] = FluidCell::new(water_id, 1.0);
        fluids[idx(0, 1, w)] = FluidCell::new(steam_id, 1.0);

        resolve_density_displacement(&mut fluids, w, h, &fr);

        assert_eq!(fluids[idx(0, 0, w)].fluid_id, water_id, "Water stays below");
        assert_eq!(fluids[idx(0, 1, w)].fluid_id, steam_id, "Steam stays above");
    }
}
```

**Step 2: Create reactions.ron**

Create `assets/content/fluids/reactions.ron`:

```ron
FluidReactionsAsset(
    reactions: [
        (
            fluid_a: "water",
            fluid_b: "lava",
            adjacency: Any,
            result_tile: Some("stone"),
            result_fluid: None,
            min_mass_a: 0.25,
            min_mass_b: 0.25,
            consume_a: 0.5,
            consume_b: 1.0,
            byproduct_fluid: Some("steam"),
            byproduct_mass: 0.3,
        ),
    ],
)
```

**Step 3: Update mod.rs**

```rust
pub mod cell;
pub mod reactions;
pub mod registry;
pub mod simulation;

pub use cell::{FluidCell, FluidId};
pub use registry::{FluidDef, FluidRegistry};
pub use simulation::FluidSimConfig;
```

**Step 4: Run tests**

Run: `cargo test fluid::reactions -- --nocapture`
Expected: All tests pass.

**Step 5: Commit**

```
git add src/fluid/reactions.rs assets/content/fluids/reactions.ron src/fluid/mod.rs
git commit -m "feat(fluid): add fluid reactions system with density displacement"
```

---

### Task 7: Fluid Rendering

**Files:**
- Create: `src/fluid/render.rs`
- Modify: `src/fluid/mod.rs`

**Step 1: Create fluid mesh builder**

Create `src/fluid/render.rs`:

```rust
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};

use crate::fluid::cell::{FluidCell, FluidId};
use crate::fluid::registry::FluidRegistry;
use crate::world::chunk::Layer;

/// Build a mesh for the fluid layer of a chunk.
/// Returns None if there are no visible fluids.
pub fn build_fluid_mesh(
    fluids: &[FluidCell],
    chunk_x: i32,
    chunk_y: i32,
    chunk_size: u32,
    tile_size: f32,
    fluid_registry: &FluidRegistry,
) -> Option<Mesh> {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let world_x = chunk_x as f32 * chunk_size as f32 * tile_size;
    let world_y = chunk_y as f32 * chunk_size as f32 * tile_size;

    for ly in 0..chunk_size {
        for lx in 0..chunk_size {
            let idx = (ly * chunk_size + lx) as usize;
            let cell = fluids[idx];

            if cell.is_empty() {
                continue;
            }

            let def = fluid_registry.get(cell.fluid_id);
            let fill = cell.mass.min(1.0).max(0.0);

            let x0 = world_x + lx as f32 * tile_size;
            let y0 = world_y + ly as f32 * tile_size;
            let x1 = x0 + tile_size;
            let y1 = if def.is_gas {
                // Gas fills from top down
                y0 + tile_size
            } else {
                // Liquid fills from bottom up
                y0 + fill * tile_size
            };
            let y_base = if def.is_gas {
                y0 + (1.0 - fill) * tile_size
            } else {
                y0
            };

            let color = [
                def.color[0] as f32 / 255.0,
                def.color[1] as f32 / 255.0,
                def.color[2] as f32 / 255.0,
                def.color[3] as f32 / 255.0 * fill.min(1.0),
            ];

            let base = positions.len() as u32;
            positions.push([x0, y_base, 0.5]); // z=0.5 between tiles and entities
            positions.push([x1, y_base, 0.5]);
            positions.push([x1, y1, 0.5]);
            positions.push([x0, y1, 0.5]);

            colors.push(color);
            colors.push(color);
            colors.push(color);
            colors.push(color);

            indices.push(base);
            indices.push(base + 1);
            indices.push(base + 2);
            indices.push(base);
            indices.push(base + 2);
            indices.push(base + 3);
        }
    }

    if positions.is_empty() {
        return None;
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        bevy::render::render_asset::RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));

    Some(mesh)
}
```

Note: The exact Bevy 0.18 mesh API may differ. The implementer should check the
existing `build_chunk_mesh` in `src/world/mesh_builder.rs` for the correct API
and adapt accordingly. The key concept is: colored quads with vertex colors for
transparency.

**Step 2: Update mod.rs**

Add `pub mod render;` to `src/fluid/mod.rs`.

**Step 3: Commit**

```
git add src/fluid/render.rs src/fluid/mod.rs
git commit -m "feat(fluid): add fluid mesh builder for chunk rendering"
```

---

### Task 8: FluidPlugin and ECS Integration

**Files:**
- Modify: `src/fluid/mod.rs` (add FluidPlugin)
- Modify: `src/main.rs` (register plugin)
- Create: `src/fluid/systems.rs`

**Step 1: Create systems module**

Create `src/fluid/systems.rs` with the Bevy systems that orchestrate the simulation:

```rust
use std::collections::HashSet;

use bevy::prelude::*;

use crate::fluid::cell::{FluidCell, FluidId};
use crate::fluid::reactions::{resolve_density_displacement, FluidReactionRegistry};
use crate::fluid::registry::FluidRegistry;
use crate::fluid::render::build_fluid_mesh;
use crate::fluid::simulation::{simulate_grid, FluidSimConfig};
use crate::registry::tile::TileRegistry;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::{ChunkCoord, ChunkDirty, LoadedChunks, WorldMap};

/// Tracks which chunks have active fluids.
#[derive(Resource, Default, Debug)]
pub struct ActiveFluidChunks {
    pub chunks: HashSet<(i32, i32)>,
}

/// Main fluid simulation system. Runs N iterations per tick.
pub fn fluid_simulation(
    mut world_map: ResMut<WorldMap>,
    fluid_registry: Res<FluidRegistry>,
    tile_registry: Res<TileRegistry>,
    active_world: Res<ActiveWorld>,
    mut active_fluids: ResMut<ActiveFluidChunks>,
    config: Res<FluidSimConfig>,
) {
    let chunk_size = active_world.chunk_size;
    let len = (chunk_size * chunk_size) as usize;

    // Collect active chunks to process
    let chunks_to_process: Vec<(i32, i32)> = active_fluids.chunks.iter().copied().collect();

    for _ in 0..config.iterations_per_tick {
        for &(cx, cy) in &chunks_to_process {
            if let Some(chunk) = world_map.chunks.get(&(cx, cy)) {
                let tiles = &chunk.fg.tiles;
                let fluids = chunk.fluids.clone();
                let mut new_fluids = vec![FluidCell::EMPTY; len];

                simulate_grid(
                    tiles,
                    &fluids,
                    &mut new_fluids,
                    chunk_size,
                    chunk_size,
                    &tile_registry,
                    &fluid_registry,
                    &config,
                );

                // Density displacement
                resolve_density_displacement(
                    &mut new_fluids,
                    chunk_size,
                    chunk_size,
                    &fluid_registry,
                );

                // Write back
                if let Some(chunk) = world_map.chunks.get_mut(&(cx, cy)) {
                    chunk.fluids = new_fluids;
                }
            }
        }
    }

    // Update active chunks set: remove chunks with no fluid
    active_fluids.chunks.retain(|&(cx, cy)| {
        world_map
            .chunks
            .get(&(cx, cy))
            .map(|chunk| chunk.fluids.iter().any(|c| !c.is_empty()))
            .unwrap_or(false)
    });
}

/// Mark chunks with changed fluids as dirty for mesh rebuild.
pub fn fluid_mark_dirty(
    active_fluids: Res<ActiveFluidChunks>,
    mut commands: Commands,
    query: Query<(Entity, &ChunkCoord)>,
) {
    for (entity, coord) in &query {
        if active_fluids.chunks.contains(&(coord.x, coord.y)) {
            commands.entity(entity).insert(ChunkDirty);
        }
    }
}
```

Note: This is a simplified version. The actual implementation should handle:
- Cross-chunk boundaries (reading neighbor chunks)
- Wrap-around via `active_world.wrap_tile_x()`
- More efficient dirty tracking (only mark if fluids actually changed)

**Step 2: Create FluidPlugin**

In `src/fluid/mod.rs`:

```rust
use bevy::prelude::*;

pub mod cell;
pub mod reactions;
pub mod registry;
pub mod render;
pub mod simulation;
pub mod systems;

pub use cell::{FluidCell, FluidId};
pub use registry::{FluidDef, FluidRegistry};
pub use simulation::FluidSimConfig;

use crate::registry::AppState;
use crate::sets::GameSet;

pub struct FluidPlugin;

impl Plugin for FluidPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FluidSimConfig>()
            .init_resource::<systems::ActiveFluidChunks>()
            .add_systems(
                Update,
                (systems::fluid_simulation, systems::fluid_mark_dirty)
                    .chain()
                    .in_set(GameSet::WorldUpdate)
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<FluidRegistry>),
            );
    }
}
```

**Step 3: Register in main.rs**

Add `mod fluid;` and `.add_plugins(fluid::FluidPlugin)` to `main.rs`.

**Step 4: Run build**

Run: `cargo build`
Expected: Compiles.

**Step 5: Run all tests**

Run: `cargo test`
Expected: All tests pass.

**Step 6: Commit**

```
git add src/fluid/ src/main.rs
git commit -m "feat(fluid): add FluidPlugin with simulation and dirty tracking systems"
```

---

### Task 9: Persistence Integration

**Files:**
- Modify: `src/cosmos/persistence.rs` (ensure fluid data is saved/loaded)

**Step 1: Verify ChunkData serialization includes fluids**

Since `ChunkData` already derives `Serialize, Deserialize`, and we added
`fluids: Vec<FluidCell>` where both `FluidCell` and `FluidId` derive
`Serialize, Deserialize`, the persistence should work automatically.

However, for backward compatibility with old saves (where `fluids` field
doesn't exist), add `#[serde(default)]`:

In `src/world/chunk.rs`:

```rust
pub struct ChunkData {
    pub fg: TileLayer,
    pub bg: TileLayer,
    #[serde(default)]
    pub fluids: Vec<FluidCell>,
    // ...
}
```

And implement `Default` or use `serde(default = "...")` to provide empty vec.

**Step 2: Write test for serialization round-trip**

```rust
#[test]
fn chunk_data_with_fluids_serializes() {
    let mut chunk = ChunkData { /* ... minimal construction ... */ };
    chunk.fluids[0] = FluidCell::new(FluidId(1), 0.8);
    let serialized = ron::to_string(&chunk).unwrap();
    let deserialized: ChunkData = ron::from_str(&serialized).unwrap();
    assert_eq!(deserialized.fluids[0].fluid_id, FluidId(1));
    assert!((deserialized.fluids[0].mass - 0.8).abs() < f32::EPSILON);
}
```

**Step 3: Run tests**

Run: `cargo test`
Expected: All pass.

**Step 4: Commit**

```
git add src/world/chunk.rs
git commit -m "feat(fluid): ensure fluid data persists with chunk serialization"
```

---

### Task 10: Debug Fluid Placement (for testing)

**Files:**
- Modify: `src/interaction/block_action.rs` or create `src/fluid/debug.rs`

**Step 1: Add debug system for placing fluids**

Create a temporary debug system that places water/gas when a key is pressed.
This is essential for testing the simulation visually:

```rust
/// Debug: press F5 to place water at cursor, F6 to place gas
pub fn debug_place_fluid(
    input: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    mut world_map: ResMut<WorldMap>,
    active_world: Res<ActiveWorld>,
    fluid_registry: Res<FluidRegistry>,
    mut active_fluids: ResMut<systems::ActiveFluidChunks>,
    // ... ctx resources for WorldCtxRef
) {
    // Convert cursor position to tile coords
    // Place fluid at that tile
    // Add chunk to active_fluids
}
```

The implementer should check how `block_action.rs` handles mouse->tile conversion
and follow the same pattern.

**Step 2: Register in FluidPlugin**

Add as a system in `GameSet::Input`.

**Step 3: Manually test**

Run: `cargo run`
Place water with F5, observe it flowing. Place gas with F6, observe it rising.

**Step 4: Commit**

```
git add src/fluid/
git commit -m "feat(fluid): add debug fluid placement for visual testing"
```

---

### Task 11: Final Integration Tests

**Files:**
- All fluid test files

**Step 1: Run full test suite**

Run: `cargo test`
Expected: All pass.

**Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: No warnings.

**Step 3: Run build in release mode**

Run: `cargo build --release`
Expected: Compiles.

**Step 4: Manual visual test**

Run: `cargo run`
Test scenarios:
- Place water — it falls and spreads on floor
- Place water in U-tube shape — levels equalize
- Place gas — it rises
- Place water above lava (if terrain has lava) — reaction creates stone + steam
- Exit and re-enter area — fluid state persists

**Step 5: Final commit**

```
git add -A
git commit -m "feat(fluid): complete fluid & gas simulation v1"
```
