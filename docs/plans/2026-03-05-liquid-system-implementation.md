# Liquid System (Pipe Model) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a tile-based liquid simulation with full hydrostatic pressure, multi-liquid support (water, lava, oil), reactions, and visual rendering integrated with the existing world.

**Architecture:** Pipe Model — each tile stores liquid type + level, each face stores flow rate. Pressure computed from hydrostatic depth, flows driven by pressure differences. Sleep optimization for stable regions. Separate liquid mesh layer per chunk with shader-based surface smoothing and temporal interpolation.

**Tech Stack:** Rust, Bevy 0.18, custom Material2d shader, RON for liquid definitions.

**Design doc:** `docs/plans/2026-03-05-liquid-system-design.md`

---

### Task 1: Liquid Data Model

**Files:**
- Create: `src/liquid/mod.rs`
- Create: `src/liquid/data.rs`
- Create: `src/liquid/registry.rs`
- Create: `assets/worlds/liquids.registry.ron`
- Modify: `src/main.rs` (add LiquidPlugin)

**Step 1: Create the liquid module with core data types**

Create `src/liquid/mod.rs`:
```rust
mod data;
mod registry;

pub use data::*;
pub use registry::*;

use bevy::prelude::*;

pub struct LiquidPlugin;

impl Plugin for LiquidPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LiquidRegistryRes>();
    }
}
```

Create `src/liquid/data.rs`:
```rust
use serde::{Deserialize, Serialize};

/// Index into the liquid registry. 0 = no liquid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct LiquidId(pub u8);

impl LiquidId {
    pub const NONE: LiquidId = LiquidId(0);

    pub fn is_none(self) -> bool {
        self.0 == 0
    }
}

/// Per-tile liquid state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LiquidCell {
    pub liquid_type: LiquidId,
    pub level: f32,
}

impl LiquidCell {
    pub const EMPTY: LiquidCell = LiquidCell {
        liquid_type: LiquidId::NONE,
        level: 0.0,
    };

    pub fn is_empty(&self) -> bool {
        self.liquid_type.is_none() || self.level < MIN_LEVEL
    }
}

/// Minimum level below which a cell is considered empty.
pub const MIN_LEVEL: f32 = 0.001;
/// Maximum level a cell can hold (visual cap = 1.0, but pressure can push higher).
pub const MAX_LEVEL: f32 = 1.0;
/// Maximum flow per face per step.
pub const MAX_FLOW: f32 = 0.5;

/// Flow state for a single cell — not persisted, recomputed each frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct FlowCell {
    /// Flows through faces: [right, up, left, down].
    /// Positive = outward from this cell.
    pub flow: [f32; 4],
}

/// Face indices.
pub const FACE_RIGHT: usize = 0;
pub const FACE_UP: usize = 1;
pub const FACE_LEFT: usize = 2;
pub const FACE_DOWN: usize = 3;

/// Opposite face lookup.
pub const OPPOSITE_FACE: [usize; 4] = [FACE_LEFT, FACE_DOWN, FACE_RIGHT, FACE_UP];

/// Tile offsets for each face direction: [right, up, left, down].
pub const FACE_OFFSET: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
```

**Step 2: Create liquid registry**

Create `src/liquid/registry.rs`:
```rust
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::data::LiquidId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidReaction {
    /// Name of the other liquid (resolved to LiquidId at load time).
    pub other: String,
    /// Name of the tile to produce (e.g. "obsidian").
    pub produce_tile: Option<String>,
    /// Whether both liquids are consumed.
    pub consume_both: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidDef {
    pub name: String,
    pub density: f32,
    pub viscosity: f32,
    pub color: [f32; 4], // RGBA
    pub damage_on_contact: f32,
    pub light_emission: [u8; 3],
    pub light_opacity: u8,
    pub swim_speed_factor: f32,
    #[serde(default)]
    pub reactions: Vec<LiquidReaction>,
}

#[derive(Resource, Default)]
pub struct LiquidRegistryRes {
    pub defs: Vec<LiquidDef>,
    name_to_id: HashMap<String, LiquidId>,
    /// Reaction lookup: (a, b) -> index into defs[a].reactions.
    reaction_cache: HashMap<(u8, u8), usize>,
}

impl LiquidRegistryRes {
    pub fn from_defs(defs: Vec<LiquidDef>) -> Self {
        let mut name_to_id = HashMap::new();
        for (i, def) in defs.iter().enumerate() {
            // LiquidId(0) = NONE, so actual liquids start at 1.
            name_to_id.insert(def.name.clone(), LiquidId((i + 1) as u8));
        }

        let mut reaction_cache = HashMap::new();
        for (i, def) in defs.iter().enumerate() {
            let a = (i + 1) as u8;
            for (ri, reaction) in def.reactions.iter().enumerate() {
                if let Some(&b_id) = name_to_id.get(&reaction.other) {
                    reaction_cache.insert((a, b_id.0), ri);
                }
            }
        }

        Self {
            defs,
            name_to_id,
            reaction_cache,
        }
    }

    pub fn get(&self, id: LiquidId) -> Option<&LiquidDef> {
        if id.is_none() {
            return None;
        }
        self.defs.get((id.0 - 1) as usize)
    }

    pub fn by_name(&self, name: &str) -> LiquidId {
        self.name_to_id.get(name).copied().unwrap_or(LiquidId::NONE)
    }

    pub fn density(&self, id: LiquidId) -> f32 {
        self.get(id).map(|d| d.density).unwrap_or(0.0)
    }

    pub fn viscosity(&self, id: LiquidId) -> f32 {
        self.get(id).map(|d| d.viscosity).unwrap_or(1.0)
    }

    pub fn get_reaction(&self, a: LiquidId, b: LiquidId) -> Option<&LiquidReaction> {
        let idx = self.reaction_cache.get(&(a.0, b.0))?;
        let def = self.get(a)?;
        def.reactions.get(*idx)
    }
}
```

**Step 3: Create RON asset file**

