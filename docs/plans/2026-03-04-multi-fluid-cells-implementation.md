# Multi-Fluid Cells Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Allow each fluid cell to hold two immiscible liquids (heavy at bottom, light on top) to eliminate visual gaps at fluid boundaries.

**Architecture:** Replace single `{fluid_id, mass}` cell with dual-slot `{primary: FluidSlot, secondary: FluidSlot}` where primary is always denser. Flow, rendering, reactions, and displacement all operate per-slot. Sum of masses per cell <= 1.0.

**Tech Stack:** Rust, Bevy ECS, custom WGSL shader, RON data files

---

### Task 1: Rewrite FluidCell data structure

**Files:**
- Modify: `src/fluid/cell.rs`

**Step 1: Replace FluidCell with dual-slot structure**

Replace the entire `FluidCell` struct and add `FluidSlot`:

```rust
use serde::{Deserialize, Serialize};

/// Compact fluid type identifier. Index into FluidRegistry.defs.
/// 0 = no fluid (empty cell).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct FluidId(pub u8);

impl FluidId {
    pub const NONE: FluidId = FluidId(0);
}

/// One layer of fluid within a cell.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct FluidSlot {
    pub fluid_id: FluidId,
    pub mass: f32,
}

impl FluidSlot {
    pub const EMPTY: FluidSlot = FluidSlot {
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

/// A single cell of fluid data. Holds up to two immiscible liquids.
///
/// **Invariants:**
/// - `primary` is always the denser fluid (renders at bottom).
/// - `secondary` sits on top (lighter fluid).
/// - `primary.mass + secondary.mass <= 1.0` (plus compression).
/// - If only one fluid is present, it lives in `primary`; `secondary` is empty.
/// - Gases do not participate in dual-slot mixing (single-slot only).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct FluidCell {
    pub primary: FluidSlot,
    pub secondary: FluidSlot,
}

impl FluidCell {
    pub const EMPTY: FluidCell = FluidCell {
        primary: FluidSlot::EMPTY,
        secondary: FluidSlot::EMPTY,
    };

    /// Create a single-fluid cell (secondary is empty).
    pub fn new(fluid_id: FluidId, mass: f32) -> Self {
        Self {
            primary: FluidSlot::new(fluid_id, mass),
            secondary: FluidSlot::EMPTY,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.primary.is_empty() && self.secondary.is_empty()
    }

    /// Total mass of both slots.
    pub fn total_mass(&self) -> f32 {
        self.primary.mass + self.secondary.mass
    }

    /// Convenience: the primary fluid_id (for backward compat in checks).
    pub fn fluid_id(&self) -> FluidId {
        self.primary.fluid_id
    }

    /// Convenience: the primary mass (for backward compat).
    pub fn mass(&self) -> f32 {
        self.primary.mass
    }

    /// Check if this cell contains a specific fluid in either slot.
    pub fn has_fluid(&self, fid: FluidId) -> bool {
        self.primary.fluid_id == fid || self.secondary.fluid_id == fid
    }

    /// Get the slot (primary or secondary) that contains the given fluid_id.
    /// Returns None if neither slot has it.
    pub fn slot_for(&self, fid: FluidId) -> Option<&FluidSlot> {
        if self.primary.fluid_id == fid {
            Some(&self.primary)
        } else if self.secondary.fluid_id == fid {
            Some(&self.secondary)
        } else {
            None
        }
    }

    /// Get mutable slot for a fluid_id.
    pub fn slot_for_mut(&mut self, fid: FluidId) -> Option<&mut FluidSlot> {
        if self.primary.fluid_id == fid {
            Some(&mut self.primary)
        } else if self.secondary.fluid_id == fid {
            Some(&mut self.secondary)
        } else {
            None
        }
    }

    /// Clean up: remove empty slots, ensure primary is always the occupied one.
    /// Call after modifying masses.
    pub fn normalize(&mut self) {
        // Clear slots with negligible mass
        if self.primary.mass <= 0.0 {
            self.primary = FluidSlot::EMPTY;
        }
        if self.secondary.mass <= 0.0 {
            self.secondary = FluidSlot::EMPTY;
        }
        // If primary is empty but secondary isn't, move secondary to primary
        if self.primary.is_empty() && !self.secondary.is_empty() {
            self.primary = self.secondary;
            self.secondary = FluidSlot::EMPTY;
        }
    }

    /// Ensure the density invariant: primary must be denser than secondary.
    /// Takes a density lookup closure. Swaps slots if needed.
    pub fn enforce_density_order(&mut self, density_of: impl Fn(FluidId) -> f32) {
        if self.secondary.is_empty() {
            return;
        }
        let d_pri = density_of(self.primary.fluid_id);
        let d_sec = density_of(self.secondary.fluid_id);
        if d_sec > d_pri {
            std::mem::swap(&mut self.primary, &mut self.secondary);
        }
    }
}
```

