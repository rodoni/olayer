use crate::geodesy::conversions::lla_to_ecef;
use crate::geodesy::coords::LatLon;
use crate::geodesy::ellipsoid::Ellipsoid;
use crate::volumetric::errors::VolumetricError;
use crate::volumetric::types::{RibbonMesh, RibbonVertex};

/// Generates a continuous 3D flight trajectory ribbon mesh in ECEF coordinates.
///
/// # Arguments
/// * `waypoints` - Sequential flight trajectory points `LatLon`. Must have at least 2 points.
/// * `ribbon_width_m` - Lateral width of the ribbon in meters. Must be > 0.
/// * `scalar_values` - Optional custom scalar values per waypoint for shader color gradient mapping. If `None`, automatically computes normalized altitude ratio $[0.0, 1.0]$.
///
/// # Returns
/// An indexed `RibbonMesh` with lateral coordinates `u in [0, 1]`, along-track distance `v`, and normals.
pub fn generate_trajectory_ribbon_mesh(
    waypoints: &[LatLon],
    ribbon_width_m: f64,
    scalar_values: Option<&[f64]>,
) -> Result<RibbonMesh, VolumetricError> {
    let n = waypoints.len();
    if n < 2 {
        return Err(VolumetricError::InsufficientVertices {
            expected: 2,
            actual: n,
        });
    }

    if ribbon_width_m <= 0.0 || !ribbon_width_m.is_finite() {
        return Err(VolumetricError::InvalidRibbonParameters(format!(
            "Ribbon width must be positive: {ribbon_width_m} m"
        )));
    }

    if let Some(scalars) = scalar_values {
        if scalars.len() != n {
            return Err(VolumetricError::InvalidRibbonParameters(format!(
                "Scalar values count ({}) does not match waypoints count ({})",
                scalars.len(),
                n
            )));
        }
    }

    let ell = Ellipsoid::wgs84();

    // Convert waypoints to ECEF Cartesian positions
    let mut ecef_points = Vec::with_capacity(n);
    let mut min_alt = f64::INFINITY;
    let mut max_alt = f64::NEG_INFINITY;

    for pt in waypoints {
        let ecef = lla_to_ecef(pt, &ell);
        ecef_points.push([ecef.x, ecef.y, ecef.z]);
        if pt.height < min_alt { min_alt = pt.height; }
        if pt.height > max_alt { max_alt = pt.height; }
    }

    let alt_span = if (max_alt - min_alt).abs() > 1e-3 {
        max_alt - min_alt
    } else {
        1.0
    };

    let half_width = ribbon_width_m * 0.5;
    let mut vertices = Vec::with_capacity(n * 2);
    let mut cumulative_dist = 0.0;

    // Helper vector math
    let normalize3 = |v: [f64; 3]| -> [f64; 3] {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if len > 1e-12 {
            [v[0] / len, v[1] / len, v[2] / len]
        } else {
            [0.0, 0.0, 1.0]
        }
    };

    let cross3 = |a: [f64; 3], b: [f64; 3]| -> [f64; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };

    for i in 0..n {
        let p_curr = ecef_points[i];

        if i > 0 {
            let p_prev = ecef_points[i - 1];
            let d = ((p_curr[0] - p_prev[0]).powi(2) + (p_curr[1] - p_prev[1]).powi(2) + (p_curr[2] - p_prev[2]).powi(2)).sqrt();
            cumulative_dist += d;
        }

        // Local Up normal (away from Earth center)
        let up = normalize3(p_curr);

        // Tangent along track
        let tangent = if i == 0 {
            let p_next = ecef_points[1];
            normalize3([p_next[0] - p_curr[0], p_next[1] - p_curr[1], p_next[2] - p_curr[2]])
        } else if i == n - 1 {
            let p_prev = ecef_points[n - 2];
            normalize3([p_curr[0] - p_prev[0], p_curr[1] - p_prev[1], p_curr[2] - p_prev[2]])
        } else {
            let p_prev = ecef_points[i - 1];
            let p_next = ecef_points[i + 1];
            let t_in = normalize3([p_curr[0] - p_prev[0], p_curr[1] - p_prev[1], p_curr[2] - p_prev[2]]);
            let t_out = normalize3([p_next[0] - p_curr[0], p_next[1] - p_curr[1], p_next[2] - p_curr[2]]);
            normalize3([t_in[0] + t_out[0], t_in[1] + t_out[1], t_in[2] + t_out[2]])
        };

        // Lateral Right vector: T x Up
        let right = normalize3(cross3(tangent, up));

        // Surface normal for the ribbon: Right x Tangent
        let ribbon_norm = normalize3(cross3(right, tangent));
        let norm_f32 = [ribbon_norm[0] as f32, ribbon_norm[1] as f32, ribbon_norm[2] as f32];

        // Scalar value for color gradient
        let scalar_val = if let Some(scalars) = scalar_values {
            scalars[i] as f32
        } else {
            ((waypoints[i].height - min_alt) / alt_span).clamp(0.0, 1.0) as f32
        };

        let v_coord = cumulative_dist as f32;

        // Left vertex (u = 0.0)
        let p_left = [
            p_curr[0] - right[0] * half_width,
            p_curr[1] - right[1] * half_width,
            p_curr[2] - right[2] * half_width,
        ];
        vertices.push(RibbonVertex {
            position_ecef: p_left,
            normal: norm_f32,
            uv: [0.0, v_coord],
            scalar: scalar_val,
        });

        // Right vertex (u = 1.0)
        let p_right = [
            p_curr[0] + right[0] * half_width,
            p_curr[1] + right[1] * half_width,
            p_curr[2] + right[2] * half_width,
        ];
        vertices.push(RibbonVertex {
            position_ecef: p_right,
            normal: norm_f32,
            uv: [1.0, v_coord],
            scalar: scalar_val,
        });
    }

    // Indices connecting segments: 2 triangles per quad
    let mut indices = Vec::with_capacity((n - 1) * 6);
    for i in 0..(n - 1) {
        let v_left0 = (i * 2) as u32;
        let v_right0 = (i * 2 + 1) as u32;
        let v_left1 = ((i + 1) * 2) as u32;
        let v_right1 = ((i + 1) * 2 + 1) as u32;

        // Triangle 1: (Left0, Right0, Right1)
        indices.push(v_left0);
        indices.push(v_right0);
        indices.push(v_right1);

        // Triangle 2: (Left0, Right1, Left1)
        indices.push(v_left0);
        indices.push(v_right1);
        indices.push(v_left1);
    }

    Ok(RibbonMesh { vertices, indices })
}
