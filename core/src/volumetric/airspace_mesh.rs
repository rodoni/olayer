use crate::geodesy::conversions::lla_to_ecef;
use crate::geodesy::coords::LatLon;
use crate::geodesy::ellipsoid::Ellipsoid;
use crate::geodesy::local_frame::LocalTangentFrame;
use crate::volumetric::errors::VolumetricError;
use crate::volumetric::triangulation::triangulate_polygon_2d;
use crate::volumetric::types::{VolumetricMesh, VolumetricVertex};

/// Generates an extruded 3D volumetric polygonal mesh (sidewalls, top ceiling cap, and bottom floor cap)
/// from a 2D geodesic polygon footprint and vertical altitude limits.
///
/// # Arguments
/// * `polygon` - Geodesic polygon vertices in radians. Must have at least 3 vertices.
/// * `floor_m` - Lower altitude limit in meters above WGS84 ellipsoid.
/// * `ceiling_m` - Upper altitude limit in meters above WGS84 ellipsoid.
///
/// # Returns
/// An indexed 3D `VolumetricMesh` with outward-pointing surface normals and height ratios.
pub fn generate_airspace_volume_mesh(
    polygon: &[LatLon],
    floor_m: f64,
    ceiling_m: f64,
) -> Result<VolumetricMesh, VolumetricError> {
    let n = polygon.len();
    if n < 3 {
        return Err(VolumetricError::InsufficientVertices {
            expected: 3,
            actual: n,
        });
    }

    if floor_m >= ceiling_m {
        return Err(VolumetricError::InvalidAltitudeBounds { floor_m, ceiling_m });
    }

    let ell = Ellipsoid::wgs84();

    // Compute polygon centroid for local tangent plane projection
    let mut sum_lat = 0.0;
    let mut sum_lon = 0.0;
    for pt in polygon {
        sum_lat += pt.lat;
        sum_lon += pt.lon;
    }
    let centroid = LatLon::new(sum_lat / n as f64, sum_lon / n as f64, 0.0);
    let frame = LocalTangentFrame::new(centroid);

    // Project polygon to 2D local tangent plane (East, North) and compute ECEF caps
    let mut local_2d = Vec::with_capacity(n);
    let mut floor_ecef = Vec::with_capacity(n);
    let mut ceil_ecef = Vec::with_capacity(n);

    for pt in polygon {
        let enu = frame.lla_to_enu(&LatLon::new(pt.lat, pt.lon, 0.0));
        local_2d.push([enu.east_m, enu.north_m]);

        let pt_floor = LatLon::new(pt.lat, pt.lon, floor_m);
        let pt_ceil = LatLon::new(pt.lat, pt.lon, ceiling_m);

        let ecef_f = lla_to_ecef(&pt_floor, &ell);
        let ecef_c = lla_to_ecef(&pt_ceil, &ell);

        floor_ecef.push([ecef_f.x, ecef_f.y, ecef_f.z]);
        ceil_ecef.push([ecef_c.x, ecef_c.y, ecef_c.z]);
    }

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // ========================================================================
    // 1. Sidewall Quads (Extruded Vertical Faces)
    // ========================================================================
    for i in 0..n {
        let next_i = (i + 1) % n;

        let p0_floor = floor_ecef[i];
        let p1_floor = floor_ecef[next_i];
        let p1_ceil = ceil_ecef[next_i];
        let p0_ceil = ceil_ecef[i];

        // Compute outward sidewall normal via cross product: (p1_floor - p0_floor) x (p0_ceil - p0_floor)
        let edge_x = p1_floor[0] - p0_floor[0];
        let edge_y = p1_floor[1] - p0_floor[1];
        let edge_z = p1_floor[2] - p0_floor[2];

        let up_x = p0_ceil[0] - p0_floor[0];
        let up_y = p0_ceil[1] - p0_floor[1];
        let up_z = p0_ceil[2] - p0_floor[2];

        let mut nx = edge_y * up_z - edge_z * up_y;
        let mut ny = edge_z * up_x - edge_x * up_z;
        let mut nz = edge_x * up_y - edge_y * up_x;

        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        if len > 1e-12 {
            nx /= len;
            ny /= len;
            nz /= len;
        } else {
            nx = 0.0;
            ny = 0.0;
            nz = 1.0;
        }
        let normal_wall = [nx as f32, ny as f32, nz as f32];

        let base_idx = vertices.len() as u32;

        // 4 vertices per sidewall segment
        vertices.push(VolumetricVertex {
            position_ecef: p0_floor,
            normal: normal_wall,
            height_ratio: 0.0,
            is_edge: 1.0,
        });
        vertices.push(VolumetricVertex {
            position_ecef: p1_floor,
            normal: normal_wall,
            height_ratio: 0.0,
            is_edge: 1.0,
        });
        vertices.push(VolumetricVertex {
            position_ecef: p1_ceil,
            normal: normal_wall,
            height_ratio: 1.0,
            is_edge: 1.0,
        });
        vertices.push(VolumetricVertex {
            position_ecef: p0_ceil,
            normal: normal_wall,
            height_ratio: 1.0,
            is_edge: 1.0,
        });

        // Two CCW triangles for the quad: (0, 1, 2) and (0, 2, 3)
        indices.push(base_idx);
        indices.push(base_idx + 1);
        indices.push(base_idx + 2);

        indices.push(base_idx);
        indices.push(base_idx + 2);
        indices.push(base_idx + 3);
    }

    // ========================================================================
    // 2. 2D Triangulation for Ceiling (Top) and Floor (Bottom) Caps
    // ========================================================================
    let cap_triangles = triangulate_polygon_2d(&local_2d)?;

    // 2A. Top Ceiling Cap (Normal pointing upwards away from Earth center)
    for tri in &cap_triangles {
        let idx0 = tri[0];
        let idx1 = tri[1];
        let idx2 = tri[2];

        let p0 = ceil_ecef[idx0];
        let p1 = ceil_ecef[idx1];
        let p2 = ceil_ecef[idx2];

        // Upward normal
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let mut nx = e1[1] * e2[2] - e1[2] * e2[1];
        let mut ny = e1[2] * e2[0] - e1[0] * e2[2];
        let mut nz = e1[0] * e2[1] - e1[1] * e2[0];
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        if len > 1e-12 {
            nx /= len;
            ny /= len;
            nz /= len;
        }
        let normal_top = [nx as f32, ny as f32, nz as f32];

        let base_idx = vertices.len() as u32;
        vertices.push(VolumetricVertex {
            position_ecef: p0,
            normal: normal_top,
            height_ratio: 1.0,
            is_edge: 0.0,
        });
        vertices.push(VolumetricVertex {
            position_ecef: p1,
            normal: normal_top,
            height_ratio: 1.0,
            is_edge: 0.0,
        });
        vertices.push(VolumetricVertex {
            position_ecef: p2,
            normal: normal_top,
            height_ratio: 1.0,
            is_edge: 0.0,
        });

        indices.push(base_idx);
        indices.push(base_idx + 1);
        indices.push(base_idx + 2);
    }

    // 2B. Bottom Floor Cap (Normal pointing downwards towards Earth center)
    for tri in &cap_triangles {
        let idx0 = tri[0];
        let idx1 = tri[1];
        let idx2 = tri[2];

        let p0 = floor_ecef[idx0];
        let p1 = floor_ecef[idx1];
        let p2 = floor_ecef[idx2];

        // Downward normal (inverted winding)
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let mut nx = -(e1[1] * e2[2] - e1[2] * e2[1]);
        let mut ny = -(e1[2] * e2[0] - e1[0] * e2[2]);
        let mut nz = -(e1[0] * e2[1] - e1[1] * e2[0]);
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        if len > 1e-12 {
            nx /= len;
            ny /= len;
            nz /= len;
        }
        let normal_bottom = [nx as f32, ny as f32, nz as f32];

        let base_idx = vertices.len() as u32;
        vertices.push(VolumetricVertex {
            position_ecef: p0,
            normal: normal_bottom,
            height_ratio: 0.0,
            is_edge: 0.0,
        });
        vertices.push(VolumetricVertex {
            position_ecef: p2,
            normal: normal_bottom,
            height_ratio: 0.0,
            is_edge: 0.0,
        });
        vertices.push(VolumetricVertex {
            position_ecef: p1,
            normal: normal_bottom,
            height_ratio: 0.0,
            is_edge: 0.0,
        });

        indices.push(base_idx);
        indices.push(base_idx + 1);
        indices.push(base_idx + 2);
    }

    Ok(VolumetricMesh { vertices, indices })
}
