use std::collections::HashMap;
use crate::declutter::types::Rect2D;

/// Uniform 2D spatial hash grid for $O(1)$ fast bounding box collision queries.
pub struct SpatialHashGrid {
    cell_size: f32,
    cells: HashMap<(i32, i32), Vec<usize>>,
}

impl SpatialHashGrid {
    pub fn new(cell_size: f32) -> Self {
        Self {
            cell_size: if cell_size > 0.0 { cell_size } else { 64.0 },
            cells: HashMap::new(),
        }
    }

    #[inline]
    fn grid_coord(&self, val: f32) -> i32 {
        (val / self.cell_size).floor() as i32
    }

    /// Inserts an item bounding box into all intersecting grid cells.
    pub fn insert(&mut self, item_index: usize, rect: &Rect2D) {
        let min_cx = self.grid_coord(rect.x);
        let max_cx = self.grid_coord(rect.x + rect.width);
        let min_cy = self.grid_coord(rect.y);
        let max_cy = self.grid_coord(rect.y + rect.height);

        for cx in min_cx..=max_cx {
            for cy in min_cy..=max_cy {
                self.cells.entry((cx, cy)).or_default().push(item_index);
            }
        }
    }

    /// Queries all item indices overlapping the given bounding box.
    pub fn query_candidates(&self, rect: &Rect2D) -> Vec<usize> {
        let min_cx = self.grid_coord(rect.x);
        let max_cx = self.grid_coord(rect.x + rect.width);
        let min_cy = self.grid_coord(rect.y);
        let max_cy = self.grid_coord(rect.y + rect.height);

        let mut candidates = Vec::new();
        for cx in min_cx..=max_cx {
            for cy in min_cy..=max_cy {
                if let Some(list) = self.cells.get(&(cx, cy)) {
                    candidates.extend_from_slice(list);
                }
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        candidates
    }

    pub fn clear(&mut self) {
        self.cells.clear();
    }
}

/// Checks whether 2D line segment AB intersects line segment CD.
pub fn line_segments_intersect(
    a: [f32; 2],
    b: [f32; 2],
    c: [f32; 2],
    d: [f32; 2],
) -> bool {
    let ccw = |p1: [f32; 2], p2: [f32; 2], p3: [f32; 2]| -> f32 {
        (p2[0] - p1[0]) * (p3[1] - p1[1]) - (p2[1] - p1[1]) * (p3[0] - p1[0])
    };

    let cp1 = ccw(a, b, c);
    let cp2 = ccw(a, b, d);
    let cp3 = ccw(c, d, a);
    let cp4 = ccw(c, d, b);

    ((cp1 > 0.0 && cp2 < 0.0) || (cp1 < 0.0 && cp2 > 0.0))
        && ((cp3 > 0.0 && cp4 < 0.0) || (cp3 < 0.0 && cp4 > 0.0))
}
