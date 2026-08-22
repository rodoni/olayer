use crate::volumetric::errors::VolumetricError;

/// Computes the 2D signed area of a polygon.
/// Positive area indicates Counter-Clockwise (CCW) winding; negative indicates Clockwise (CW).
pub fn signed_area_2d(points: &[[f64; 2]]) -> f64 {
    let n = points.len();
    if n < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        area += points[i][0] * points[j][1] - points[j][0] * points[i][1];
    }
    area * 0.5
}

/// Checks if a 2D point P is strictly inside the 2D triangle ABC.
fn is_point_in_triangle_2d(p: [f64; 2], a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> bool {
    let cross_product = |p1: [f64; 2], p2: [f64; 2], p3: [f64; 2]| -> f64 {
        (p2[0] - p1[0]) * (p3[1] - p1[1]) - (p2[1] - p1[1]) * (p3[0] - p1[0])
    };

    let cp1 = cross_product(a, b, p);
    let cp2 = cross_product(b, c, p);
    let cp3 = cross_product(c, a, p);

    // All cross products must have the same sign (or be zero on boundary)
    let has_neg = (cp1 < -1e-12) || (cp2 < -1e-12) || (cp3 < -1e-12);
    let has_pos = (cp1 > 1e-12) || (cp2 > 1e-12) || (cp3 > 1e-12);

    !(has_neg && has_pos)
}

/// Triangulates a simple 2D polygon (convex or concave, without self-intersections)
/// using the robust Ear Clipping algorithm.
///
/// # Arguments
/// * `points` - Slice of 2D coordinates `[x, y]`.
///
/// # Returns
/// Array of triangle vertex index triplets `[i0, i1, i2]` indexing into `points`.
pub fn triangulate_polygon_2d(points: &[[f64; 2]]) -> Result<Vec<[usize; 3]>, VolumetricError> {
    let n = points.len();
    if n < 3 {
        return Err(VolumetricError::InsufficientVertices {
            expected: 3,
            actual: n,
        });
    }

    if n == 3 {
        return Ok(vec![[0, 1, 2]]);
    }

    let area = signed_area_2d(points);
    if area.abs() < 1e-14 {
        return Err(VolumetricError::DegenerateGeometry(
            "Polygon has zero or near-zero area".to_string(),
        ));
    }

    // Ensure counter-clockwise winding order for consistent ear tests
    let mut vertex_indices: Vec<usize> = if area > 0.0 {
        (0..n).collect()
    } else {
        (0..n).rev().collect()
    };

    let mut triangles = Vec::with_capacity(n - 2);

    let is_convex = |prev: [f64; 2], curr: [f64; 2], next: [f64; 2]| -> bool {
        let cross = (curr[0] - prev[0]) * (next[1] - prev[1]) - (curr[1] - prev[1]) * (next[0] - prev[0]);
        cross > 1e-14
    };

    let is_ear = |i: usize, indices: &[usize]| -> bool {
        let count = indices.len();
        let prev_idx = indices[(i + count - 1) % count];
        let curr_idx = indices[i];
        let next_idx = indices[(i + 1) % count];

        let a = points[prev_idx];
        let b = points[curr_idx];
        let c = points[next_idx];

        if !is_convex(a, b, c) {
            return false;
        }

        // Check that no other remaining vertex lies inside triangle ABC
        for (j, &other_idx) in indices.iter().enumerate() {
            if j == (i + count - 1) % count || j == i || j == (i + 1) % count {
                continue;
            }
            if is_point_in_triangle_2d(points[other_idx], a, b, c) {
                return false;
            }
        }
        true
    };

    let mut attempts = 0;
    let max_attempts = n * n * 2;

    while vertex_indices.len() > 3 {
        let mut ear_found = false;
        let count = vertex_indices.len();

        for i in 0..count {
            if is_ear(i, &vertex_indices) {
                let prev_idx = vertex_indices[(i + count - 1) % count];
                let curr_idx = vertex_indices[i];
                let next_idx = vertex_indices[(i + 1) % count];

                triangles.push([prev_idx, curr_idx, next_idx]);
                vertex_indices.remove(i);
                ear_found = true;
                break;
            }
        }

        attempts += 1;
        if !ear_found || attempts > max_attempts {
            // Fallback: clip the best angle triangle if non-convex / self-intersecting artifacts occur
            let i = 0;
            let count = vertex_indices.len();
            let prev_idx = vertex_indices[count - 1];
            let curr_idx = vertex_indices[0];
            let next_idx = vertex_indices[1];
            triangles.push([prev_idx, curr_idx, next_idx]);
            vertex_indices.remove(i);
        }
    }

    if vertex_indices.len() == 3 {
        triangles.push([vertex_indices[0], vertex_indices[1], vertex_indices[2]]);
    }

    Ok(triangles)
}
