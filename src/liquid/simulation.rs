use std::collections::HashMap;

use super::data::*;

// ---------------------------------------------------------------------------
// SimGrid — standalone simulation grid for testing
// ---------------------------------------------------------------------------

/// Self-contained liquid simulation grid. Owns its own cell and solid data
/// so the core jgallant CA algorithm can be tested without ECS or chunks.
pub struct SimGrid {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<LiquidCell>,
    pub solid: Vec<bool>,
    /// Per-cell level delta accumulator (applied at end of each iteration).
    diffs: Vec<f32>,
}

impl SimGrid {
    pub fn new(width: usize, height: usize) -> Self {
        let len = width * height;
        Self {
            width,
            height,
            cells: vec![LiquidCell::EMPTY; len],
            solid: vec![false; len],
            diffs: vec![0.0; len],
        }
    }

    // -- accessors ----------------------------------------------------------

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

    pub fn set_solid(&mut self, x: usize, y: usize, val: bool) {
        let i = self.idx(x, y);
        self.solid[i] = val;
    }

    pub fn is_solid(&self, x: usize, y: usize) -> bool {
        self.solid[self.idx(x, y)]
    }

    /// Sum of all liquid levels in the grid.
    pub fn total_volume(&self) -> f32 {
        self.cells.iter().map(|c| c.level).sum()
    }
}

// ---------------------------------------------------------------------------
// Simulation step — jgallant / Starbound cellular automata
// ---------------------------------------------------------------------------

/// Can liquid of type `lt` flow into cell at index `ni`?
fn can_flow_to_idx(
    grid: &SimGrid,
    ni: usize,
    lt: LiquidId,
    claimed: &HashMap<usize, LiquidId>,
) -> bool {
    if grid.solid[ni] {
        return false;
    }
    let dest = grid.cells[ni];
    if dest.is_empty() {
        match claimed.get(&ni) {
            Some(&t) => t == lt,
            None => true,
        }
    } else {
        dest.liquid_type == lt
    }
}

/// Get the destination level for flow calculation (0 if different type or empty).
fn dest_level_idx(grid: &SimGrid, ni: usize, lt: LiquidId) -> f32 {
    let dest = grid.cells[ni];
    if dest.liquid_type == lt {
        dest.level
    } else {
        0.0
    }
}