Create `assets/worlds/liquids.registry.ron`:
```ron
[
    (
        name: "water",
        density: 1.0,
        viscosity: 1.0,
        color: [0.2, 0.4, 0.8, 0.6],
        damage_on_contact: 0.0,
        light_emission: [0, 0, 0],
        light_opacity: 20,
        swim_speed_factor: 0.5,
        reactions: [
            (other: "lava", produce_tile: Some("obsidian"), consume_both: true),
        ],
    ),
    (
        name: "lava",
        density: 3.0,
        viscosity: 4.0,
        color: [1.0, 0.3, 0.0, 1.0],
        damage_on_contact: 20.0,
        light_emission: [255, 120, 40],
        light_opacity: 200,
        swim_speed_factor: 0.2,
        reactions: [
            (other: "water", produce_tile: Some("obsidian"), consume_both: true),
            (other: "oil", produce_tile: None, consume_both: false),
        ],
    ),
    (
        name: "oil",
        density: 0.8,
        viscosity: 2.0,
        color: [0.15, 0.1, 0.05, 0.85],
        damage_on_contact: 0.0,
        light_emission: [0, 0, 0],
        light_opacity: 180,
        swim_speed_factor: 0.35,
        reactions: [],
    ),
]
```

**Step 4: Register plugin in main.rs**

Add `mod liquid;` and `liquid::LiquidPlugin` to `main.rs` plugin list, after `world::WorldPlugin`.

**Step 5: Build and verify**

Run: `cargo build 2>&1 | head -30`
Expected: Compiles successfully.

**Step 6: Commit**

```
git add src/liquid/ assets/worlds/liquids.registry.ron src/main.rs
git commit -m "feat(liquid): add data model, registry, and liquid definitions"
```

---

### Task 2: Liquid Layer in Chunks

**Files:**
- Modify: `src/world/chunk.rs` (add LiquidLayer to ChunkData, liquid accessors to WorldMap)
- Modify: `src/liquid/data.rs` (add LiquidLayer struct)

**Step 1: Add LiquidLayer struct**

In `src/liquid/data.rs`, add:
```rust
/// Per-chunk liquid storage. Same layout as TileLayer: row-major local_y * chunk_size + local_x.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidLayer {
    pub cells: Vec<LiquidCell>,
}

impl LiquidLayer {
    pub fn new_empty(len: usize) -> Self {
        Self {
            cells: vec![LiquidCell::EMPTY; len],
        }
    }

    pub fn get(&self, local_x: u32, local_y: u32, chunk_size: u32) -> LiquidCell {
        self.cells[(local_y * chunk_size + local_x) as usize]
    }

    pub fn set(&mut self, local_x: u32, local_y: u32, cell: LiquidCell, chunk_size: u32) {
        self.cells[(local_y * chunk_size + local_x) as usize] = cell;
    }

    /// Returns true if any cell has liquid.
    pub fn has_liquid(&self) -> bool {
        self.cells.iter().any(|c| !c.is_empty())
    }
}
```

**Step 2: Add liquid field to ChunkData**

In `src/world/chunk.rs`, add to `ChunkData` struct (line ~80):
```rust
pub liquid: LiquidLayer,
```

Update all places where `ChunkData` is constructed to include `liquid: LiquidLayer::new_empty(size)`.

**Step 3: Add liquid accessors to WorldMap**

In `src/world/chunk.rs`, add methods to `WorldMap`:
```rust
pub fn get_liquid(&self, tile_x: i32, tile_y: i32, ctx: &WorldCtxRef) -> LiquidCell {
    let tx = ctx.config.wrap_tile_x(tile_x);
    if tile_y < 0 || tile_y >= ctx.config.height_tiles {
        return LiquidCell::EMPTY;
    }
    let (cx, cy) = tile_to_chunk(tx, tile_y, ctx.config.chunk_size);
    let (lx, ly) = tile_to_local(tx, tile_y, ctx.config.chunk_size);
    match self.chunk(cx, cy) {
        Some(chunk) => chunk.liquid.get(lx, ly, ctx.config.chunk_size),
        None => LiquidCell::EMPTY,
    }
}

pub fn set_liquid(&mut self, tile_x: i32, tile_y: i32, cell: LiquidCell, ctx: &WorldCtxRef) {
    let tx = ctx.config.wrap_tile_x(tile_x);
    if tile_y < 0 || tile_y >= ctx.config.height_tiles {
        return;
    }
    let (cx, cy) = tile_to_chunk(tx, tile_y, ctx.config.chunk_size);
    let (lx, ly) = tile_to_local(tx, tile_y, ctx.config.chunk_size);
    if let Some(chunk) = self.chunk_mut(cx, cy) {
        chunk.liquid.set(lx, ly, cell, ctx.config.chunk_size);
    }
}
```

**Step 4: Build and verify**

Run: `cargo build 2>&1 | head -30`
Expected: Compiles. Fix any ChunkData construction sites that now miss the `liquid` field.

**Step 5: Commit**

```
git add src/liquid/data.rs src/world/chunk.rs
git commit -m "feat(liquid): add LiquidLayer to ChunkData with world accessors"
```

---

### Task 3: Pipe Model Simulation Core

**Files:**
- Create: `src/liquid/simulation.rs`
- Modify: `src/liquid/mod.rs`

**Step 1: Write unit tests for simulation logic**

