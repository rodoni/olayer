use crate::geodesy::coords::LatLon;
use crate::geodesy::ellipsoid::Ellipsoid;
use crate::geodesy::math::{normalize_bearing, normalize_longitude};
use crate::geodesy::solvers::{GeodeticSolver, VincentySolver};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Result of a cross-track error and along-track distance calculation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RouteDeviation {
    /// Cross-track error in meters. Positive indicates the point is to the RIGHT of the track,
    /// negative indicates the point is to the LEFT of the track.
    pub cross_track_error_meters: f64,
    /// Along-track distance in meters from the segment start point to the projected point on track.
    /// Can be negative if the position is behind the segment start.
    pub along_track_distance_meters: f64,
    /// The coordinates of the nearest orthogonal projection point on the geodesic route.
    pub nearest_point_on_route: LatLon,
}

/// Computes the cross-track error (XTK) and along-track distance (ATD) of a position
/// relative to a geodesic route segment from `segment_start` to `segment_end`.
///
/// # Sign Convention
/// * **Cross-Track Error (XTK):** Positive = Right of track, Negative = Left of track.
/// * **Along-Track Distance (ATD):** Positive = Ahead of start point, Negative = Behind start point.
pub fn compute_route_deviation(segment_start: &LatLon, segment_end: &LatLon, pos: &LatLon) -> RouteDeviation {
    let ell = Ellipsoid::wgs84();
    let solver = VincentySolver;
    let earth_radius = ell.a;

    // Segment bearing theta12 and distance
    let seg_res = solver.inverse(segment_start, segment_end, &ell).unwrap_or_else(|_| {
        let (p1, p2) = (to_unit_vector(segment_start), to_unit_vector(segment_end));
        let dist = angle_between(&p1, &p2) * earth_radius;
        crate::geodesy::solvers::GeodeticResult::new(dist, 0.0, 0.0)
    });
    let theta12 = seg_res.initial_bearing;

    // Bearing and distance from start to position pos
    let pos_res = solver.inverse(segment_start, pos, &ell).unwrap_or_else(|_| {
        let (p1, p3) = (to_unit_vector(segment_start), to_unit_vector(pos));
        let dist = angle_between(&p1, &p3) * earth_radius;
        crate::geodesy::solvers::GeodeticResult::new(dist, 0.0, 0.0)
    });
    let d13 = pos_res.distance;
    let theta13 = pos_res.initial_bearing;

    // Angular distance delta13 = d13 / R
    let delta13 = d13 / earth_radius;
    let angle_diff = theta13 - theta12;

    // Spherical cross-track angular offset: sin(delta_xt) = sin(delta13) * sin(theta13 - theta12)
    let sin_delta_xt = delta13.sin() * angle_diff.sin();
    let delta_xt = sin_delta_xt.clamp(-1.0, 1.0).asin();
    let xtk_meters = delta_xt * earth_radius;

    // Spherical along-track angular distance: cos(delta_at) = cos(delta13) / cos(delta_xt)
    let cos_delta_xt = delta_xt.cos();
    let cos_delta_at = if cos_delta_xt.abs() > 1e-12 {
        (delta13.cos() / cos_delta_xt).clamp(-1.0, 1.0)
    } else {
        1.0
    };

    let delta_at_mag = cos_delta_at.acos();
    let atd_sign = if angle_diff.cos() >= 0.0 { 1.0 } else { -1.0 };
    let atd_meters = atd_sign * delta_at_mag * earth_radius;

    // Projected nearest point on the route segment
    let nearest_point = if atd_meters >= 0.0 {
        solver.direct(segment_start, theta12, atd_meters, &ell).unwrap_or(*segment_start)
    } else {
        // Project backwards along reciprocal bearing
        let back_bearing = normalize_bearing(theta12 + PI);
        solver.direct(segment_start, back_bearing, -atd_meters, &ell).unwrap_or(*segment_start)
    };

    RouteDeviation {
        cross_track_error_meters: xtk_meters,
        along_track_distance_meters: atd_meters,
        nearest_point_on_route: nearest_point,
    }
}

