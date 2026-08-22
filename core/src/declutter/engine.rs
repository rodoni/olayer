use crate::declutter::spatial_grid::{line_segments_intersect, SpatialHashGrid};
use crate::declutter::types::{
    DeclutterConfig, LabelPlacement, LabelTarget, OctantDirection, Rect2D,
};

/// High-performance 8-octant force-directed label anti-cluttering solver.
pub struct DeclutterEngine {
    config: DeclutterConfig,
}

impl DeclutterEngine {
    pub fn new(config: DeclutterConfig) -> Self {
        Self { config }
    }

    pub fn with_default_config() -> Self {
        Self {
            config: DeclutterConfig::default(),
        }
    }

    /// Computes the bounding box and leader line start/end for a target in a given octant.
    pub fn compute_candidate_geometry(
        target: &LabelTarget,
        octant: OctantDirection,
        leader_length: f32,
    ) -> (Rect2D, [f32; 2], [f32; 2]) {
        let tx = target.x;
        let ty = target.y;
        let w = target.width;
        let h = target.height;
        let diag = leader_length * std::f32::consts::FRAC_1_SQRT_2;

        let (rect_x, rect_y, end_x, end_y) = match octant {
            OctantDirection::North => (tx - w * 0.5, ty - leader_length - h, tx, ty - leader_length),
            OctantDirection::NorthEast => (tx + diag, ty - diag - h * 0.5, tx + diag, ty - diag),
            OctantDirection::East => (tx + leader_length, ty - h * 0.5, tx + leader_length, ty),
            OctantDirection::SouthEast => (tx + diag, ty + diag - h * 0.5, tx + diag, ty + diag),
            OctantDirection::South => (tx - w * 0.5, ty + leader_length, tx, ty + leader_length),
            OctantDirection::SouthWest => (tx - diag - w, ty + diag - h * 0.5, tx - diag, ty + diag),
            OctantDirection::West => (tx - leader_length - w, ty - h * 0.5, tx - leader_length, ty),
            OctantDirection::NorthWest => (tx - diag - w, ty - diag - h * 0.5, tx - diag, ty - diag),
        };

        let rect = Rect2D::new(rect_x, rect_y, w, h);
        let leader_start = [tx, ty];
        let leader_end = [end_x, end_y];

        (rect, leader_start, leader_end)
    }

    /// Computes heading conflict penalty (0.0 to 1.0) if leader arm is placed near velocity vector.
    fn compute_heading_conflict(octant: OctantDirection, heading_rad: Option<f32>) -> f32 {
        if let Some(hdg) = heading_rad {
            let octant_rad = octant.angle_rad();
            let mut diff = (octant_rad - hdg).abs();
            while diff > std::f32::consts::PI {
                diff = (2.0 * std::f32::consts::PI - diff).abs();
            }
            // Heavily penalize if within +/- 45 deg of aircraft heading
            if diff < std::f32::consts::FRAC_PI_4 {
                (std::f32::consts::FRAC_PI_4 - diff) / std::f32::consts::FRAC_PI_4
            } else {
                0.0
            }
        } else {
            0.0
        }
    }