At the bottom of `src/liquid/simulation.rs`, add `#[cfg(test)] mod tests` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a small grid for testing.
    fn make_grid(width: usize, height: usize) -> SimGrid {
        SimGrid::new(width, height)
    }

    #[test]
    fn water_falls_down() {
        let mut grid = make_grid(3, 3);
        // Place water at top-center (1, 2).
        grid.set(1, 2, LiquidCell { liquid_type: LiquidId(1), level: 1.0 });
        // One step: water should flow down.
        let densities = [1.0]; // water density
        let viscosities = [1.0];
        step(&mut grid, &densities, &viscosities, 0.05);
        // Top cell should have lost some water.
        assert!(grid.get(1, 2).level < 1.0);
        // Cell below (1, 1) should have gained water.
        assert!(grid.get(1, 1).level > 0.0);
    }

    #[test]
    fn water_spreads_horizontally() {
        let mut grid = make_grid(5, 2);
        // Solid floor at y=0.
        for x in 0..5 { grid.set_solid(x, 0, true); }
        // Water column at center.
        grid.set(2, 1, LiquidCell { liquid_type: LiquidId(1), level: 1.0 });
        let densities = [1.0];
        let viscosities = [1.0];
        // Several steps to let it spread.
        for _ in 0..20 {
            step(&mut grid, &densities, &viscosities, 0.05);
        }
        // Should have spread to neighbors.
        assert!(grid.get(1, 1).level > 0.0);
        assert!(grid.get(3, 1).level > 0.0);
    }

    #[test]
    fn pressure_u_tube() {
        // U-tube: solid walls with gap at bottom.
        // Left column: water, right column: empty.
        // Water should flow through bottom and rise on right.
        let mut grid = make_grid(5, 6);
        // Floor.
        for x in 0..5 { grid.set_solid(x, 0, true); }
        // Walls: center column is solid except bottom.
        for y in 2..6 { grid.set_solid(2, y, true); }
        // Fill left column (x=0,1) with water from y=1 to y=5.
        for y in 1..6 {
            for x in 0..2 {
                grid.set(x, y, LiquidCell { liquid_type: LiquidId(1), level: 1.0 });
            }
        }
        let densities = [1.0];
        let viscosities = [1.0];
        // Many steps for pressure to propagate.
        for _ in 0..200 {
            step(&mut grid, &densities, &viscosities, 0.05);
        }
        // Right side (x=3,4) should have water — pressure pushed it through bottom gap.
        let right_water: f32 = (1..6).map(|y| grid.get(3, y).level).sum();
        assert!(right_water > 1.0, "Water should flow through U-tube, got {right_water}");
    }

    #[test]
    fn oil_floats_on_water() {
        let mut grid = make_grid(3, 5);
        // Floor.
        for x in 0..3 { grid.set_solid(x, 0, true); }
        // Water in bottom cells.
        for y in 1..3 {
            grid.set(1, y, LiquidCell { liquid_type: LiquidId(1), level: 1.0 }); // water
        }
        // Oil on top of water.
        grid.set(1, 3, LiquidCell { liquid_type: LiquidId(2), level: 1.0 }); // oil
        // densities: water=1.0, oil=0.8
        let densities = [1.0, 0.8];
        let viscosities = [1.0, 2.0];
        for _ in 0..50 {
            step(&mut grid, &densities, &viscosities, 0.05);
        }
        // Oil should remain above water (lighter density).
        // Find highest cell with oil — should be above highest cell with water.
        let highest_oil = (0..5).rev().find(|&y| {
            let c = grid.get(1, y);
            c.liquid_type == LiquidId(2) && c.level > MIN_LEVEL
        });
        let highest_water = (0..5).rev().find(|&y| {
            let c = grid.get(1, y);
            c.liquid_type == LiquidId(1) && c.level > MIN_LEVEL
        });
        assert!(highest_oil > highest_water, "Oil should float above water");
    }

    #[test]
    fn conservation_of_volume() {
        let mut grid = make_grid(5, 5);
        for x in 0..5 { grid.set_solid(x, 0, true); }
        // Place some water.
        grid.set(2, 3, LiquidCell { liquid_type: LiquidId(1), level: 1.0 });
        grid.set(2, 2, LiquidCell { liquid_type: LiquidId(1), level: 0.5 });
        let initial_volume: f32 = grid.total_volume();
        let densities = [1.0];
        let viscosities = [1.0];
        for _ in 0..100 {
            step(&mut grid, &densities, &viscosities, 0.05);
        }
        let final_volume = grid.total_volume();
        let diff = (initial_volume - final_volume).abs();
        assert!(diff < 0.01, "Volume should be conserved, diff={diff}");
    }
}
```

**Step 2: Implement SimGrid and step function**

Create `src/liquid/simulation.rs`:

```rust
use super::data::*;

/// Standalone simulation grid for the pipe model.
/// Used both as the actual simulation substrate (operating on loaded chunks)
/// and for unit testing.
pub struct SimGrid {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<LiquidCell>,
    pub solid: Vec<bool>,
    pub flows: Vec<FlowCell>,
}

const GRAVITY: f32 = 9.8;
const GRAVITY_BIAS: f32 = 2.0;

impl SimGrid {
    pub fn new(width: usize, height: usize) -> Self {
        let len = width * height;
        Self {
            width,
            height,
            cells: vec![LiquidCell::EMPTY; len],
            solid: vec![false; len],
            flows: vec![FlowCell::default(); len],
        }
    }

    fn idx(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }

    pub fn get(&self, x: usize, y: usize) -> LiquidCell {
        self.cells[self.idx(x, y)]
    }

    pub fn set(&mut self, x: usize, y: usize, cell: LiquidCell) {
        let i = self.idx(x, y);
        self.cells[i] = cell;
    }

    pub fn set_solid(&mut self, x: usize, y: usize, solid: bool) {
        let i = self.idx(x, y);
        self.solid[i] = solid;
    }

    pub fn is_solid(&self, x: usize, y: usize) -> bool {
        self.solid[self.idx(x, y)]
    }

    pub fn total_volume(&self) -> f32 {
        self.cells.iter().map(|c| c.level).sum()
    }

    /// Get neighbor coordinates for a face, returning None if out of bounds.
    fn neighbor(&self, x: usize, y: usize, face: usize) -> Option<(usize, usize)> {
        let (dx, dy) = FACE_OFFSET[face];
        let nx = x as i32 + dx;
        let ny = y as i32 + dy;
        if nx < 0 || nx >= self.width as i32 || ny < 0 || ny >= self.height as i32 {
            return None;
        }
        Some((nx as usize, ny as usize))
    }
}

