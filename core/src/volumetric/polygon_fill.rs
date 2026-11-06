//! Bounded planar polygon tessellation with explicit interior rings.
use super::VolumetricError;

pub const MAX_FILL_NODES: usize = 2048;
const MAX_TRIANGLE_VERTICES: usize = 196_608;
const EPS: f64 = 1e-12;
type Point = [f64; 3];
#[derive(Clone, Copy)]
struct Edge {
    a: Point,
    b: Point,
    na: [f64; 2],
    nb: [f64; 2],
    ring: usize,
    index: usize,
    count: usize,
}
fn invalid(message: &str) -> VolumetricError {
    VolumetricError::DegenerateGeometry(message.into())
}
fn cross(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
fn on(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> bool {
    cross(a, b, p).abs() <= EPS
        && (0..2).all(|i| p[i] >= a[i].min(b[i]) - EPS && p[i] <= a[i].max(b[i]) + EPS)
}
fn intersect(a: Edge, b: Edge) -> bool {
    let (c1, c2, c3, c4) = (
        cross(a.na, a.nb, b.na),
        cross(a.na, a.nb, b.nb),
        cross(b.na, b.nb, a.na),
        cross(b.na, b.nb, a.nb),
    );
    (c1 * c2 < 0.0 && c3 * c4 < 0.0)
        || on(a.na, a.nb, b.na)
        || on(a.na, a.nb, b.nb)
        || on(b.na, b.nb, a.na)
        || on(b.na, b.nb, a.nb)
}
fn inside(p: [f64; 2], ring: &[[f64; 2]]) -> bool {
    let mut result = false;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1])
        {
            result = !result;
        }
    }
    result
}
impl Edge {
    fn at(self, x: f64) -> Point {
        let t = ((x - self.na[0]) / (self.nb[0] - self.na[0])).clamp(0.0, 1.0);
        std::array::from_fn(|i| self.a[i] * (1.0 - t) + self.b[i] * t)
    }
    fn y(self, x: f64) -> f64 {
        self.na[1] + (self.nb[1] - self.na[1]) * (x - self.na[0]) / (self.nb[0] - self.na[0])
    }
}