    /// Solves optimal label placement across all provided targets.
    pub fn solve(&self, targets: &[LabelTarget]) -> Vec<LabelPlacement> {
        let n = targets.len();
        if n == 0 {
            return Vec::new();
        }

        // Sort targets by priority (highest priority 0 first)
        let mut sorted_indices: Vec<usize> = (0..n).collect();
        sorted_indices.sort_by_key(|&i| targets[i].priority);

        let mut placements: Vec<Option<LabelPlacement>> = vec![None; n];
        let mut grid = SpatialHashGrid::new(self.config.leader_length_px * 2.0 + 50.0);

        // ====================================================================
        // Pass 1: Greedy Placement
        // ====================================================================
        for &idx in &sorted_indices {
            let target = &targets[idx];
            let mut best_octant = OctantDirection::NorthEast;
            let mut best_cost = f32::MAX;
            let mut best_rect = Rect2D::new(0.0, 0.0, 0.0, 0.0);
            let mut best_start = [0.0, 0.0];
            let mut best_end = [0.0, 0.0];

            for octant in OctantDirection::all() {
                let (rect, start, end) = Self::compute_candidate_geometry(
                    target,
                    octant,
                    self.config.leader_length_px,
                );
                let expanded_rect = rect.expanded(self.config.safety_margin_px);

                // 1. Preference cost
                let mut cost = octant.default_preference_cost() * self.config.weight_preference;

                // 2. Heading conflict cost
                let hdg_conflict = Self::compute_heading_conflict(octant, target.heading_rad);
                cost += hdg_conflict * self.config.weight_heading;

                // 3. Overlap cost using Spatial Grid
                let candidate_indices = grid.query_candidates(&expanded_rect);
                let mut overlap_area = 0.0;
                let mut leader_crossings = 0;

                for &other_idx in &candidate_indices {
                    if other_idx == idx {
                        continue;
                    }
                    if let Some(ref other_p) = placements[other_idx] {
                        overlap_area += expanded_rect.intersection_area(&other_p.rect);
                        if line_segments_intersect(start, end, other_p.leader_start, other_p.leader_end) {
                            leader_crossings += 1;
                        }
                    }
                }

                cost += overlap_area * self.config.weight_overlap;
                cost += (leader_crossings as f32) * self.config.weight_leader_crossing;

                if cost < best_cost {
                    best_cost = cost;
                    best_octant = octant;
                    best_rect = rect;
                    best_start = start;
                    best_end = end;
                    // Early exit if optimal zero-overlap preferred placement found
                    if cost < 0.01 {
                        break;
                    }
                }
            }

            let placement = LabelPlacement {
                id: target.id.clone(),
                octant: best_octant,
                rect: best_rect,
                leader_start: best_start,
                leader_end: best_end,
                cost: best_cost,
                visible: true,
            };

            grid.insert(idx, &best_rect);
            placements[idx] = Some(placement);
        }

        // ====================================================================
        // Pass 2: Force-Directed Iterative Relaxation
        // ====================================================================
        for _ in 0..self.config.max_iterations {
            let mut improved = false;

            for &idx in &sorted_indices {
                let current_cost = placements[idx].as_ref().map(|p| p.cost).unwrap_or(0.0);
                if current_cost < 0.1 {
                    continue; // Already clean
                }

                let target = &targets[idx];
                let mut best_octant = placements[idx].as_ref().unwrap().octant;
                let mut best_cost = current_cost;
                let mut best_rect = placements[idx].as_ref().unwrap().rect;
                let mut best_start = placements[idx].as_ref().unwrap().leader_start;
                let mut best_end = placements[idx].as_ref().unwrap().leader_end;

                for octant in OctantDirection::all() {
                    let (rect, start, end) = Self::compute_candidate_geometry(
                        target,
                        octant,
                        self.config.leader_length_px,
                    );
                    let expanded_rect = rect.expanded(self.config.safety_margin_px);

                    let mut cost = octant.default_preference_cost() * self.config.weight_preference;
                    cost += Self::compute_heading_conflict(octant, target.heading_rad)
                        * self.config.weight_heading;

                    let candidate_indices = grid.query_candidates(&expanded_rect);
                    let mut overlap_area = 0.0;
                    let mut leader_crossings = 0;

                    for &other_idx in &candidate_indices {
                        if other_idx == idx {
                            continue;
                        }
                        if let Some(ref other_p) = placements[other_idx] {
                            overlap_area += expanded_rect.intersection_area(&other_p.rect);
                            if line_segments_intersect(start, end, other_p.leader_start, other_p.leader_end) {
                                leader_crossings += 1;
                            }
                        }
                    }

                    cost += overlap_area * self.config.weight_overlap;
                    cost += (leader_crossings as f32) * self.config.weight_leader_crossing;

                    if cost < best_cost - 0.5 {
                        best_cost = cost;
                        best_octant = octant;
                        best_rect = rect;
                        best_start = start;
                        best_end = end;
                        improved = true;
                    }
                }

                placements[idx] = Some(LabelPlacement {
                    id: target.id.clone(),
                    octant: best_octant,
                    rect: best_rect,
                    leader_start: best_start,
                    leader_end: best_end,
                    cost: best_cost,
                    visible: true,
                });
            }

            if !improved {
                break;
            }
        }

        placements.into_iter().map(|p| p.unwrap()).collect()
    }
}