/// Compute hydrostatic pressure for a cell.
fn pressure(level: f32, density: f32, depth_above: f32) -> f32 {
    level + density * GRAVITY * depth_above * 0.01
}

/// Run one simulation step on the grid.
pub fn step(
    grid: &mut SimGrid,
    densities: &[f32],   // indexed by (LiquidId.0 - 1)
    viscosities: &[f32], // indexed by (LiquidId.0 - 1)
    dt: f32,
) {
    let w = grid.width;
    let h = grid.height;

    // Phase 1: Compute pressure per cell.
    // Scan columns top-down to compute depth.
    let mut pressures = vec![0.0f32; w * h];
    for x in 0..w {
        let mut depth: f32 = 0.0;
        for y in (0..h).rev() {
            let i = y * w + x;
            let cell = grid.cells[i];
            if cell.is_empty() || grid.solid[i] {
                depth = 0.0;
                continue;
            }
            let lid = (cell.liquid_type.0 - 1) as usize;
            let density = densities.get(lid).copied().unwrap_or(1.0);
            pressures[i] = pressure(cell.level, density, depth);
            depth += cell.level;
        }
    }

    // Phase 2: Compute flows.
    // Reset flows.
    for f in grid.flows.iter_mut() {
        *f = FlowCell::default();
    }

    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if grid.solid[i] {
                continue;
            }
            let cell = grid.cells[i];
            if cell.is_empty() {
                continue;
            }
            let lid = (cell.liquid_type.0 - 1) as usize;
            let visc = viscosities.get(lid).copied().unwrap_or(1.0);

            for face in 0..4 {
                let Some((nx, ny)) = grid.neighbor(x, y, face) else {
                    continue;
                };
                let ni = ny * w + nx;
                if grid.solid[ni] {
                    continue;
                }

                let n_cell = grid.cells[ni];

                // Block flow into cell occupied by different, denser liquid.
                if !n_cell.is_empty() && n_cell.liquid_type != cell.liquid_type {
                    let n_lid = (n_cell.liquid_type.0 - 1) as usize;
                    let n_density = densities.get(n_lid).copied().unwrap_or(1.0);
                    let my_density = densities.get(lid).copied().unwrap_or(1.0);
                    // Can only flow into cell with lighter or same liquid.
                    if n_density > my_density {
                        continue;
                    }
                }

                let pressure_diff = pressures[i] - pressures[ni];

                // Gravity bias: prefer downward flow.
                let gravity_bonus = if face == FACE_DOWN {
                    GRAVITY_BIAS
                } else if face == FACE_UP {
                    -GRAVITY_BIAS * 0.5
                } else {
                    0.0
                };

                let flow = dt * (pressure_diff + gravity_bonus) / visc;
                if flow > 0.0 {
                    grid.flows[i].flow[face] += flow.min(MAX_FLOW);
                }
            }
        }
    }

    // Clamp outgoing flows to not exceed cell level.
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let cell = grid.cells[i];
            if cell.is_empty() {
                continue;
            }
            let total_out: f32 = grid.flows[i].flow.iter().filter(|&&f| f > 0.0).sum();
            if total_out > cell.level {
                let scale = cell.level / total_out;
                for f in &mut grid.flows[i].flow {
                    if *f > 0.0 {
                        *f *= scale;
                    }
                }
            }
        }
    }

    // Phase 3: Apply flows — update levels.
    // We need a copy because we read neighbors while writing.
    let mut new_cells = grid.cells.clone();

    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if grid.solid[i] {
                continue;
            }

            // Subtract outgoing.
            let out: f32 = grid.flows[i].flow.iter().filter(|&&f| f > 0.0).sum();
            if out > 0.0 && !grid.cells[i].is_empty() {
                new_cells[i].level -= out;
            }

            // Add incoming from neighbors.
            for face in 0..4 {
                let Some((nx, ny)) = grid.neighbor(x, y, face) else {
                    continue;
                };
                let ni = ny * w + nx;
                let opposite = OPPOSITE_FACE[face];
                let incoming = grid.flows[ni].flow[opposite];
                if incoming > 0.0 {
                    let src_type = grid.cells[ni].liquid_type;
                    if new_cells[i].is_empty() {
                        new_cells[i].liquid_type = src_type;
                        new_cells[i].level = incoming;
                    } else if new_cells[i].liquid_type == src_type {
                        new_cells[i].level += incoming;
                    }
                    // Different liquid type handling (displacement) happens via flow blocking above.
                }
            }

            // Clean up tiny amounts.
            if new_cells[i].level < MIN_LEVEL {
                new_cells[i] = LiquidCell::EMPTY;
            }
            // Clamp max.
            if new_cells[i].level > MAX_LEVEL {
                new_cells[i].level = MAX_LEVEL;
            }
        }
    }

    grid.cells = new_cells;
}
```

**Step 3: Run tests**

Run: `cargo test -p starbeam -- liquid::simulation`
Expected: All 5 tests pass. Tune constants (GRAVITY_BIAS, pressure scaling) if U-tube or float tests fail.

**Step 4: Commit**

```
git add src/liquid/simulation.rs src/liquid/mod.rs
git commit -m "feat(liquid): pipe model simulation core with unit tests"
```

---

### Task 4: Sleep Optimization

**Files:**
- Create: `src/liquid/sleep.rs`
- Modify: `src/liquid/mod.rs`

**Step 1: Implement sleep tracker**

Create `src/liquid/sleep.rs`:

```rust
use std::collections::HashSet;

/// Tracks which tiles are "active" (need simulation).
/// Sleeping tiles are not processed until woken.
#[derive(Default)]
pub struct SleepTracker {
    /// Set of active tile coordinates (world tile_x, tile_y).
    active: HashSet<(i32, i32)>,
    /// Tiles that have been stable for consecutive steps.
    /// Key = (tile_x, tile_y), value = consecutive stable steps.
    stable_count: HashMap<(i32, i32), u8>,
}

