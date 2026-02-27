/// Axis-aligned bounding box for 2D collision detection.
pub struct Aabb {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

impl Aabb {
    pub fn from_center(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            min_x: x - w / 2.0,
            max_x: x + w / 2.0,
            min_y: y - h / 2.0,
            max_y: y + h / 2.0,
        }
    }

    pub fn overlaps(&self, other: &Aabb) -> bool {
        self.max_x > other.min_x
            && self.min_x < other.max_x
            && self.max_y > other.min_y
            && self.min_y < other.max_y
    }

    pub fn overlapping_tiles(&self, tile_size: f32) -> TileIterator {
        let min_tx = (self.min_x / tile_size).floor() as i32;
        let max_tx = ((self.max_x - 0.001) / tile_size).floor() as i32;
        let min_ty = (self.min_y / tile_size).floor() as i32;
        let max_ty = ((self.max_y - 0.001) / tile_size).floor() as i32;

        TileIterator {
            min_tx,
            max_tx,
            min_ty,
            max_ty,
            current_x: min_tx,
            current_y: min_ty,
        }
    }
}

/// Zero-allocation iterator over tile coordinates that overlap an AABB.
pub struct TileIterator {
    min_tx: i32,
    max_tx: i32,
    #[allow(dead_code)] // stored for clarity; iteration uses max_ty directly
    min_ty: i32,
    max_ty: i32,
    current_x: i32,
    current_y: i32,
}

impl Iterator for TileIterator {
    type Item = (i32, i32);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_y > self.max_ty {
            return None;
        }

        let result = (self.current_x, self.current_y);

        self.current_x += 1;
        if self.current_x > self.max_tx {
            self.current_x = self.min_tx;
            self.current_y += 1;
        }

        Some(result)
    }
}

/// Build an AABB for the tile at grid coordinates `(tx, ty)`.
pub fn tile_aabb(tx: i32, ty: i32, tile_size: f32) -> Aabb {
    Aabb {
        min_x: tx as f32 * tile_size,
        max_x: (tx + 1) as f32 * tile_size,
        min_y: ty as f32 * tile_size,
        max_y: (ty + 1) as f32 * tile_size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TS: f32 = 32.0;

    #[test]
    fn aabb_from_center() {
        let aabb = Aabb::from_center(100.0, 200.0, 24.0, 48.0);
        assert_eq!(aabb.min_x, 88.0);
        assert_eq!(aabb.max_x, 112.0);
        assert_eq!(aabb.min_y, 176.0);
        assert_eq!(aabb.max_y, 224.0);
    }

    #[test]
    fn overlaps_true() {
        let a = Aabb::from_center(50.0, 50.0, 20.0, 20.0);
        let b = Aabb::from_center(55.0, 55.0, 20.0, 20.0);
        assert!(a.overlaps(&b));
    }

    #[test]
    fn overlaps_false() {
        let a = Aabb::from_center(0.0, 0.0, 10.0, 10.0);
        let b = Aabb::from_center(100.0, 100.0, 10.0, 10.0);
        assert!(!a.overlaps(&b));
    }

    #[test]
    fn overlapping_tiles_single() {
        let center_x = 3.0 * TS + TS / 2.0;
        let center_y = 3.0 * TS + TS / 2.0;
        let aabb = Aabb::from_center(center_x, center_y, 20.0, 20.0);
        let tiles: Vec<_> = aabb.overlapping_tiles(TS).collect();
        assert_eq!(tiles, vec![(3, 3)]);
    }

    #[test]
    fn overlapping_tiles_multiple() {
        let aabb = Aabb::from_center(32.0, 32.0, 24.0, 48.0);
        let tiles: Vec<_> = aabb.overlapping_tiles(TS).collect();
        assert!(tiles.len() >= 2);
        assert!(tiles.contains(&(0, 0)));
    }

    #[test]
    fn tile_aabb_basic() {
        let aabb = tile_aabb(3, 5, TS);
        assert_eq!(aabb.min_x, 96.0);
        assert_eq!(aabb.max_x, 128.0);
        assert_eq!(aabb.min_y, 160.0);
        assert_eq!(aabb.max_y, 192.0);
    }
}
