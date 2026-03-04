use bevy::math::Vec2;
use std::collections::HashMap;

pub struct SpatialHash {
    cell_size: f32,
    inv_cell_size: f32,
    cells: HashMap<(i32, i32), Vec<usize>>,
}

impl SpatialHash {
    pub fn new(cell_size: f32) -> Self {
        Self {
            cell_size,
            inv_cell_size: 1.0 / cell_size,
            cells: HashMap::new(),
        }
    }

    pub fn from_positions(positions: &[Vec2], cell_size: f32) -> Self {
        let mut grid = Self::new(cell_size);
        for (i, pos) in positions.iter().enumerate() {
            grid.insert(i, *pos);
        }
        grid
    }

    fn cell_coord(&self, pos: Vec2) -> (i32, i32) {
        (
            (pos.x * self.inv_cell_size).floor() as i32,
            (pos.y * self.inv_cell_size).floor() as i32,
        )
    }

    pub fn insert(&mut self, index: usize, pos: Vec2) {
        let coord = self.cell_coord(pos);
        self.cells.entry(coord).or_default().push(index);
    }

    pub fn query(&self, pos: Vec2) -> Vec<usize> {
        let (cx, cy) = self.cell_coord(pos);
        let mut result = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(indices) = self.cells.get(&(cx + dx, cy + dy)) {
                    result.extend_from_slice(indices);
                }
            }
        }
        result
    }

    pub fn cell(&self, cx: i32, cy: i32) -> &[usize] {
        self.cells.get(&(cx, cy)).map_or(&[], |v| v.as_slice())
    }

    pub fn clear(&mut self) {
        self.cells.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec2;

    #[test]
    fn empty_grid_returns_no_neighbors() {
        let grid = SpatialHash::new(10.0);
        let neighbors = grid.query(Vec2::ZERO);
        assert!(neighbors.is_empty());
    }

    #[test]
    fn insert_and_find_self() {
        let mut grid = SpatialHash::new(10.0);
        grid.insert(0, Vec2::new(5.0, 5.0));
        let neighbors = grid.query(Vec2::new(5.0, 5.0));
        assert!(neighbors.contains(&0));
    }

    #[test]
    fn find_neighbor_in_adjacent_cell() {
        let mut grid = SpatialHash::new(10.0);
        grid.insert(0, Vec2::new(9.0, 5.0));
        grid.insert(1, Vec2::new(11.0, 5.0));
        let neighbors = grid.query(Vec2::new(9.0, 5.0));
        assert!(neighbors.contains(&0));
        assert!(neighbors.contains(&1));
    }

    #[test]
    fn far_particle_not_found() {
        let mut grid = SpatialHash::new(10.0);
        grid.insert(0, Vec2::new(0.0, 0.0));
        grid.insert(1, Vec2::new(100.0, 100.0));
        let neighbors = grid.query(Vec2::new(0.0, 0.0));
        assert!(neighbors.contains(&0));
        assert!(!neighbors.contains(&1));
    }

    #[test]
    fn clear_removes_all() {
        let mut grid = SpatialHash::new(10.0);
        grid.insert(0, Vec2::new(5.0, 5.0));
        grid.clear();
        let neighbors = grid.query(Vec2::new(5.0, 5.0));
        assert!(neighbors.is_empty());
    }

    #[test]
    fn build_from_positions() {
        let positions = vec![
            Vec2::new(1.0, 1.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(100.0, 100.0),
        ];
        let grid = SpatialHash::from_positions(&positions, 10.0);
        let near = grid.query(Vec2::new(1.5, 1.5));
        assert!(near.contains(&0));
        assert!(near.contains(&1));
        assert!(!near.contains(&2));
    }
}
