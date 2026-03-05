use super::data::*;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Gravity contribution to pressure per unit of depth.
const GRAVITY_SCALE: f32 = 0.1;

/// Bias added to downward flow (encourages liquid to fall).
const GRAVITY_BIAS_DOWN: f32 = 2.0;

/// Bias added to upward flow (discourages liquid from rising).
const GRAVITY_BIAS_UP: f32 = -1.0;

// ---------------------------------------------------------------------------
// SimGrid — standalone simulation grid for testing
// ---------------------------------------------------------------------------

/// Self-contained liquid simulation grid. Owns its own cell and solid data
/// so the core algorithm can be tested without ECS or chunk infrastructure.
pub struct SimGrid {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<LiquidCell>,
    pub solid: Vec<bool>,
    /// Per-cell outgoing flows: [right, up, left, down].
    flows: Vec<[f32; 4]>,
    /// Per-cell pressure scratch buffer.
    pressure: Vec<f32>,
}

impl SimGrid {
    pub fn new(width: usize, height: usize) -> Self {
        let len = width * height;
        Self {
            width,
            height,
            cells: vec![LiquidCell::EMPTY; len],
            solid: vec![false; len],
            flows: vec![[0.0; 4]; len],
            pressure: vec![0.0; len],
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

    /// Returns the neighbor coordinate in the given face direction, or `None`
    /// if it would be out of bounds.
    pub fn neighbor(&self, x: usize, y: usize, face: usize) -> Option<(usize, usize)> {
        let (dx, dy) = FACE_OFFSET[face];
        let nx = x as i32 + dx;
        let ny = y as i32 + dy;
        if nx < 0 || ny < 0 || nx >= self.width as i32 || ny >= self.height as i32 {
            return None;
        }
        Some((nx as usize, ny as usize))
    }
}

// ---------------------------------------------------------------------------
// Simulation step
// ---------------------------------------------------------------------------

/// Advance the liquid simulation by one time step.
///
/// `densities` and `viscosities` are indexed by `LiquidId.0` (index 0 is
/// unused since `LiquidId::NONE` = 0).
pub fn step(grid: &mut SimGrid, densities: &[f32], viscosities: &[f32], dt: f32) {
    let len = grid.width * grid.height;

    // Reset scratch buffers.
    for i in 0..len {
        grid.flows[i] = [0.0; 4];
        grid.pressure[i] = 0.0;
    }

    // -- Phase 1: Compute pressure ------------------------------------------
    // Scan each column top-down, accumulating depth for contiguous same-liquid
    // cells.
    for x in 0..grid.width {
        let mut depth_above: f32 = 0.0;
        let mut prev_liquid = LiquidId::NONE;

        // y goes from top (height-1) to bottom (0).
        for y in (0..grid.height).rev() {
            let i = grid.idx(x, y);
            let cell = grid.cells[i];

            if cell.is_empty() || grid.solid[i] {
                // Break the column — reset depth accumulator.
                depth_above = 0.0;
                prev_liquid = LiquidId::NONE;
                continue;
            }

            // Reset depth if liquid type changes.
            if cell.liquid_type != prev_liquid {
                depth_above = 0.0;
                prev_liquid = cell.liquid_type;
            }

            let density = densities
                .get(cell.liquid_type.0 as usize)
                .copied()
                .unwrap_or(1.0);
            grid.pressure[i] = cell.level + density * GRAVITY_SCALE * depth_above;

            depth_above += cell.level;
        }
    }

    // -- Phase 2: Compute flows ---------------------------------------------
    for y in 0..grid.height {
        for x in 0..grid.width {
            let i = grid.idx(x, y);
            let cell = grid.cells[i];

            if cell.is_empty() || grid.solid[i] {
                continue;
            }

            let viscosity = viscosities
                .get(cell.liquid_type.0 as usize)
                .copied()
                .unwrap_or(1.0)
                .max(0.01); // prevent division by zero

            let p_a = grid.pressure[i];

            for face in 0..4 {
                let Some((nx, ny)) = grid.neighbor(x, y, face) else {
                    continue;
                };
                let ni = grid.idx(nx, ny);

                // Solid blocks all flow.
                if grid.solid[ni] {
                    continue;
                }

                let cell_b = grid.cells[ni];

                // Block flow into a cell with a denser *different* liquid.
                if !cell_b.is_empty() && cell_b.liquid_type != cell.liquid_type {
                    let density_a = densities
                        .get(cell.liquid_type.0 as usize)
                        .copied()
                        .unwrap_or(1.0);
                    let density_b = densities
                        .get(cell_b.liquid_type.0 as usize)
                        .copied()
                        .unwrap_or(1.0);
                    if density_a < density_b {
                        continue;
                    }
                }

                let p_b = grid.pressure[ni];

                // Gravity bias: encourage downward, discourage upward.
                let gravity_bias = match face {
                    FACE_DOWN => GRAVITY_BIAS_DOWN,
                    FACE_UP => GRAVITY_BIAS_UP,
                    _ => 0.0,
                };

                let flow = dt * (p_a - p_b + gravity_bias) / viscosity;
                // Only store positive (outgoing) flows from this cell.
                if flow > 0.0 {
                    grid.flows[i][face] = flow.min(MAX_FLOW);
                }
            }
        }
    }

    // Validate: total outgoing flow cannot exceed cell level.
    for i in 0..len {
        if grid.cells[i].is_empty() || grid.solid[i] {
            continue;
        }

        let total_out: f32 = grid.flows[i].iter().copied().sum();
        if total_out > grid.cells[i].level {
            let scale = grid.cells[i].level / total_out;
            for f in 0..4 {
                grid.flows[i][f] *= scale;
            }
        }
    }

    // -- Phase 3: Update levels ---------------------------------------------
    // Collect level deltas first to avoid order-dependent mutation.
    let mut deltas = vec![0.0_f32; len];

    for y in 0..grid.height {
        for x in 0..grid.width {
            let i = grid.idx(x, y);
            if grid.solid[i] {
                continue;
            }

            let cell = grid.cells[i];
            if cell.is_empty() {
                // Even empty cells can receive incoming flow.
            }

            // Subtract outgoing flows.
            for face in 0..4 {
                let out = grid.flows[i][face];
                if out > 0.0 {
                    deltas[i] -= out;

                    // Add to neighbor as incoming.
                    if let Some((nx, ny)) = grid.neighbor(x, y, face) {
                        let ni = grid.idx(nx, ny);
                        deltas[ni] += out;

                        // Propagate liquid type to empty neighbors.
                        if grid.cells[ni].is_empty() {
                            grid.cells[ni].liquid_type = cell.liquid_type;
                        }
                    }
                }
            }
        }
    }

    // Apply deltas.
    for i in 0..len {
        if grid.solid[i] {
            continue;
        }

        grid.cells[i].level += deltas[i];

        if grid.cells[i].level < MIN_LEVEL {
            grid.cells[i] = LiquidCell::EMPTY;
        } else if grid.cells[i].level > MAX_LEVEL {
            grid.cells[i].level = MAX_LEVEL;
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
        // Place water at (1, 2) — top-center of 3x3.
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

        for _ in 0..200 {
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

        for _ in 0..50 {
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
}