/// Computes the intersection coordinate of two geodesic segments $(P_1 \rightarrow P_2)$ and $(P_3 \rightarrow P_4)$,
/// if an intersection exists within both segment boundaries.
pub fn geodesic_intersection(p1: &LatLon, p2: &LatLon, p3: &LatLon, p4: &LatLon) -> Option<LatLon> {
    let v1 = to_unit_vector(p1);
    let v2 = to_unit_vector(p2);
    let v3 = to_unit_vector(p3);
    let v4 = to_unit_vector(p4);

    // Normal vectors to the great circle planes
    let n1 = cross_product(&v1, &v2);
    let n2 = cross_product(&v3, &v4);

    let len_n1 = norm(&n1);
    let len_n2 = norm(&n2);
    if len_n1 < 1e-12 || len_n2 < 1e-12 {
        return None; // Degenerate segment
    }

    // Line of intersection of the two planes
    let line = cross_product(&n1, &n2);
    let len_line = norm(&line);
    if len_line < 1e-12 {
        return None; // Collinear or parallel great circles
    }

    let i1 = [line[0] / len_line, line[1] / len_line, line[2] / len_line];
    let i2 = [-i1[0], -i1[1], -i1[2]];

    if is_on_segment(&i1, &v1, &v2) && is_on_segment(&i1, &v3, &v4) {
        Some(from_unit_vector(&i1))
    } else if is_on_segment(&i2, &v1, &v2) && is_on_segment(&i2, &v3, &v4) {
        Some(from_unit_vector(&i2))
    } else {
        None
    }
}

/// A geodesic polygon on the WGS84 Earth representing airspaces, FIR sectors, or geofences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeodesicPolygon {
    pub vertices: Vec<LatLon>,
}

impl GeodesicPolygon {
    /// Creates a new `GeodesicPolygon` with the given vertex list.
    pub fn new(vertices: Vec<LatLon>) -> Self {
        Self { vertices }
    }

    /// Evaluates whether a point is contained inside this polygon using the
    /// **Spherical Winding Number** algorithm.
    ///
    /// This method is exact on the sphere and free of coordinate singularities
    /// at the antimeridian ($180^\circ / -180^\circ$) and both poles.
    pub fn contains_point(&self, point: &LatLon) -> bool {
        let n = self.vertices.len();
        if n < 3 {
            return false;
        }

        let p = to_unit_vector(point);
        let mut total_winding_rad = 0.0;

        for i in 0..n {
            let next_idx = (i + 1) % n;
            let v1 = to_unit_vector(&self.vertices[i]);
            let v2 = to_unit_vector(&self.vertices[next_idx]);

            // Tangent vectors to the sphere at p pointing toward v1 and v2
            let dot1 = dot_product(&v1, &p);
            let dot2 = dot_product(&v2, &p);

            let t1 = [v1[0] - dot1 * p[0], v1[1] - dot1 * p[1], v1[2] - dot1 * p[2]];
            let t2 = [v2[0] - dot2 * p[0], v2[1] - dot2 * p[1], v2[2] - dot2 * p[2]];

            let len1 = norm(&t1);
            let len2 = norm(&t2);

            // Point is directly on one of the vertices
            if len1 < 1e-12 || len2 < 1e-12 {
                return true;
            }

            let u1 = [t1[0] / len1, t1[1] / len1, t1[2] / len1];
            let u2 = [t2[0] / len2, t2[1] / len2, t2[2] / len2];

            let cross_u = cross_product(&u1, &u2);
            let sin_dtheta = dot_product(&cross_u, &p);
            let cos_dtheta = dot_product(&u1, &u2);

            let dtheta = sin_dtheta.atan2(cos_dtheta);
            total_winding_rad += dtheta;
        }

        let winding_number = total_winding_rad / (2.0 * PI);
        winding_number.abs() > 0.5
    }

    /// Computes the minimum geodesic distance in meters from a point to the polygon boundary.
    pub fn distance_to_boundary(&self, point: &LatLon) -> f64 {
        let n = self.vertices.len();
        if n == 0 {
            return f64::INFINITY;
        }
        if n == 1 {
            let solver = VincentySolver;
            let ell = Ellipsoid::wgs84();
            return solver.inverse(point, &self.vertices[0], &ell)
                .map(|r| r.distance)
                .unwrap_or(0.0);
        }

        let mut min_distance = f64::INFINITY;
        let solver = VincentySolver;
        let ell = Ellipsoid::wgs84();

        for i in 0..n {
            let v1 = &self.vertices[i];
            let v2 = &self.vertices[(i + 1) % n];

            let dev = compute_route_deviation(v1, v2, point);
            let seg_len = solver.inverse(v1, v2, &ell)
                .map(|r| r.distance)
                .unwrap_or(0.0);

            let edge_dist = if dev.along_track_distance_meters >= 0.0 && dev.along_track_distance_meters <= seg_len {
                dev.cross_track_error_meters.abs()
            } else if dev.along_track_distance_meters < 0.0 {
                solver.inverse(point, v1, &ell).map(|r| r.distance).unwrap_or(0.0)
            } else {
                solver.inverse(point, v2, &ell).map(|r| r.distance).unwrap_or(0.0)
            };

            if edge_dist < min_distance {
                min_distance = edge_dist;
            }
        }

        min_distance
    }