use std::collections::HashMap;

const SLEEP_THRESHOLD: u8 = 5; // steps with no change before sleeping
const MAX_ACTIVE_PER_STEP: usize = 20_000;

impl SleepTracker {
    pub fn wake(&mut self, tile_x: i32, tile_y: i32) {
        self.active.insert((tile_x, tile_y));
        self.stable_count.remove(&(tile_x, tile_y));
    }

    pub fn wake_region(&mut self, min_x: i32, min_y: i32, max_x: i32, max_y: i32) {
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                self.wake(x, y);
            }
        }
    }

    /// Wake a tile and its 4 neighbors.
    pub fn wake_with_neighbors(&mut self, tile_x: i32, tile_y: i32) {
        self.wake(tile_x, tile_y);
        self.wake(tile_x + 1, tile_y);
        self.wake(tile_x - 1, tile_y);
        self.wake(tile_x, tile_y + 1);
        self.wake(tile_x, tile_y - 1);
    }

    /// Mark a tile as stable this step. If it has been stable long enough, put it to sleep.
    pub fn mark_stable(&mut self, tile_x: i32, tile_y: i32) {
        let count = self.stable_count.entry((tile_x, tile_y)).or_insert(0);
        *count = count.saturating_add(1);
        if *count >= SLEEP_THRESHOLD {
            self.active.remove(&(tile_x, tile_y));
            self.stable_count.remove(&(tile_x, tile_y));
        }
    }

    /// Mark a tile as changed this step — reset its stable count.
    pub fn mark_changed(&mut self, tile_x: i32, tile_y: i32) {
        self.stable_count.remove(&(tile_x, tile_y));
        // Also wake neighbors since they might need to react.
        self.wake_with_neighbors(tile_x, tile_y);
    }

    /// Iterator over active tiles, capped at MAX_ACTIVE_PER_STEP.
    pub fn active_tiles(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.active.iter().copied().take(MAX_ACTIVE_PER_STEP)
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Remove tiles that are outside the simulation radius.
    pub fn cull_outside(&mut self, min_x: i32, min_y: i32, max_x: i32, max_y: i32) {
        self.active.retain(|&(x, y)| x >= min_x && x <= max_x && y >= min_y && y <= max_y);
    }
}
```

**Step 2: Build and verify**

Run: `cargo build 2>&1 | head -20`

**Step 3: Commit**

```
git add src/liquid/sleep.rs src/liquid/mod.rs
git commit -m "feat(liquid): sleep tracker for active/inactive cell optimization"
```

---

### Task 5: World Simulation System

**Files:**
- Create: `src/liquid/system.rs`
- Modify: `src/liquid/mod.rs`
- Modify: `src/world/mod.rs` (scheduling)

**Step 1: Create the Bevy system that runs simulation on loaded chunks**

Create `src/liquid/system.rs`:

```rust
use bevy::prelude::*;

use crate::liquid::data::*;
use crate::liquid::registry::LiquidRegistryRes;
use crate::liquid::sleep::SleepTracker;
use crate::registry::world::ActiveWorld;
use crate::world::chunk::{self, WorldMap, DirtyChunks};
use crate::world::ctx::WorldCtx;

/// Resource: liquid simulation state.
#[derive(Resource)]
pub struct LiquidSimState {
    pub sleep: SleepTracker,
    /// Previous frame levels for interpolation (tile coords -> prev level).
    pub prev_levels: Vec<(i32, i32, f32)>,
    /// Accumulator for fixed timestep.
    pub accumulator: f32,
}

impl Default for LiquidSimState {
    fn default() -> Self {
        Self {
            sleep: SleepTracker::default(),
            prev_levels: Vec::new(),
            accumulator: 0.0,
        }
    }
}

/// Fixed timestep for liquid simulation (~20 FPS).
const LIQUID_DT: f32 = 1.0 / 20.0;

/// The main liquid simulation system.
pub fn liquid_simulation_system(
    time: Res<Time>,
    config: Res<ActiveWorld>,
    liquid_registry: Res<LiquidRegistryRes>,
    mut world_map: ResMut<WorldMap>,
    mut sim_state: ResMut<LiquidSimState>,
    mut dirty_chunks: ResMut<DirtyChunks>,
) {
    if liquid_registry.defs.is_empty() {
        return;
    }

    sim_state.accumulator += time.delta_secs().min(0.1);

    while sim_state.accumulator >= LIQUID_DT {
        sim_state.accumulator -= LIQUID_DT;
        run_liquid_step(
            &config,
            &liquid_registry,
            &mut world_map,
            &mut sim_state,
            &mut dirty_chunks,
            LIQUID_DT,
        );
    }
}