**Step 2: Update tests**

Replace existing tests in `cell.rs` to test the new structure:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_cell_is_empty() {
        let cell = FluidCell::EMPTY;
        assert!(cell.is_empty());
        assert!(cell.primary.is_empty());
        assert!(cell.secondary.is_empty());
    }

    #[test]
    fn single_fluid_cell() {
        let cell = FluidCell::new(FluidId(1), 0.5);
        assert!(!cell.is_empty());
        assert_eq!(cell.fluid_id(), FluidId(1));
        assert!((cell.mass() - 0.5).abs() < f32::EPSILON);
        assert!(cell.secondary.is_empty());
    }

    #[test]
    fn total_mass_both_slots() {
        let mut cell = FluidCell::new(FluidId(1), 0.4);
        cell.secondary = FluidSlot::new(FluidId(2), 0.3);
        assert!((cell.total_mass() - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn has_fluid_checks_both_slots() {
        let mut cell = FluidCell::new(FluidId(1), 0.4);
        cell.secondary = FluidSlot::new(FluidId(2), 0.3);
        assert!(cell.has_fluid(FluidId(1)));
        assert!(cell.has_fluid(FluidId(2)));
        assert!(!cell.has_fluid(FluidId(3)));
    }

    #[test]
    fn normalize_moves_secondary_to_primary() {
        let mut cell = FluidCell::EMPTY;
        cell.secondary = FluidSlot::new(FluidId(1), 0.5);
        cell.normalize();
        assert_eq!(cell.primary.fluid_id, FluidId(1));
        assert!(cell.secondary.is_empty());
    }

    #[test]
    fn enforce_density_swaps_when_needed() {
        let mut cell = FluidCell::new(FluidId(1), 0.4); // light, density 100
        cell.secondary = FluidSlot::new(FluidId(2), 0.3); // heavy, density 3000
        cell.enforce_density_order(|fid| if fid == FluidId(2) { 3000.0 } else { 100.0 });
        assert_eq!(cell.primary.fluid_id, FluidId(2)); // heavy is now primary
        assert_eq!(cell.secondary.fluid_id, FluidId(1));
    }

    #[test]
    fn fluid_cell_serialization_roundtrip() {
        let mut cell = FluidCell::new(FluidId(2), 0.6);
        cell.secondary = FluidSlot::new(FluidId(1), 0.3);
        let serialized = ron::to_string(&cell).unwrap();
        let deserialized: FluidCell = ron::from_str(&serialized).unwrap();
        assert_eq!(deserialized.primary.fluid_id, FluidId(2));
        assert!((deserialized.primary.mass - 0.6).abs() < f32::EPSILON);
        assert_eq!(deserialized.secondary.fluid_id, FluidId(1));
        assert!((deserialized.secondary.mass - 0.3).abs() < f32::EPSILON);
    }
}
```

**Step 3: Run tests**

Run: `cargo test fluid::cell`
Expected: All tests pass.

**Step 4: Commit**

```bash
git add src/fluid/cell.rs
git commit -m "refactor(fluid): rewrite FluidCell with dual-slot primary/secondary"
```

---

### Task 2: Update FluidWorld read/write operations

**Files:**
- Modify: `src/fluid/fluid_world.rs`

The key change: `FluidWorld` methods that read/write `FluidCell` now work with the dual-slot structure. Most methods operate on the whole cell, but `add_mass` and `sub_mass` need to target specific slots.

**Step 1: Update `add_mass` to target correct slot**

Replace the current `add_mass`:

```rust
/// Add mass to a cell for a specific fluid_id.
///
/// Routing logic:
/// 1. If primary matches fluid_id → add to primary
/// 2. If secondary matches fluid_id → add to secondary
/// 3. If cell is empty → set primary
/// 4. If primary is occupied by different fluid, secondary is empty → set secondary
/// 5. Otherwise (both slots occupied by different fluids) → no-op, return false
///
/// Caps total mass at `cap` if > 0.0 (pass 0.0 to skip cap).
/// Returns actual amount added.
pub fn add_mass(&mut self, gx: i32, gy: i32, fluid_id: FluidId, amount: f32) -> f32 {
    let Some((cx, cy, lx, ly)) = self.resolve(gx, gy) else {
        return 0.0;
    };
    if let Some(chunk) = self.world_map.chunks.get_mut(&(cx, cy)) {
        let idx = (ly * self.chunk_size + lx) as usize;
        let cell = &mut chunk.fluids[idx];

        if cell.primary.fluid_id == fluid_id {
            cell.primary.mass += amount;
            return amount;
        }
        if cell.secondary.fluid_id == fluid_id {
            cell.secondary.mass += amount;
            return amount;
        }
        if cell.is_empty() {
            cell.primary = FluidSlot::new(fluid_id, amount);
            return amount;
        }
        if cell.secondary.is_empty() {
            cell.secondary = FluidSlot::new(fluid_id, amount);
            return amount;
        }
        // Both slots occupied by different fluids — can't add
        0.0
    } else {
        0.0
    }
}
```

**Step 2: Update `sub_mass` to target specific fluid**

The current `sub_mass` doesn't know which fluid to subtract from. We need a version that specifies the fluid_id:

```rust
/// Subtract mass from the slot matching `fluid_id`.
/// If mass drops to zero, clears the slot and normalizes the cell.
pub fn sub_mass(&mut self, gx: i32, gy: i32, fluid_id: FluidId, amount: f32) {
    let Some((cx, cy, lx, ly)) = self.resolve(gx, gy) else {
        return;
    };
    if let Some(chunk) = self.world_map.chunks.get_mut(&(cx, cy)) {
        let idx = (ly * self.chunk_size + lx) as usize;
        let cell = &mut chunk.fluids[idx];

        let slot = if cell.primary.fluid_id == fluid_id {
            &mut cell.primary
        } else if cell.secondary.fluid_id == fluid_id {
            &mut cell.secondary
        } else {
            return;
        };

        slot.mass -= amount;
        if slot.mass <= 0.0 {
            *slot = FluidSlot::EMPTY;
        }
        cell.normalize();
    }
}
```

**Step 3: Update `read`, `read_current`**

These return `FluidCell` — no signature change needed since `FluidCell` is still the return type. But usage sites that access `.fluid_id` and `.mass` directly will need to use `.fluid_id()` and `.mass()` (the convenience methods) or access `.primary` explicitly.

**Step 4: Update `swap_fluids`**

No change needed — it swaps entire cells.

**Step 5: Update tests in fluid_world.rs**

Update the test assertions to use the new API. For example:
- `cell.fluid_id` → `cell.fluid_id()` or `cell.primary.fluid_id`
- `cell.mass` → `cell.mass()` or `cell.primary.mass`

**Step 6: Run tests**

Run: `cargo test fluid::fluid_world`
Expected: All tests pass.

**Step 7: Commit**

```bash
git add src/fluid/fluid_world.rs
git commit -m "refactor(fluid): update FluidWorld for dual-slot FluidCell"
```

---

### Task 3: Update simulation flow logic

**Files:**
- Modify: `src/fluid/simulation.rs`

**Step 1: Update `simulate_tick` to iterate both slots**

The current loop reads `cell.fluid_id` and `cell.mass` for a single fluid. Now each cell can have two fluids. We need to process each slot independently.

Replace the inner loop body in `simulate_tick`:

```rust
for lx in x_iter {
    let gx = base_gx + lx;
    let cell = world.read(gx, gy);
    if cell.is_empty() {
        continue;
    }

    // Process primary slot
    if !cell.primary.is_empty() {
        let def = world.fluid_registry.get(cell.primary.fluid_id);
        let max_speed = config.max_speed * (1.0 - def.viscosity).max(0.1);
        let current = world.read_current(gx, gy);
        let remaining = current.primary.mass;

        if def.is_gas {
            let remaining = flow_vertical(world, gx, gy, 1, true, remaining, cell.primary.fluid_id, def.max_compress, max_speed, config.min_flow);
            let remaining = flow_horizontal(world, gx, gy, remaining, cell.primary.fluid_id, cell.primary.mass, max_speed, config.min_flow);
            flow_vertical(world, gx, gy, -1, false, remaining, cell.primary.fluid_id, def.max_compress, max_speed, config.min_flow);
        } else {
            let remaining = flow_vertical(world, gx, gy, -1, true, remaining, cell.primary.fluid_id, def.max_compress, max_speed, config.min_flow);
            let remaining = flow_horizontal(world, gx, gy, remaining, cell.primary.fluid_id, cell.primary.mass, max_speed, config.min_flow);
            flow_vertical(world, gx, gy, 1, false, remaining, cell.primary.fluid_id, def.max_compress, max_speed, config.min_flow);
        }
    }

    // Process secondary slot
    if !cell.secondary.is_empty() {
        let def = world.fluid_registry.get(cell.secondary.fluid_id);
        let max_speed = config.max_speed * (1.0 - def.viscosity).max(0.1);
        let current = world.read_current(gx, gy);
        let remaining = current.secondary.mass;

        if def.is_gas {
            let remaining = flow_vertical(world, gx, gy, 1, true, remaining, cell.secondary.fluid_id, def.max_compress, max_speed, config.min_flow);
            let remaining = flow_horizontal(world, gx, gy, remaining, cell.secondary.fluid_id, cell.secondary.mass, max_speed, config.min_flow);
            flow_vertical(world, gx, gy, -1, false, remaining, cell.secondary.fluid_id, def.max_compress, max_speed, config.min_flow);
        } else {
            let remaining = flow_vertical(world, gx, gy, -1, true, remaining, cell.secondary.fluid_id, def.max_compress, max_speed, config.min_flow);
            let remaining = flow_horizontal(world, gx, gy, remaining, cell.secondary.fluid_id, cell.secondary.mass, max_speed, config.min_flow);
            flow_vertical(world, gx, gy, 1, false, remaining, cell.secondary.fluid_id, def.max_compress, max_speed, config.min_flow);
        }
    }
}
```

**Step 2: Update `flow_vertical` to use dual-slot aware reads**

The key change is in how we check the neighbor and decide flow amount. The neighbor check needs to consider both slots:

```rust
fn flow_vertical(
    world: &mut FluidWorld,
    gx: i32, gy: i32, dy: i32,
    is_primary: bool, remaining: f32,
    fluid_id: FluidId, max_compress: f32,
    max_speed: f32, min_flow: f32,
) -> f32 {
    if remaining <= 0.0 { return 0.0; }

    let ny = gy + dy;
    if world.is_solid(gx, ny) { return remaining; }

    // Check snapshot: can this fluid enter the neighbor?
    let neighbor = world.read(gx, ny);
    if !can_accept_fluid(&neighbor, fluid_id) {
        return remaining;
    }

    // Live-state check
    let current_neighbor = world.read_current(gx, ny);
    if !can_accept_fluid(&current_neighbor, fluid_id) {
        return remaining;
    }

    // Get the mass of our fluid_id in the neighbor cell
    let neighbor_mass = current_neighbor
        .slot_for(fluid_id)
        .map(|s| s.mass)
        .unwrap_or(0.0);

    let total = remaining + neighbor_mass;

    let flow = if is_primary {
        get_stable_state(total, max_compress) - neighbor_mass
    } else {
        if remaining <= MAX_MASS { return remaining; }
        remaining - get_stable_state(total, max_compress)
    };

    if flow <= 0.0 { return remaining; }

    let mut flow = flow;
    if flow > min_flow { flow *= 0.5; }

    // Cap flow so neighbor total_mass doesn't exceed 1.0
    let neighbor_total = current_neighbor.total_mass();
    let available = (1.0 + max_compress - neighbor_total).max(0.0);
    flow = flow.min(max_speed).min(remaining).min(available).max(0.0);

    if flow <= 0.0 { return remaining; }

    world.sub_mass(gx, gy, fluid_id, flow);
    world.add_mass(gx, ny, fluid_id, flow);
    remaining - flow
}
```

Add helper function:

```rust
/// Check if a cell can accept more of the given fluid_id.
/// Returns true if: cell is empty, cell has matching slot, or cell has an empty slot.
fn can_accept_fluid(cell: &FluidCell, fluid_id: FluidId) -> bool {
    if cell.is_empty() { return true; }
    if cell.has_fluid(fluid_id) { return true; }
    if cell.secondary.is_empty() { return true; }
    false
}
```

**Step 3: Update `flow_side` similarly**

Same pattern as `flow_vertical` but for horizontal flow.

**Step 4: Update cleanup loop**

```rust
// Cleanup: remove cells with negligible mass
for &(cx, cy) in active_chunks {
    if let Some(chunk) = world.world_map.chunks.get_mut(&(cx, cy)) {
        for cell in chunk.fluids.iter_mut() {
            if cell.primary.mass > 0.0 && cell.primary.mass < config.min_mass {
                cell.primary = FluidSlot::EMPTY;
            }
            if cell.secondary.mass > 0.0 && cell.secondary.mass < config.min_mass {
                cell.secondary = FluidSlot::EMPTY;
            }
            cell.normalize();
        }
    }
}
```

**Step 5: Update simulation tests**

All tests that use `cell.fluid_id` or `cell.mass` need updating to use `cell.fluid_id()` / `cell.mass()` or `cell.primary.fluid_id` / `cell.primary.mass`.

**Step 6: Run tests, fix compilation errors**

Run: `cargo test fluid::simulation`
Expected: Tests pass.

**Step 7: Commit**

```bash
git add src/fluid/simulation.rs
git commit -m "refactor(fluid): update simulation flow for dual-slot cells"
```

---

### Task 4: Update density displacement and reactions

**Files:**
- Modify: `src/fluid/reactions.rs`

**Step 1: Add intra-cell density enforcement**

Add a new phase at the start of `resolve_density_displacement_global`:

```rust
// Phase 0: Intra-cell — enforce density order within each cell.
for &(cx, cy) in active_chunks {
    if let Some(chunk) = world.world_map.chunks.get_mut(&(cx, cy)) {
        let registry = &world.fluid_registry;
        for cell in chunk.fluids.iter_mut() {
            cell.enforce_density_order(|fid| registry.get(fid).density);
        }
    }
}
```

**Step 2: Update vertical displacement**

The current code swaps entire cells. With dual-slot, we need smarter logic:
- If above cell's primary is heavier than below cell's secondary → partial swap
- Simplest approach: keep swapping entire cells (the intra-cell normalization in Phase 0 will sort slots within cells after the swap)

No change needed to the swap logic itself — `swap_fluids` already swaps entire `FluidCell`s, and Phase 0 handles intra-cell ordering.

**Step 3: Update horizontal displacement**

Same reasoning — the existing swap logic works because Phase 0 handles intra-cell ordering.

**Step 4: Add intra-cell reactions**

In `execute_fluid_reactions_global`, add a check at the start of each cell:

```rust
// Check intra-cell reaction (primary vs secondary in same cell)
let cell = world.read_current(gx, gy);
if !cell.primary.is_empty() && !cell.secondary.is_empty()
    && cell.primary.fluid_id != cell.secondary.fluid_id
{
    if let Some(reaction) = reaction_registry.find_reaction(
        cell.primary.fluid_id,
        cell.secondary.fluid_id,
        &Adjacency::Any,
    ) {
        // ... execute reaction between primary and secondary
    }
}
```

**Step 5: Update inter-cell reactions**

The neighbor checks need to consider all slot combinations. For each neighbor, check:
- `cell.primary` vs `neighbor.primary`
- `cell.primary` vs `neighbor.secondary`
- `cell.secondary` vs `neighbor.primary`
- `cell.secondary` vs `neighbor.secondary`

**Step 6: Update reaction tests**

Fix all `cell.fluid_id` → `cell.fluid_id()` etc.

**Step 7: Run tests**

Run: `cargo test fluid::reactions`
Expected: Tests pass.

**Step 8: Commit**

```bash
git add src/fluid/reactions.rs
git commit -m "feat(fluid): intra-cell reactions and dual-slot displacement"
```

---

### Task 5: Update rendering for dual-slot cells

**Files:**
- Modify: `src/fluid/render.rs`

This is the largest change. Each cell now potentially generates **2 quads** instead of 1.

**Step 1: Update `build_fluid_mesh` inner loop**

For each cell, emit quads for both primary and secondary slots:

```rust
for local_y in 0..chunk_size {
    for local_x in 0..chunk_size {
        let idx = (local_y * chunk_size + local_x) as usize;
        let cell = &fluids[idx];
        if cell.is_empty() { continue; }

        let world_x = (base_x + local_x as i32) as f32 * tile_size;
        let world_y = (base_y + local_y as i32) as f32 * tile_size;

        // Primary quad (bottom)
        if !cell.primary.is_empty() {
            let def = fluid_registry.get(cell.primary.fluid_id);
            let fill = cell.primary.mass.min(1.0);
            let (y0, y1) = if def.is_gas {
                // Gas: top-down (but gases are single-slot, so this is the only quad)
                (world_y + (1.0 - fill) * tile_size, world_y + tile_size)
            } else {
                // Liquid: bottom-up
                (world_y, world_y + fill * tile_size)
            };
            // ... emit quad with y0, y1, color from def, etc.
        }

        // Secondary quad (top) — only for liquids
        if !cell.secondary.is_empty() {
            let def = fluid_registry.get(cell.secondary.fluid_id);
            let primary_fill = cell.primary.mass.min(1.0);
            let secondary_fill = cell.secondary.mass.min(1.0 - primary_fill);
            let y0 = world_y + primary_fill * tile_size;
            let y1 = y0 + secondary_fill * tile_size;
            // ... emit quad with y0, y1, color from secondary def
            // Surface detection: secondary is the surface if it's the topmost slot
        }
    }
}
```

**Step 2: Update surface detection**

`is_liquid_surface` needs to check: is this the topmost fluid in the cell AND is the cell above empty/gas?

For a cell with secondary: secondary is the surface candidate.
For a cell with only primary: primary is the surface candidate.

**Step 3: Update depth computation**

`compute_depth` scans upward counting fluid cells. With dual-slot, a cell is "non-empty" if either slot is non-empty. The depth should consider the topmost slot's fluid_id for continuity.

**Step 4: Update emission coverage**

`emission_covered` logic: if a different fluid is above, suppress emission. With dual-slot, the secondary slot in the same cell is "above" the primary, so primary's emission should be suppressed when secondary is a different fluid.

**Step 5: Update `column_liquid_surface_h` and `column_gas_surface_h`**

These find the surface height for cross-chunk smoothing. With dual-slot, the surface is at `primary.mass + secondary.mass` if both are liquids.

**Step 6: Update `compute_column_surface_data`**

Surface data needs to account for the total fill of both slots.

**Step 7: Run the game visually**

Run: `cargo run`
Expected: Fluids render correctly with no gaps between different fluid types.

**Step 8: Commit**

```bash
git add src/fluid/render.rs
git commit -m "feat(fluid): render dual-slot cells as two stacked quads"
```

---

### Task 6: Update wave simulation

**Files:**
- Modify: `src/fluid/wave.rs`

**Step 1: Update `WaveBuffer::step`**

The wave check `fluids[i].is_empty()` already works with the new `FluidCell::is_empty()` (checks both slots). No functional change needed.

The neighbor check `fluids[ni].is_empty()` also works correctly.

**Verify:** Read through `wave.rs` and confirm all `.is_empty()` calls compile and work correctly.

**Step 2: Run wave tests**

Run: `cargo test fluid::wave`
Expected: All tests pass.

**Step 3: Commit (only if changes needed)**

```bash
git add src/fluid/wave.rs
git commit -m "refactor(fluid): verify wave simulation works with dual-slot cells"
```

---

### Task 7: Update debug overlay

**Files:**
- Modify: `src/fluid/debug_overlay.rs`

**Step 1: Update cell display for dual-slot**

In `draw_fluid_debug_panel`, the "Cell at Cursor" section currently shows single fluid info. Update to show both slots:

```rust
if let Some(cell) = cell_info {
    if cell.is_empty() {
        ui.label("Fluid:");
        ui.colored_label(egui::Color32::GRAY, "(empty)");
        ui.end_row();
    } else {
        // Primary slot
        if !cell.primary.is_empty() {
            let def = fluid_registry.get(cell.primary.fluid_id);
            ui.label("Primary:");
            ui.colored_label(
                egui::Color32::from_rgb(def.color[0], def.color[1], def.color[2]),
                &def.id,
            );
            ui.end_row();

            ui.label("Mass:");
            ui.colored_label(mass_color(cell.primary.mass), format!("{:.4}", cell.primary.mass));
            ui.end_row();

            ui.label("Fill:");
            let fill = cell.primary.mass.min(1.0);
            ui.add(egui::ProgressBar::new(fill).text(format!("{:.0}%", fill * 100.0)));
            ui.end_row();
        }

        // Secondary slot
        if !cell.secondary.is_empty() {
            ui.separator();
            let def = fluid_registry.get(cell.secondary.fluid_id);
            ui.label("Secondary:");
            ui.colored_label(
                egui::Color32::from_rgb(def.color[0], def.color[1], def.color[2]),
                &def.id,
            );
            ui.end_row();

            ui.label("Mass:");
            ui.colored_label(mass_color(cell.secondary.mass), format!("{:.4}", cell.secondary.mass));
            ui.end_row();

            ui.label("Fill:");
            let fill = cell.secondary.mass.min(1.0);
            ui.add(egui::ProgressBar::new(fill).text(format!("{:.0}%", fill * 100.0)));
            ui.end_row();
        }

        // Shared info
        ui.label("Total mass:");
        ui.monospace(format!("{:.4}", cell.total_mass()));
        ui.end_row();
    }
}
```

**Step 2: Update mass stats**

The "Mass Stats" section iterates `cell.mass`. Update to use `cell.total_mass()` and count both slots:

```rust
for cell in &chunk.fluids {
    if !cell.primary.is_empty() {
        total_mass += cell.primary.mass;
        total_cells += 1;
        let entry = by_type.entry(cell.primary.fluid_id).or_insert((0.0, 0));
        entry.0 += cell.primary.mass;
        entry.1 += 1;
    }
    if !cell.secondary.is_empty() {
        total_mass += cell.secondary.mass;
        // Don't count as separate cell, but add to type stats
        let entry = by_type.entry(cell.secondary.fluid_id).or_insert((0.0, 0));
        entry.0 += cell.secondary.mass;
        entry.1 += 1;
    }
}
```

**Step 3: Update neighbours display**

Update neighbor display to show both slots:
```rust
Some(nc) => {
    let mut parts = Vec::new();
    if !nc.primary.is_empty() {
        let nd = fluid_registry.get(nc.primary.fluid_id);
        parts.push(format!("{} m={:.3}", nd.id, nc.primary.mass));
    }
    if !nc.secondary.is_empty() {
        let nd = fluid_registry.get(nc.secondary.fluid_id);
        parts.push(format!("{} m={:.3}", nd.id, nc.secondary.mass));
    }
    format!("{name}: {}", parts.join(" + "))
}
```

**Step 4: Commit**

```bash
git add src/fluid/debug_overlay.rs
git commit -m "feat(fluid): update F8 debug overlay for dual-slot cells"
```

---

### Task 8: Update systems.rs and fix remaining compilation

**Files:**
- Modify: `src/fluid/systems.rs`

**Step 1: Update movement detection**

In `run_one_tick`, the movement detection compares old vs new cells:

```rust
initial.iter().zip(chunk.fluids.iter()).any(|(old, new)| {
    old.fluid_id != new.fluid_id || (old.mass - new.mass).abs() >= CALM_MASS_EPSILON
})
```

Update to compare both slots:

```rust
initial.iter().zip(chunk.fluids.iter()).any(|(old, new)| {
    old.primary.fluid_id != new.primary.fluid_id
    || (old.primary.mass - new.primary.mass).abs() >= CALM_MASS_EPSILON
    || old.secondary.fluid_id != new.secondary.fluid_id
    || (old.secondary.mass - new.secondary.mass).abs() >= CALM_MASS_EPSILON
})
```

**Step 2: Update "chunk has fluid" checks**

The `chunk.fluids.iter().any(|c| !c.is_empty())` checks already work because `is_empty()` checks both slots.

**Step 3: Fix any remaining compilation errors across the whole project**

Run: `cargo check 2>&1`

Fix all `.fluid_id` → `.fluid_id()` and `.mass` → `.mass()` usages throughout the codebase. Search with:
- `grep -rn '\.fluid_id[^(]' src/` (find direct field access that should use method)
- `grep -rn '\.mass[^(]' src/` (find direct field access)

Note: Some accesses to `.primary.fluid_id` and `.primary.mass` are correct (when you need the specific slot). Only the old-style `cell.fluid_id` and `cell.mass` need updating.

**Step 4: Run full test suite**

Run: `cargo test`
Expected: All tests pass.

**Step 5: Commit**

```bash
git add -A
git commit -m "fix(fluid): update systems and fix compilation for dual-slot cells"
```

---

### Task 9: Integration testing

**Step 1: Run the game**

Run: `cargo run`

Test scenarios:
1. Place water above lava — verify no visual gap between them
2. Place lava above water — verify density displacement works (lava sinks)
3. Check F8 debug overlay shows both slots correctly
4. Verify mass conservation (check Mass Stats before and after)
5. Verify wave effects work on the surface of mixed fluid columns

**Step 2: Fix any visual issues**

If gaps still appear, check:
- Is the secondary quad positioned correctly (y0 = primary_fill, y1 = primary_fill + secondary_fill)?
- Is the surface detection correct (secondary slot is the surface)?
- Are the colors correct for each quad?

**Step 3: Final commit**

```bash
git add -A
git commit -m "feat(fluid): complete multi-fluid cell implementation"
```