    /// Generates a constant-width geodesic buffer polygon around this polygon.
    ///
    /// # Arguments
    /// * `radius_meters` - Width of the buffer corridor in meters (positive = outward expansion).
    /// * `num_segments` - Number of chord segments used to discretize rounded corners at vertices.
    pub fn generate_buffer(&self, radius_meters: f64, num_segments: usize) -> GeodesicPolygon {
        let n = self.vertices.len();
        if n < 3 || radius_meters.abs() < 1e-3 {
            return self.clone();
        }

        let ell = Ellipsoid::wgs84();
        let solver = VincentySolver;
        let segs = num_segments.max(1);

        // Determine polygon orientation via spherical excess / winding
        let is_ccw = self.is_counter_clockwise();
        let offset_angle = if is_ccw { PI / 2.0 } else { -PI / 2.0 };

        let mut buffered_vertices = Vec::new();

        for i in 0..n {
            let prev_idx = if i == 0 { n - 1 } else { i - 1 };
            let next_idx = (i + 1) % n;

            let curr = &self.vertices[i];
            let next = &self.vertices[next_idx];

            let leg_in_res = solver.inverse(&self.vertices[prev_idx], curr, &ell).unwrap_or_else(|_| {
                crate::geodesy::solvers::GeodeticResult::new(0.0, 0.0, 0.0)
            });
            let leg_out_res = solver.inverse(curr, next, &ell).unwrap_or_else(|_| {
                crate::geodesy::solvers::GeodeticResult::new(0.0, 0.0, 0.0)
            });

            let normal_in = normalize_bearing(leg_in_res.final_bearing + offset_angle);
            let normal_out = normalize_bearing(leg_out_res.initial_bearing + offset_angle);

            // Generate rounded corner arc from normal_in to normal_out
            let mut angle_sweep = normal_out - normal_in;
            if is_ccw && angle_sweep < 0.0 {
                angle_sweep += 2.0 * PI;
            } else if !is_ccw && angle_sweep > 0.0 {
                angle_sweep -= 2.0 * PI;
            }

            for s in 0..=segs {
                let frac = s as f64 / segs as f64;
                let bearing = normalize_bearing(normal_in + frac * angle_sweep);
                if let Ok(pt) = solver.direct(curr, bearing, radius_meters, &ell) {
                    buffered_vertices.push(pt);
                }
            }
        }

        GeodesicPolygon {
            vertices: buffered_vertices,
        }
    }

    /// Determines if polygon vertices are ordered counter-clockwise on the sphere.
    fn is_counter_clockwise(&self) -> bool {
        let n = self.vertices.len();
        if n < 3 {
            return true;
        }
        let mut total_angle = 0.0;
        for i in 0..n {
            let v1 = to_unit_vector(&self.vertices[i]);
            let v2 = to_unit_vector(&self.vertices[(i + 1) % n]);
            let v3 = to_unit_vector(&self.vertices[(i + 2) % n]);

            let n1 = cross_product(&v1, &v2);
            let n2 = cross_product(&v2, &v3);

            let len1 = norm(&n1);
            let len2 = norm(&n2);
            if len1 > 1e-12 && len2 > 1e-12 {
                let u1 = [n1[0] / len1, n1[1] / len1, n1[2] / len1];
                let u2 = [n2[0] / len2, n2[1] / len2, n2[2] / len2];
                let cross = cross_product(&u1, &u2);
                let sin_a = dot_product(&cross, &v2);
                let cos_a = dot_product(&u1, &u2);
                total_angle += sin_a.atan2(cos_a);
            }
        }
        total_angle >= 0.0
    }
}

// --- 3D Vector Math Helpers for Spherical Geometry ---

#[inline]
fn to_unit_vector(lla: &LatLon) -> [f64; 3] {
    let cos_lat = lla.lat.cos();
    [
        cos_lat * lla.lon.cos(),
        cos_lat * lla.lon.sin(),
        lla.lat.sin(),
    ]
}

#[inline]
fn from_unit_vector(v: &[f64; 3]) -> LatLon {
    let lat = v[2].clamp(-1.0, 1.0).asin();
    let lon = normalize_longitude(v[1].atan2(v[0]));
    LatLon::new(lat, lon, 0.0)
}

#[inline]
fn dot_product(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn cross_product(a: &[f64; 3], b: &[f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
fn norm(v: &[f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

#[inline]
fn angle_between(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    dot_product(a, b).clamp(-1.0, 1.0).acos()
}

#[inline]
fn is_on_segment(pt: &[f64; 3], start: &[f64; 3], end: &[f64; 3]) -> bool {
    let total_angle = angle_between(start, end);
    let d1 = angle_between(start, pt);
    let d2 = angle_between(pt, end);
    (d1 + d2 - total_angle).abs() < 1e-7
}