/// Advance the liquid simulation by one time step.
///
/// `densities` and `viscosities` are indexed by `LiquidId.0` (index 0 is
/// unused since `LiquidId::NONE` = 0).
///
/// NOTE: `dt` is accepted for API compatibility but ignored — the jgallant
/// algorithm is purely iterative; convergence speed is controlled by
/// `FLOW_SPEED` and the number of calls to `step()` per frame.
pub fn step(grid: &mut SimGrid, densities: &[f32], viscosities: &[f32], _dt: f32) {
    let w = grid.width;
    let h = grid.height;
    let len = w * h;

    // Reset diffs.
    for d in grid.diffs.iter_mut().take(len) {
        *d = 0.0;
    }

    // Track which empty cells have been "claimed" by a liquid type in this
    // iteration. Prevents two different types from both flowing into the
    // same empty cell (which would cause type conflicts in the diffs).
    let mut claimed: HashMap<usize, LiquidId> = HashMap::new();

    // Track source type for each cell that receives positive flow.
    // Used to propagate liquid type to newly-filled empty cells.
    let mut source_types: HashMap<usize, LiquidId> = HashMap::new();

    // Process every cell bottom-to-top, left-to-right.
    // The /4 vs /3 asymmetry for left/right compensates for this sweep order.
    for y in 0..h {
        for x in 0..w {
            let i = grid.idx(x, y);
            let cell = grid.cells[i];

            if cell.is_empty() || grid.solid[i] {
                continue;
            }

            let viscosity_factor = viscosities
                .get(cell.liquid_type.0 as usize)
                .copied()
                .unwrap_or(1.0)
                .recip()
                .min(1.0);

            let lt = cell.liquid_type;
            let mut remaining = cell.level;

            // ---- 1. Flow Down ----
            if y > 0 {
                let bi = grid.idx(x, y - 1);
                if can_flow_to_idx(grid, bi, lt, &claimed) {
                    let dest = dest_level_idx(grid, bi, lt);
                    let mut flow = vertical_flow_target(remaining, dest) - dest;
                    flow = constrain_flow(flow, remaining);
                    if flow > MIN_FLOW {
                        flow *= FLOW_SPEED * viscosity_factor;
                    }
                    if flow > 0.0 {
                        grid.diffs[i] -= flow;
                        grid.diffs[bi] += flow;
                        remaining -= flow;
                        if grid.cells[bi].is_empty() {
                            claimed.insert(bi, lt);
                        }
                        source_types.insert(bi, lt);
                    }
                    if remaining < MIN_LEVEL {
                        continue;
                    }
                }
            }

            // ---- 2. Flow Left ----
            if x > 0 {
                let li = grid.idx(x - 1, y);
                if can_flow_to_idx(grid, li, lt, &claimed) {
                    let dest = dest_level_idx(grid, li, lt);
                    let mut flow = (remaining - dest) / 4.0;
                    flow = constrain_flow(flow, remaining);
                    if flow > MIN_FLOW {
                        flow *= FLOW_SPEED * viscosity_factor;
                    }
                    if flow > 0.0 {
                        grid.diffs[i] -= flow;
                        grid.diffs[li] += flow;
                        remaining -= flow;
                        if grid.cells[li].is_empty() {
                            claimed.insert(li, lt);
                        }
                        source_types.insert(li, lt);
                    }
                    if remaining < MIN_LEVEL {
                        continue;
                    }
                }
            }

            // ---- 3. Flow Right ----
            if x + 1 < w {
                let ri = grid.idx(x + 1, y);
                if can_flow_to_idx(grid, ri, lt, &claimed) {
                    let dest = dest_level_idx(grid, ri, lt);
                    // /3 (not /4) compensates for left-to-right sweep bias.
                    let mut flow = (remaining - dest) / 3.0;
                    flow = constrain_flow(flow, remaining);
                    if flow > MIN_FLOW {
                        flow *= FLOW_SPEED * viscosity_factor;
                    }
                    if flow > 0.0 {
                        grid.diffs[i] -= flow;
                        grid.diffs[ri] += flow;
                        remaining -= flow;
                        if grid.cells[ri].is_empty() {
                            claimed.insert(ri, lt);
                        }
                        source_types.insert(ri, lt);
                    }
                    if remaining < MIN_LEVEL {
                        continue;
                    }
                }
            }

            // ---- 4. Flow Up ----
            // Only happens when the cell is over-full (> MAX_LEVEL), creating
            // the communicating-vessels effect.
            if y + 1 < h {
                let ti = grid.idx(x, y + 1);
                if can_flow_to_idx(grid, ti, lt, &claimed) {
                    let dest = dest_level_idx(grid, ti, lt);
                    let mut flow = remaining - vertical_flow_target(remaining, dest);
                    flow = constrain_flow(flow, remaining);
                    if flow > MIN_FLOW {
                        flow *= FLOW_SPEED * viscosity_factor;
                    }
                    if flow > 0.0 {
                        grid.diffs[i] -= flow;
                        grid.diffs[ti] += flow;
                        // remaining -= flow; // not needed, last direction
                        if grid.cells[ti].is_empty() {
                            claimed.insert(ti, lt);
                        }
                        source_types.insert(ti, lt);
                    }
                }
            }
        }
    }

    // -- Apply diffs -------------------------------------------------------

    // Set liquid types for newly-filled cells.
    for (&idx, &ltype) in &source_types {
        if grid.cells[idx].is_empty() && grid.diffs[idx] > 0.0 {
            grid.cells[idx].liquid_type = ltype;
        }
    }

    // Apply level changes.
    for i in 0..len {
        if grid.solid[i] {
            continue;
        }
        grid.cells[i].level += grid.diffs[i];
        if grid.cells[i].level < MIN_LEVEL {
            grid.cells[i] = LiquidCell::EMPTY;
        }
    }

    // -- Density sorting: swap lighter-below-denser -------------------------
    for x in 0..w {
        for y in 0..(h.saturating_sub(1)) {
            let bi = grid.idx(x, y);
            let ti = grid.idx(x, y + 1);

            if grid.solid[bi] || grid.solid[ti] {
                continue;
            }

            let bottom = grid.cells[bi];
            let top = grid.cells[ti];

            if bottom.is_empty() || top.is_empty() {
                continue;
            }
            if bottom.liquid_type == top.liquid_type {
                continue;
            }

            let d_bottom = densities
                .get(bottom.liquid_type.0 as usize)
                .copied()
                .unwrap_or(1.0);
            let d_top = densities
                .get(top.liquid_type.0 as usize)
                .copied()
                .unwrap_or(1.0);

            if d_bottom < d_top {
                // Lighter below denser — swap.
                grid.cells[bi] = top;
                grid.cells[ti] = bottom;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Standard densities: index 0 unused, 1=water, 2=oil.
    fn water_densities() -> Vec<f32> {
        vec![0.0, 1.0]
    }

    fn water_viscosities() -> Vec<f32> {
        vec![0.0, 1.0]
    }

    /// Densities with water (1) and oil (2).
    fn multi_densities() -> Vec<f32> {
        vec![0.0, 1.0, 0.8]
    }

    fn multi_viscosities() -> Vec<f32> {
        vec![0.0, 1.0, 1.0]
    }

    fn water_cell(level: f32) -> LiquidCell {
        LiquidCell {
            liquid_type: LiquidId(1),
            level,
        }
    }

    fn oil_cell(level: f32) -> LiquidCell {
        LiquidCell {
            liquid_type: LiquidId(2),
            level,
        }
    }

    // ---- water_falls_down -------------------------------------------------

    #[test]
    fn water_falls_down() {
        let mut grid = SimGrid::new(3, 3);
        // Place water at (1, 2) — top-center of 3×3.
        grid.set(1, 2, water_cell(1.0));

        let densities = water_densities();
        let viscosities = water_viscosities();

        step(&mut grid, &densities, &viscosities, 0.1);

        let above = grid.get(1, 2);
        let below = grid.get(1, 1);
        assert!(
            above.level < 1.0,
            "water at (1,2) should decrease after step, got {}",
            above.level
        );
        assert!(
            below.level > 0.0,
            "water at (1,1) should increase after step, got {}",
            below.level
        );
    }

    // ---- water_spreads_horizontally ---------------------------------------

    #[test]
    fn water_spreads_horizontally() {
        let mut grid = SimGrid::new(5, 3);
        // Solid floor at y=0.
        for x in 0..5 {
            grid.set_solid(x, 0, true);
        }
        // Water at (2, 1).
        grid.set(2, 1, water_cell(1.0));

        let densities = water_densities();
        let viscosities = water_viscosities();

        for _ in 0..20 {
            step(&mut grid, &densities, &viscosities, 0.1);
        }

        let left = grid.get(1, 1);
        let right = grid.get(3, 1);
        assert!(
            left.level > MIN_LEVEL,
            "water should spread left to (1,1), got {}",
            left.level
        );
        assert!(
            right.level > MIN_LEVEL,
            "water should spread right to (3,1), got {}",
            right.level
        );
    }

    // ---- pressure_u_tube --------------------------------------------------

    #[test]
    fn pressure_u_tube() {
        // 7 wide, 8 tall.
        // Solid floor at y=0.
        // Solid center wall at x=3 from y=2..=6 (open at y=1 so water flows
        // through the bottom).
        let mut grid = SimGrid::new(7, 8);

        // Floor.
        for x in 0..7 {
            grid.set_solid(x, 0, true);
        }

        // Center wall: x=3, y=2..=6.
        for y in 2..=6 {
            grid.set_solid(3, y, true);
        }

        // Fill left side with water: x=0..=2, y=1..=5.
        for x in 0..=2 {
            for y in 1..=5 {
                grid.set(x, y, water_cell(1.0));
            }
        }

        let densities = water_densities();
        let viscosities = water_viscosities();

        for _ in 0..500 {
            step(&mut grid, &densities, &viscosities, 0.1);
        }

        // Water should appear on the right side (x=4..=6).
        let right_volume: f32 = (4..=6)
            .flat_map(|x| (1..grid.height).map(move |y| (x, y)))
            .map(|(x, y)| grid.get(x, y).level)
            .sum();

        assert!(
            right_volume > 0.5,
            "water should flow through U-tube to right side, got volume {}",
            right_volume
        );
    }

    // ---- oil_floats_on_water ----------------------------------------------

    #[test]
    fn oil_floats_on_water() {
        let mut grid = SimGrid::new(3, 6);

        // Solid floor at y=0.
        for x in 0..3 {
            grid.set_solid(x, 0, true);
        }

        // Water at y=1..=2 in center column.
        grid.set(1, 1, water_cell(1.0));
        grid.set(1, 2, water_cell(1.0));

        // Oil at y=3.
        grid.set(1, 3, oil_cell(1.0));

        let densities = multi_densities();
        let viscosities = multi_viscosities();

        for _ in 0..100 {
            step(&mut grid, &densities, &viscosities, 0.1);
        }

        // Find highest cell with oil and highest cell with water.
        let mut highest_oil: Option<usize> = None;
        let mut highest_water: Option<usize> = None;

        for y in (0..grid.height).rev() {
            for x in 0..grid.width {
                let cell = grid.get(x, y);
                if cell.level > MIN_LEVEL {
                    if cell.liquid_type == LiquidId(2) && highest_oil.is_none() {
                        highest_oil = Some(y);
                    }
                    if cell.liquid_type == LiquidId(1) && highest_water.is_none() {
                        highest_water = Some(y);
                    }
                }
            }
        }

        let oil_y = highest_oil.expect("oil should still exist in the grid");
        let water_y = highest_water.expect("water should still exist in the grid");

        assert!(
            oil_y >= water_y,
            "oil (density 0.8) should float above water (density 1.0): oil_y={}, water_y={}",
            oil_y,
            water_y
        );
    }

    // ---- conservation_of_volume -------------------------------------------

    #[test]
    fn conservation_of_volume() {
        let mut grid = SimGrid::new(5, 5);

        // Solid floor.
        for x in 0..5 {
            grid.set_solid(x, 0, true);
        }

        // Place 1.5 total volume.
        grid.set(2, 2, water_cell(1.0));
        grid.set(2, 1, water_cell(0.5));

        let initial_volume = grid.total_volume();
        assert!(
            (initial_volume - 1.5).abs() < 0.001,
            "initial volume should be 1.5, got {}",
            initial_volume
        );

        let densities = water_densities();
        let viscosities = water_viscosities();

        for _ in 0..100 {
            step(&mut grid, &densities, &viscosities, 0.1);
        }

        let final_volume = grid.total_volume();
        assert!(
            (final_volume - initial_volume).abs() < 0.01,
            "volume should be conserved: initial={}, final={}",
            initial_volume,
            final_volume
        );
    }

    // ---- liquid_stops_at_solid --------------------------------------------

    #[test]
    fn liquid_stops_at_solid() {
        let mut grid = SimGrid::new(5, 3);

        // Solid floor.
        for x in 0..5 {
            grid.set_solid(x, 0, true);
        }

        // Solid wall at x=3.
        for y in 0..3 {
            grid.set_solid(3, y, true);
        }

        // Water at (2, 1) — right next to the wall.
        grid.set(2, 1, water_cell(1.0));

        let densities = water_densities();
        let viscosities = water_viscosities();

        for _ in 0..50 {
            step(&mut grid, &densities, &viscosities, 0.1);
        }

        // Nothing should leak past the wall.
        let beyond_wall = grid.get(4, 1);
        assert!(
            beyond_wall.level < MIN_LEVEL,
            "liquid should not pass through solid wall, got {} at (4,1)",
            beyond_wall.level
        );
    }

    // ---- empty_grid_is_noop -----------------------------------------------

    #[test]
    fn empty_grid_is_noop() {
        let mut grid = SimGrid::new(4, 4);

        let densities = water_densities();
        let viscosities = water_viscosities();

        step(&mut grid, &densities, &viscosities, 0.1);

        for y in 0..4 {
            for x in 0..4 {
                let cell = grid.get(x, y);
                assert!(
                    cell.is_empty(),
                    "empty grid should remain empty after step, cell ({},{}) has level {}",
                    x,
                    y,
                    cell.level
                );
            }
        }
    }

    // ---- compression_creates_pressure -------------------------------------

    #[test]
    fn compression_creates_upward_flow() {
        // A tall column of water on a solid floor should push liquid
        // sideways at the bottom due to compression.
        let mut grid = SimGrid::new(5, 8);

        // Solid floor.
        for x in 0..5 {
            grid.set_solid(x, 0, true);
        }

        // 5-high column of water at x=2.
        for y in 1..=5 {
            grid.set(2, y, water_cell(1.0));
        }

        let densities = water_densities();
        let viscosities = water_viscosities();

        for _ in 0..100 {
            step(&mut grid, &densities, &viscosities, 0.1);
        }

        // Water should have spread sideways at the bottom.
        let left = grid.get(1, 1);
        let right = grid.get(3, 1);
        assert!(
            left.level > MIN_LEVEL,
            "tall column should push water left at bottom, got {}",
            left.level
        );
        assert!(
            right.level > MIN_LEVEL,
            "tall column should push water right at bottom, got {}",
            right.level
        );
    }

    // ---- communicating_vessels_equalize -----------------------------------

    #[test]
    fn communicating_vessels_equalize() {
        // Two connected containers should equalize water level.
        //
        //  W...|....
        //  W...|....
        //  W...|....
        //  WWWWWWWWW   <- shared bottom row
        //  #########   <- floor
        //
        let mut grid = SimGrid::new(9, 6);

        // Floor.
        for x in 0..9 {
            grid.set_solid(x, 0, true);
        }

        // Wall at x=4, y=2..=5.
        for y in 2..=5 {
            grid.set_solid(4, y, true);
        }

        // Fill left container: x=0..=3, y=1..=4.
        for x in 0..=3 {
            for y in 1..=4 {
                grid.set(x, y, water_cell(1.0));
            }
        }

        let initial_volume = grid.total_volume();
        let densities = water_densities();
        let viscosities = water_viscosities();

        for _ in 0..1000 {
            step(&mut grid, &densities, &viscosities, 0.1);
        }

        // Volume must be conserved (allow small loss from MIN_LEVEL zeroing).
        let final_volume = grid.total_volume();
        assert!(
            (final_volume - initial_volume).abs() < 0.15,
            "volume must be conserved: initial={}, final={}",
            initial_volume,
            final_volume
        );

        // Right container should have received significant water.
        let right_volume: f32 = (5..=8)
            .flat_map(|x| (1..grid.height).map(move |y| (x, y)))
            .map(|(x, y)| grid.get(x, y).level)
            .sum();
        assert!(
            right_volume > 2.0,
            "right container should have significant water (communicating vessels), got {}",
            right_volume
        );
    }
}