fn run_liquid_step(
    config: &ActiveWorld,
    registry: &LiquidRegistryRes,
    world_map: &mut WorldMap,
    sim_state: &mut LiquidSimState,
    dirty_chunks: &mut DirtyChunks,
    dt: f32,
) {
    // Build density/viscosity arrays from registry.
    let densities: Vec<f32> = registry.defs.iter().map(|d| d.density).collect();
    let viscosities: Vec<f32> = registry.defs.iter().map(|d| d.viscosity).collect();

    // Collect active tiles to process.
    let active: Vec<(i32, i32)> = sim_state.sleep.active_tiles().collect();

    if active.is_empty() {
        return;
    }

    // For each active tile, compute flows and update.
    // We work directly on the WorldMap's chunk data.
    // Collect changes first, apply after.
    let mut changes: Vec<(i32, i32, LiquidCell)> = Vec::new();

    for &(tx, ty) in &active {
        let wtx = config.wrap_tile_x(tx);
        if ty < 0 || ty >= config.height_tiles {
            continue;
        }

        let cell = get_liquid(world_map, wtx, ty, config);
        if cell.is_empty() {
            sim_state.sleep.mark_stable(tx, ty);
            continue;
        }

        // Check if tile is solid (liquid shouldn't be in solid tiles).
        if is_tile_solid(world_map, wtx, ty, config) {
            changes.push((wtx, ty, LiquidCell::EMPTY));
            sim_state.sleep.mark_changed(tx, ty);
            continue;
        }

        let lid = (cell.liquid_type.0 - 1) as usize;
        let density = densities.get(lid).copied().unwrap_or(1.0);
        let visc = viscosities.get(lid).copied().unwrap_or(1.0);

        // Compute pressure (simplified: just level + depth estimation).
        let my_pressure = cell.level + compute_depth(world_map, wtx, ty, cell.liquid_type, config) * density * 0.1;

        let mut total_out: f32 = 0.0;
        let mut out_flows: [(i32, i32, f32); 4] = [(0, 0, 0.0); 4];
        let offsets = [(1, 0), (0, 1), (-1, 0), (0, -1)];
        let gravity_bias = [0.0, -1.0, 0.0, 2.0]; // right, up, left, down

        for (fi, &(dx, dy)) in offsets.iter().enumerate() {
            let nx = config.wrap_tile_x(tx + dx);
            let ny = ty + dy;
            if ny < 0 || ny >= config.height_tiles {
                continue;
            }
            if is_tile_solid(world_map, nx, ny, config) {
                continue;
            }

            let n_cell = get_liquid(world_map, nx, ny, config);

            // Don't flow into denser different liquid.
            if !n_cell.is_empty() && n_cell.liquid_type != cell.liquid_type {
                let n_lid = (n_cell.liquid_type.0 - 1) as usize;
                let n_density = densities.get(n_lid).copied().unwrap_or(1.0);
                if n_density > density {
                    continue;
                }
            }

            let n_pressure = if n_cell.is_empty() {
                0.0
            } else {
                let n_lid = (n_cell.liquid_type.0 - 1) as usize;
                let n_density = densities.get(n_lid).copied().unwrap_or(1.0);
                n_cell.level + compute_depth(world_map, nx, ny, n_cell.liquid_type, config) * n_density * 0.1
            };

            let flow = dt * (my_pressure - n_pressure + gravity_bias[fi]) / visc;
            if flow > 0.0 {
                let clamped = flow.min(MAX_FLOW);
                out_flows[fi] = (nx, ny, clamped);
                total_out += clamped;
            }
        }

        // Scale if exceeding available volume.
        if total_out > cell.level && total_out > 0.0 {
            let scale = cell.level / total_out;
            for of in &mut out_flows {
                of.2 *= scale;
            }
            total_out = cell.level;
        }

        if total_out < MIN_LEVEL {
            sim_state.sleep.mark_stable(tx, ty);
            continue;
        }

        // Apply: reduce source.
        let new_level = cell.level - total_out;
        if new_level < MIN_LEVEL {
            changes.push((wtx, ty, LiquidCell::EMPTY));
        } else {
            changes.push((wtx, ty, LiquidCell {
                liquid_type: cell.liquid_type,
                level: new_level,
            }));
        }
        sim_state.sleep.mark_changed(tx, ty);

        // Apply: increase destinations.
        for &(nx, ny, flow) in &out_flows {
            if flow < MIN_LEVEL {
                continue;
            }
            let existing = get_liquid(world_map, nx, ny, config);
            if existing.is_empty() || existing.liquid_type == cell.liquid_type {
                let new_dest_level = (existing.level + flow).min(MAX_LEVEL);
                changes.push((nx, ny, LiquidCell {
                    liquid_type: cell.liquid_type,
                    level: new_dest_level,
                }));
            }
            sim_state.sleep.mark_changed(nx, ny);
        }
    }

    // Apply all changes to world map.
    for (tx, ty, cell) in changes {
        set_liquid(world_map, tx, ty, cell, config);
        let (cx, cy) = chunk::tile_to_chunk(tx, ty, config.chunk_size);
        dirty_chunks.0.insert((cx, cy));
    }
}

/// Helper: get liquid from world map by tile coords.
fn get_liquid(world_map: &WorldMap, tx: i32, ty: i32, config: &ActiveWorld) -> LiquidCell {
    let (cx, cy) = chunk::tile_to_chunk(tx, ty, config.chunk_size);
    let (lx, ly) = chunk::tile_to_local(tx, ty, config.chunk_size);
    match world_map.chunk(cx, cy) {
        Some(chunk) => chunk.liquid.get(lx, ly, config.chunk_size),
        None => LiquidCell::EMPTY,
    }
}

/// Helper: set liquid in world map.
fn set_liquid(world_map: &mut WorldMap, tx: i32, ty: i32, cell: LiquidCell, config: &ActiveWorld) {
    let (cx, cy) = chunk::tile_to_chunk(tx, ty, config.chunk_size);
    let (lx, ly) = chunk::tile_to_local(tx, ty, config.chunk_size);
    if let Some(chunk) = world_map.chunk_mut(cx, cy) {
        chunk.liquid.set(lx, ly, cell, config.chunk_size);
    }
}

/// Helper: check if tile is solid.
fn is_tile_solid(world_map: &WorldMap, tx: i32, ty: i32, config: &ActiveWorld) -> bool {
    let (cx, cy) = chunk::tile_to_chunk(tx, ty, config.chunk_size);
    let (lx, ly) = chunk::tile_to_local(tx, ty, config.chunk_size);
    match world_map.chunk(cx, cy) {
        Some(chunk) => {
            let tile_id = chunk.fg.get(lx, ly, config.chunk_size);
            tile_id != crate::registry::tile::TileId::AIR
                // TODO: check tile_registry.is_solid() when available in this scope.
                // For now, any non-air FG tile is treated as solid.
        }
        None => true, // unloaded = wall
    }
}