/// Triangulates a closed exterior ring followed by zero or more closed holes.
/// Coordinates are planar x/y plus an interpolated scalar height; no CRS conversion occurs.
/// Returns packed triangle vertices. Winding is ignored. Self-intersections, touching or
/// nested rings and holes outside the exterior are rejected. Work is bounded by 2048 nodes.
///
/// # Errors
/// Returns a coded volumetric error for invalid topology, non-finite/near-degenerate
/// input, inconsistent lengths or the output/work limit.
pub fn triangulate_polygon_rings(
    coords: &[Point],
    ring_lengths: &[usize],
) -> Result<Vec<Point>, VolumetricError> {
    if coords.len() > MAX_FILL_NODES
        || ring_lengths.is_empty()
        || ring_lengths.len() > MAX_FILL_NODES / 4
        || coords.iter().flatten().any(|v| !v.is_finite())
        || ring_lengths
            .iter()
            .try_fold(0usize, |sum, n| sum.checked_add(*n))
            != Some(coords.len())
    {
        return Err(invalid(
            "invalid polygon ring lengths, coordinates or node limit",
        ));
    }
    let min_x = coords.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let min_y = coords.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let max_x = coords
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let max_y = coords
        .iter()
        .map(|p| p[1])
        .fold(f64::NEG_INFINITY, f64::max);
    let scale = (max_x - min_x).max(max_y - min_y);
    if !scale.is_finite() || scale <= 0.0 {
        return Err(invalid("polygon has invalid extent"));
    }
    let norm = |p: Point| [(p[0] - min_x) / scale, (p[1] - min_y) / scale];
    let mut rings = Vec::new();
    let mut edges = Vec::new();
    let mut offset = 0;
    for (ring, &length) in ring_lengths.iter().enumerate() {
        if length < 4 {
            return Err(invalid("closed rings need at least four nodes"));
        }
        let points = &coords[offset..offset + length];
        offset += length;
        if points.first() != points.last() {
            return Err(invalid("polygon rings must be closed"));
        }
        let points = &points[..length - 1];
        let normalized: Vec<_> = points.iter().map(|p| norm(*p)).collect();
        let area: f64 = (0..points.len())
            .map(|i| {
                let (a, b) = (normalized[i], normalized[(i + 1) % points.len()]);
                a[0] * b[1] - a[1] * b[0]
            })
            .sum();
        if area.abs() <= EPS {
            return Err(invalid("polygon ring has near-zero area"));
        }
        for i in 0..points.len() {
            let (a, b, c) = (
                normalized[(i + points.len() - 1) % points.len()],
                normalized[i],
                normalized[(i + 1) % points.len()],
            );
            if (b[0] - c[0]).hypot(b[1] - c[1]) <= EPS
                || (cross(a, b, c).abs() <= EPS
                    && (a[0] - b[0]) * (c[0] - b[0]) + (a[1] - b[1]) * (c[1] - b[1]) > EPS)
            {
                return Err(invalid("duplicate or overlapping polygon edges"));
            }
            edges.push(Edge {
                a: points[i],
                b: points[(i + 1) % points.len()],
                na: b,
                nb: c,
                ring,
                index: i,
                count: points.len(),
            });
        }
        rings.push(normalized);
    }
    for (i, &a) in edges.iter().enumerate() {
        for &b in &edges[i + 1..] {
            let adjacent = a.ring == b.ring
                && ((a.index + 1) % a.count == b.index || (b.index + 1) % b.count == a.index);
            if !adjacent && intersect(a, b) {
                return Err(invalid("polygon rings intersect or touch"));
            }
        }
    }
    for hole in 1..rings.len() {
        if !inside(rings[hole][0], &rings[0]) {
            return Err(invalid("polygon hole lies outside exterior"));
        }
        for other in 1..hole {
            if inside(rings[hole][0], &rings[other]) || inside(rings[other][0], &rings[hole]) {
                return Err(invalid("polygon holes overlap or nest"));
            }
        }
    }
    let mut xs: Vec<_> = edges.iter().map(|e| e.na[0]).collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    let mut output = Vec::new();
    for slab in xs.windows(2) {
        let (left, right) = (slab[0], slab[1]);
        if right - left <= EPS {
            continue;
        }
        let middle = left + (right - left) * 0.5;
        let mut active: Vec<_> = edges
            .iter()
            .copied()
            .filter(|e| e.na[0].min(e.nb[0]) < middle && middle < e.na[0].max(e.nb[0]))
            .collect();
        active.sort_by(|a, b| a.y(middle).total_cmp(&b.y(middle)));
        if active.len() % 2 != 0 {
            return Err(invalid("polygon has inconsistent interior"));
        }
        for pair in active.as_chunks::<2>().0 {
            let (a, b, c, d) = (
                pair[0].at(left),
                pair[0].at(right),
                pair[1].at(left),
                pair[1].at(right),
            );
            for tri in [[a, b, c], [b, d, c]] {
                if cross(norm(tri[0]), norm(tri[1]), norm(tri[2])).abs() <= EPS {
                    continue;
                }
                if output.len() + 3 > MAX_TRIANGLE_VERTICES {
                    return Err(VolumetricError::TriangulationFailed(
                        "polygon fill output limit exceeded".into(),
                    ));
                }
                if tri.iter().flatten().any(|v| !v.is_finite()) {
                    return Err(invalid("polygon interpolation is not finite"));
                }
                output.extend(tri);
            }
        }
    }
    if output.is_empty() {
        return Err(invalid("polygon has no fillable area"));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ring(points: &[[f64; 2]]) -> Vec<Point> {
        points.iter().map(|p| [p[0], p[1], p[0] + p[1]]).collect()
    }
    #[test]
    fn multiple_holes_and_translated_scales_preserve_area() {
        let mut points = ring(&[[0., 0.], [10., 0.], [10., 10.], [0., 10.], [0., 0.]]);
        points.extend(ring(&[[1., 1.], [2., 1.], [2., 2.], [1., 2.], [1., 1.]]));
        points.extend(ring(&[[6., 6.], [8., 6.], [8., 8.], [6., 8.], [6., 6.]]));
        for scale in [1.0, 0.00001, 1e9] {
            let scaled: Vec<_> = points
                .iter()
                .map(|p| {
                    [
                        p[0] * scale + 20.0 * scale,
                        p[1] * scale - 10.0 * scale,
                        p[2],
                    ]
                })
                .collect();
            let triangles = triangulate_polygon_rings(&scaled, &[5, 5, 5]).unwrap();
            let area: f64 = triangles
                .as_chunks::<3>()
                .0
                .iter()
                .map(|p| {
                    cross([p[0][0], p[0][1]], [p[1][0], p[1][1]], [p[2][0], p[2][1]]).abs() * 0.5
                })
                .sum();
            assert!((area / (scale * scale) - 95.0).abs() < 1e-8);
        }
        let nested = ring(&[[1.2, 1.2], [1.8, 1.2], [1.8, 1.8], [1.2, 1.8], [1.2, 1.2]]);
        points.extend(nested);
        assert!(triangulate_polygon_rings(&points, &[5, 5, 5, 5]).is_err());
    }

    #[test]
    fn concave_exterior_and_hole_preserve_area_and_interpolated_height() {
        let mut coords = ring(&[
            [0., 0.],
            [4., 0.],
            [4., 4.],
            [2., 4.],
            [2., 2.],
            [0., 2.],
            [0., 0.],
        ]);
        let hole = ring(&[[2.5, 0.5], [3.5, 0.5], [3.5, 1.5], [2.5, 1.5], [2.5, 0.5]]);
        coords.extend(hole);
        let tris = triangulate_polygon_rings(&coords, &[7, 5]).unwrap();
        let area: f64 = tris
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| cross([p[0][0], p[0][1]], [p[1][0], p[1][1]], [p[2][0], p[2][1]]).abs() * 0.5)
            .sum();
        assert!((area - 11.0).abs() < 1e-10);
        assert!(tris.iter().all(|p| (p[2] - p[0] - p[1]).abs() < 1e-10));
        let mut exterior = coords[..7].to_vec();
        exterior.reverse();
        let mut hole = coords[7..].to_vec();
        hole.reverse();
        exterior.extend(hole);
        assert_eq!(tris, triangulate_polygon_rings(&exterior, &[7, 5]).unwrap());
    }
    #[test]
    fn rejects_invalid_topology_and_unbounded_work() {
        let outer = ring(&[[0., 0.], [4., 0.], [4., 4.], [0., 4.], [0., 0.]]);
        for hole in [
            ring(&[[5., 5.], [6., 5.], [6., 6.], [5., 5.]]),
            ring(&[[0., 0.], [1., 0.], [1., 1.], [0., 0.]]),
        ] {
            let mut coords = outer.clone();
            let n = hole.len();
            coords.extend(hole);
            assert!(triangulate_polygon_rings(&coords, &[5, n]).is_err());
        }
        assert!(triangulate_polygon_rings(
            &ring(&[[0., 0.], [4., 4.], [0., 4.], [4., 0.], [0., 0.]]),
            &[5]
        )
        .is_err());
        assert!(triangulate_polygon_rings(
            &vec![[0.; 3]; MAX_FILL_NODES + 1],
            &[MAX_FILL_NODES + 1]
        )
        .is_err());
        assert!(triangulate_polygon_rings(&outer, &[usize::MAX, 1]).is_err());
    }
}