/// Compute depth of liquid column above a tile (for pressure calculation).
fn compute_depth(world_map: &WorldMap, tx: i32, ty: i32, liquid_type: LiquidId, config: &ActiveWorld) -> f32 {
    let mut depth: f32 = 0.0;
    let mut y = ty + 1;
    while y < config.height_tiles {
        let cell = get_liquid(world_map, tx, y, config);
        if cell.liquid_type != liquid_type || cell.is_empty() {
            break;
        }
        depth += cell.level;
        y += 1;
    }
    depth
}
```

**Step 2: Register system and resources in LiquidPlugin**

Update `src/liquid/mod.rs`:
```rust
mod data;
mod registry;
mod simulation;
mod sleep;
mod system;

pub use data::*;
pub use registry::*;
pub use system::*;

use bevy::prelude::*;
use crate::sets::GameSet;

pub struct LiquidPlugin;

impl Plugin for LiquidPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LiquidRegistryRes>()
            .init_resource::<LiquidSimState>()
            .add_systems(
                Update,
                liquid_simulation_system.in_set(GameSet::WorldUpdate),
            );
    }
}
```

**Step 3: Build and verify**

Run: `cargo build 2>&1 | head -30`

**Step 4: Commit**

```
git add src/liquid/
git commit -m "feat(liquid): world simulation system with sleep optimization"
```

---

### Task 6: Liquid Generation in Terrain

**Files:**
- Modify: `src/world/terrain_gen.rs`
- Modify: `src/world/chunk.rs` (generate_chunk_tiles returns liquid data too)

**Step 1: Add liquid generation to terrain_gen**

In `terrain_gen.rs`, modify `generate_chunk_tiles` to also produce a `Vec<LiquidCell>`. For each tile:
- If tile is AIR and y is below sea level → water (level = 1.0)
- If tile is AIR and y is in deep cave biome → chance of lava
- Otherwise → LiquidCell::EMPTY

Add a `generate_liquid` function:
```rust
pub fn generate_liquid(tile_x: i32, tile_y: i32, fg_tile: TileId, ctx: &WorldCtxRef) -> LiquidCell {
    if fg_tile != TileId::AIR {
        return LiquidCell::EMPTY;
    }
    let surface = surface_height(/* ... */);
    let sea_level = (surface as f32 * 0.85) as i32; // slightly below average surface
    if tile_y <= sea_level {
        return LiquidCell {
            liquid_type: LiquidId(1), // water
            level: 1.0,
        };
    }
    LiquidCell::EMPTY
}
```

Update `ChunkTiles` to include `liquid: Vec<LiquidCell>`.

**Step 2: Update chunk generation to use liquid data**

In `chunk.rs`, where `generate_chunk_tiles` result is used to build `ChunkData`, include the liquid layer.

**Step 3: Wake newly generated liquid cells**

When a chunk with liquid is loaded, wake all non-empty liquid cells at chunk edges (so they interact with adjacent chunks).

**Step 4: Build and test in-game**

Run: `cargo run`
Expected: Water appears below sea level on generated planets.

**Step 5: Commit**

```
git add src/world/terrain_gen.rs src/world/chunk.rs
git commit -m "feat(liquid): generate water below sea level during terrain generation"
```

---

### Task 7: Liquid Rendering

**Files:**
- Create: `src/liquid/render.rs`
- Create: `assets/shaders/liquid.wgsl`
- Modify: `src/liquid/mod.rs`
- Modify: `src/world/mod.rs`

**Step 1: Create liquid material and mesh builder**

Create `src/liquid/render.rs` with:
- `LiquidMaterial` — custom `Material2d` with liquid color, time, neighbor levels for surface interpolation
- `build_liquid_mesh()` — iterates chunk's LiquidLayer, emits a quad per non-empty cell. Quad height = `level * tile_size`. Bottom-aligned within tile.
- `liquid_mesh_rebuild_system` — rebuilds meshes for dirty chunks

Pattern follows `mesh_builder.rs` and `tile_renderer.rs`:
- Each chunk gets a `LiquidMeshEntity` component
- Spawned alongside fg/bg entities during chunk loading
- Mesh rebuilt when `ChunkDirty` is present

**Step 2: Create liquid shader**

Create `assets/shaders/liquid.wgsl`:
- Fragment shader: flat color with alpha from `LiquidDef.color`
- Surface smoothing: vertex y-position interpolated between left/right neighbor levels
- Surface wave: `sin(vertex_x * 8.0 + time * 2.0) * 0.5` pixels

**Step 3: Spawn liquid mesh entities during chunk loading**

In `chunk_loading_system`, when spawning chunk entities, also spawn a liquid mesh entity at the same position with z-index between bg and fg (or slightly above fg, depending on visual preference).

**Step 4: Build and test visually**

Run: `cargo run`
Expected: Blue semi-transparent water visible below sea level.

**Step 5: Commit**

```
git add src/liquid/render.rs assets/shaders/liquid.wgsl src/liquid/mod.rs src/world/mod.rs
git commit -m "feat(liquid): rendering with surface smoothing and chunk meshes"
```

---

### Task 8: Physics Integration (Swimming & Buoyancy)

**Files:**
- Modify: `src/physics.rs`

**Step 1: Add liquid overlap detection**

After tile collision in `tile_collision` system, check if entity AABB overlaps any liquid cells. If so:

```rust
// In tile_collision system, after resolving solid collisions:
let submerged = calculate_submersion(&world_map, &ctx_ref, entity_aabb);
if submerged.depth > 0.0 {
    // Buoyancy: counteract gravity proportional to submersion.
    velocity.y += submerged.density * GRAVITY_CONSTANT * submerged.depth * dt;
    // Drag: reduce velocity.
    velocity.x *= 1.0 - (submerged.viscosity_factor * dt);
    velocity.y *= 1.0 - (submerged.viscosity_factor * dt * 0.5);
}
```

**Step 2: Build and test**

Run: `cargo run`
Expected: Player slows down and floats in water.

**Step 3: Commit**

```
git add src/physics.rs
git commit -m "feat(liquid): swimming, buoyancy, and drag in physics system"
```

---

### Task 9: Block Interaction Integration

**Files:**
- Modify: `src/interaction/block_action.rs`
- Modify: `src/liquid/system.rs`

**Step 1: Wake liquid when tiles are broken**

In `block_action.rs`, after `set_tile(..., TileId::AIR)` (breaking a block), wake neighboring liquid cells:

```rust
// After breaking a tile:
if let Some(mut sim_state) = sim_state_opt {
    sim_state.sleep.wake_with_neighbors(tile_x, tile_y);
}
```

Add `Option<ResMut<LiquidSimState>>` to the system parameters.

**Step 2: Displace liquid when tiles are placed**

When placing a solid tile into a cell with liquid, remove the liquid and try to push it to neighbors:

```rust
// Before placing tile, check for liquid:
let liquid = world_map.get_liquid(tile_x, tile_y, &ctx_ref);
if !liquid.is_empty() {
    world_map.set_liquid(tile_x, tile_y, LiquidCell::EMPTY, &ctx_ref);
    // Wake neighbors so displaced liquid redistributes.
    sim_state.sleep.wake_with_neighbors(tile_x, tile_y);
}
```

**Step 3: Build and test**

Run: `cargo run`
Expected: Breaking a wall next to water causes water to flow in. Placing a block in water removes liquid.

**Step 4: Commit**

```
git add src/interaction/block_action.rs src/liquid/system.rs
git commit -m "feat(liquid): wake liquid on tile break, displace on tile place"
```

---

### Task 10: RC Lighting Integration

**Files:**
- Modify: `src/world/rc_lighting.rs`

**Step 1: Add liquid light emission and opacity**

In the RC lighting grid construction (where it reads tile light data), also read liquid data:

```rust
// When building the lighting input grid, for each tile:
let liquid = world_map.get_liquid(tile_x, tile_y, &ctx_ref);
if !liquid.is_empty() {
    if let Some(def) = liquid_registry.get(liquid.liquid_type) {
        // Add light emission (e.g., lava glows).
        emission[0] += def.light_emission[0] as f32 * liquid.level;
        emission[1] += def.light_emission[1] as f32 * liquid.level;
        emission[2] += def.light_emission[2] as f32 * liquid.level;
        // Add light opacity.
        opacity = opacity.max(def.light_opacity as f32 * liquid.level / 255.0);
    }
}
```

**Step 2: Mark RC dirty when liquid changes**

In `liquid_simulation_system`, after applying changes, set `rc_dirty.0 = true` if any changes occurred. Add `ResMut<RcGridDirty>` to system params.

**Step 3: Build and test**

Run: `cargo run`
Expected: Lava glows, water slightly dims light passing through it.

**Step 4: Commit**

```
git add src/world/rc_lighting.rs src/liquid/system.rs
git commit -m "feat(liquid): integrate liquid light emission and opacity with RC lighting"
```

---

### Task 11: Liquid Reactions

**Files:**
- Modify: `src/liquid/system.rs`

**Step 1: Add reaction checking in simulation step**

In `run_liquid_step`, when a flow would move liquid A into a cell with liquid B, check reactions:

```rust
// When liquid A flows into cell with liquid B:
if let Some(reaction) = registry.get_reaction(src_type, existing.liquid_type) {
    if let Some(ref tile_name) = reaction.produce_tile {
        // Replace cell with solid tile (e.g., obsidian).
        // Set tile in world_map, remove liquid from both cells.
        // Wake neighbors.
    }
    if reaction.consume_both {
        // Remove liquid from source cell too.
    }
    continue; // Don't apply normal flow.
}
```

This requires access to `TileRegistry` to resolve tile names. Add it to system params.

**Step 2: Build and test**

Run: `cargo run`
Expected: Pouring water onto lava creates obsidian blocks.

**Step 3: Commit**

```
git add src/liquid/system.rs
git commit -m "feat(liquid): liquid reactions (water+lava=obsidian)"
```

---

### Task 12: Persistence

**Files:**
- Modify: `src/cosmos/persistence.rs`

**Step 1: Ensure LiquidLayer is serialized with chunks**

`LiquidLayer` already derives `Serialize, Deserialize`. Verify that `ChunkData` serialization (in `persistence.rs`) includes the new `liquid` field. Since `ChunkData` derives `Serialize, Deserialize` and we added `liquid: LiquidLayer`, this should work automatically.

Test by: save world, reload, verify liquid is still there.

**Step 2: Commit**

```
git add src/cosmos/persistence.rs
git commit -m "feat(liquid): verify liquid persistence with chunk save/load"
```

---

### Task 13: Registry Loading

**Files:**
- Modify: `src/registry/loader.rs` or `src/registry/loading.rs` (wherever RON assets are loaded)
- Modify: `src/liquid/mod.rs`

**Step 1: Load liquids.registry.ron during AppState::Loading**

Follow the pattern of `tiles.registry.ron` loading. During `AppState::Loading`, read the RON file, deserialize `Vec<LiquidDef>`, build `LiquidRegistryRes::from_defs()`, insert as resource.

**Step 2: Build and verify**

Run: `cargo run`
Expected: Game loads without errors, liquid registry populated with water/lava/oil.

**Step 3: Commit**

```
git add src/registry/ src/liquid/mod.rs
git commit -m "feat(liquid): load liquid definitions from RON registry"
```

---

## Execution Order

Tasks have these dependencies:

```
Task 1 (data model) → Task 2 (chunk layer) → Task 3 (simulation) → Task 4 (sleep)
                                                                        ↓
Task 13 (registry loading) ←──────────── Task 5 (world system) ← Task 4
                                              ↓
                              Task 6 (terrain gen) → Task 7 (rendering)
                                              ↓
                              Task 8 (physics) ─── independent after Task 5
                              Task 9 (block interaction) ─── independent after Task 5
                              Task 10 (lighting) ─── independent after Task 5
                              Task 11 (reactions) ─── independent after Task 5
                              Task 12 (persistence) ─── independent after Task 2
```

**Recommended order:** 1 → 2 → 3 → 4 → 5 → 13 → 6 → 7 → 8 → 9 → 10 → 11 → 12
