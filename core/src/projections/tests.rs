use crate::geodesy::coords::LatLon;
use crate::geodesy::ellipsoid::Ellipsoid;
use super::lcc::LambertConformalConic;
use super::matrix::Matrix4;
use super::mercator::WebMercator;
use super::stereographic::Stereographic;
use super::{CameraState, Projection, ProjectionError};

fn check_roundtrip(proj: &dyn Projection, points: &[LatLon]) {
    for p in points {
        let (x, y) = proj.project(p).unwrap();
        let back = proj.unproject(x, y).unwrap();

        let (lat_d, lon_d, _) = p.to_degrees();
        let (lat_b, lon_b, _) = back.to_degrees();

        assert!((lat_d - lat_b).abs() < 1e-7, "Latitude mismatch: {} vs {}", lat_d, lat_b);
        assert!((lon_d - lon_b).abs() < 1e-7, "Longitude mismatch: {} vs {}", lon_d, lon_b);
    }
}

#[test]
fn test_lcc_roundtrip() {
    let ellipsoid = Ellipsoid::wgs84();
    let lcc = LambertConformalConic::new(
        33.0_f64.to_radians(),
        45.0_f64.to_radians(),
        0.0_f64.to_radians(),
        -96.0_f64.to_radians(),
        ellipsoid,
    );

    let test_points = vec![
        LatLon::from_degrees(38.8951, -77.0364, 0.0), // Washington DC
        LatLon::from_degrees(40.7128, -74.0060, 0.0), // New York
        LatLon::from_degrees(34.0522, -118.2437, 0.0), // Los Angeles
    ];

    check_roundtrip(&lcc, &test_points);
}

#[test]
fn test_stereographic_roundtrip() {
    let ellipsoid = Ellipsoid::wgs84();
    let stereo = Stereographic::new(
        -23.5505_f64.to_radians(),
        -46.6333_f64.to_radians(),
        ellipsoid,
    );

    let test_points = vec![
        LatLon::from_degrees(-23.5505, -46.6333, 0.0), // Center (São Paulo)
        LatLon::from_degrees(-23.4505, -46.5333, 0.0), // Near
        LatLon::from_degrees(-22.9068, -43.1729, 0.0), // Rio de Janeiro (approx 350km away)
    ];

    check_roundtrip(&stereo, &test_points);
}

#[test]
fn test_web_mercator_roundtrip() {
    let ellipsoid = Ellipsoid::wgs84();
    let wm = WebMercator::new(ellipsoid);

    let test_points = vec![
        LatLon::from_degrees(0.0, 0.0, 0.0),          // Origin
        LatLon::from_degrees(51.4778, -0.0015, 0.0),   // Greenwich
        LatLon::from_degrees(-23.5505, -46.6333, 0.0), // São Paulo
    ];

    check_roundtrip(&wm, &test_points);
}

#[test]
fn test_stereographic_antipodal() {
    let ellipsoid = Ellipsoid::wgs84();
    let stereo = Stereographic::new(0.0, 0.0, ellipsoid);

    // The antipodal point to the center of projection is a singularity
    let antipode = LatLon::from_degrees(0.0, 180.0, 0.0);
    let result = stereo.project(&antipode);
    assert!(matches!(result, Err(ProjectionError::Singularity)));
}

#[test]
fn test_web_mercator_at_limit() {
    let ellipsoid = Ellipsoid::wgs84();
    let wm = WebMercator::new(ellipsoid);

    // Test the exact Web Mercator latitude limit
    let limit = 85.05112878;
    let p = LatLon::from_degrees(limit, 45.0, 0.0);
    let (x, y) = wm.project(&p).unwrap();
    let back = wm.unproject(x, y).unwrap();
    assert!((back.lat.to_degrees() - limit).abs() < 1e-9);
}

#[test]
fn test_camera_state_validation() {
    let ellipsoid = Ellipsoid::wgs84();
    let wm = WebMercator::new(ellipsoid);
    let center = LatLon::from_degrees(0.0, 0.0, 0.0);

    // Valid state should succeed
    let valid = CameraState::new(center, 1.0, 0.0, 1.0, 100_000.0);
    assert_eq!(valid.validate(), Ok(()));

    // zoom = 0 should fail
    let bad_zoom = CameraState::new(center, 0.0, 0.0, 1.0, 100_000.0);
    assert!(matches!(
        wm.get_view_proj_matrix(&bad_zoom),
        Err(ProjectionError::InvalidCameraState)
    ));

    // aspect_ratio = 0 should fail
    let bad_aspect = CameraState::new(center, 1.0, 0.0, 0.0, 100_000.0);
    assert!(matches!(
        wm.get_view_proj_matrix(&bad_aspect),
        Err(ProjectionError::InvalidCameraState)
    ));

    // viewport_base_meters = 0 should fail
    let bad_base = CameraState::new(center, 1.0, 0.0, 1.0, 0.0);
    assert!(matches!(
        wm.get_view_proj_matrix(&bad_base),
        Err(ProjectionError::InvalidCameraState)
    ));
}

#[test]
fn test_view_projection_matrix_with_rotation() {
    let ellipsoid = Ellipsoid::wgs84();
    let wm = WebMercator::new(ellipsoid);
    let center = LatLon::from_degrees(0.0, 0.0, 0.0);
    let center_proj = wm.project(&center).unwrap();

    // 90 degree rotation
    let camera_rot = CameraState::new(
        center,
        1.0,
        std::f64::consts::FRAC_PI_2,
        1.0,
        100_000.0,
    );
    let m_rot_arr = wm.get_view_proj_matrix(&camera_rot).unwrap();

    // Multiply a vector v by column-major matrix m
    let multiply_vector = |v: &[f32; 4]| -> [f32; 4] {
        let mut out = [0.0; 4];
        for row in 0..4 {
            out[row] = m_rot_arr[row] * v[0]
                + m_rot_arr[4 + row] * v[1]
                + m_rot_arr[8 + row] * v[2]
                + m_rot_arr[12 + row] * v[3];
        }
        out
    };

    // Center should still map to NDC (0, 0)
    let v_center = [center_proj.0 as f32, center_proj.1 as f32, 0.0, 1.0];
    let ndc_center = multiply_vector(&v_center);
    assert!(ndc_center[0].abs() < 1e-4);
    assert!(ndc_center[1].abs() < 1e-4);

    // A point 50_000m NORTH of center should now map to NDC x ≈ 1 (rotated to the right)
    let v_north = [center_proj.0 as f32, (center_proj.1 + 50_000.0) as f32, 0.0, 1.0];
    let ndc_north = multiply_vector(&v_north);
    assert!((ndc_north[0] - 1.0).abs() < 1e-4);
    assert!(ndc_north[1].abs() < 1e-4);

    // A point 50_000m EAST of center should now map to NDC y ≈ -1 (rotated downward)
    let v_east = [(center_proj.0 + 50_000.0) as f32, center_proj.1 as f32, 0.0, 1.0];
    let ndc_east = multiply_vector(&v_east);
    assert!(ndc_east[0].abs() < 1e-4);
    assert!((ndc_east[1] + 1.0).abs() < 1e-4);
}

#[test]
fn test_matrix4_default_is_identity() {
    let id = Matrix4::identity();
    let def = Matrix4::default();
    assert_eq!(id, def);
}

#[test]
fn test_matrix4_multiply_correctness() {
    // Test that a translation followed by a rotation produces the expected result.
    let trans = Matrix4::translation(1.0, 2.0, 3.0);
    let rot = Matrix4::rotation_z(std::f32::consts::FRAC_PI_2);

    let combined = rot * trans;
    let m = combined.as_slice();

    // The rotation matrix for PI/2 around Z:
    // [ 0  -1   0   0 ]
    // [ 1   0   0   0 ]
    // [ 0   0   1   0 ]
    // [ 0   0   0   1 ]
    //
    // Multiplying by translation(1,2,3):
    // col 3 becomes rot * [1,2,3,1] = [-2, 1, 3, 1]
    assert!((m[12] - -2.0).abs() < 1e-6, "m[12] expected -2.0, got {}", m[12]);
    assert!((m[13] - 1.0).abs() < 1e-6, "m[13] expected 1.0, got {}", m[13]);
    assert!((m[14] - 3.0).abs() < 1e-6, "m[14] expected 3.0, got {}", m[14]);
    assert!((m[15] - 1.0).abs() < 1e-6, "m[15] expected 1.0, got {}", m[15]);
}

#[test]
fn test_matrix4_mul_by_value() {
    let a = Matrix4::translation(1.0, 0.0, 0.0);
    let b = Matrix4::translation(2.0, 0.0, 0.0);
    let c = a * b;
    let m = c.as_slice();
    // Combined translation should be (3, 0, 0)
    assert!((m[12] - 3.0).abs() < 1e-6);
}

#[test]
fn test_view_projection_matrix() {
    let ellipsoid = Ellipsoid::wgs84();
    let wm = WebMercator::new(ellipsoid);

    // Camera centered at (0, 0)
    let camera = CameraState::new(
        LatLon::from_degrees(0.0, 0.0, 0.0),
        1.0,           // zoom
        0.0,           // rotation
        1.0,           // aspect ratio (square viewport)
        100_000.0,     // viewport base in meters
    );

    let m_arr = wm.get_view_proj_matrix(&camera).unwrap();

    // Multiply a vector v by column-major matrix m
    let multiply_vector = |v: &[f32; 4]| -> [f32; 4] {
        let mut out = [0.0; 4];
        for row in 0..4 {
            out[row] = m_arr[row] * v[0]
                + m_arr[4 + row] * v[1]
                + m_arr[8 + row] * v[2]
                + m_arr[12 + row] * v[3];
        }
        out
    };

    // Point exactly at the center of the camera should map to NDC center (0, 0)
    let center_proj = wm.project(&camera.center).unwrap();
    let v_center = [center_proj.0 as f32, center_proj.1 as f32, 0.0, 1.0];
    let ndc_center = multiply_vector(&v_center);
    assert!(ndc_center[0].abs() < 1e-4);
    assert!(ndc_center[1].abs() < 1e-4);

    // Point on the right edge of viewport (dx = 50,000 meters) should map to NDC x = 1.0
    let v_right = [(center_proj.0 + 50_000.0) as f32, center_proj.1 as f32, 0.0, 1.0];
    let ndc_right = multiply_vector(&v_right);
    assert!((ndc_right[0] - 1.0).abs() < 1e-4);
    assert!(ndc_right[1].abs() < 1e-4);

    // Point on the top edge of viewport (dy = 50,000 meters) should map to NDC y = 1.0
    let v_top = [center_proj.0 as f32, (center_proj.1 + 50_000.0) as f32, 0.0, 1.0];
    let ndc_top = multiply_vector(&v_top);
    assert!(ndc_top[0].abs() < 1e-4);
    assert!((ndc_top[1] - 1.0).abs() < 1e-4);
}

#[test]
fn test_stereographic_update_center() {
    let ellipsoid = Ellipsoid::wgs84();
    let mut stereo = Stereographic::new(0.0, 0.0, ellipsoid);

    // Initial center projects to (0, 0)
    let origin = LatLon::new(0.0, 0.0, 0.0);
    let (x, y) = stereo.project(&origin).unwrap();
    assert!(x.abs() < 1e-6);
    assert!(y.abs() < 1e-6);

    // Update center to a new location (e.g. São Paulo)
    let sp_lat = -23.5505_f64.to_radians();
    let sp_lon = -46.6333_f64.to_radians();
    stereo.update_center(sp_lat, sp_lon);

    // Old center (0, 0) should now NOT project to (0, 0)
    let (x_old, y_old) = stereo.project(&origin).unwrap();
    assert!(x_old.abs() > 1000.0 || y_old.abs() > 1000.0);

    // New center should now project to (0, 0)
    let sp_origin = LatLon::new(sp_lat, sp_lon, 0.0);
    let (x_new, y_new) = stereo.project(&sp_origin).unwrap();
    assert!(x_new.abs() < 1e-6);
    assert!(y_new.abs() < 1e-6);

    // Verify roundtrip for some points in the new projection
    let test_points = vec![
        LatLon::from_degrees(-23.5505, -46.6333, 0.0), // Center
        LatLon::from_degrees(-23.4505, -46.5333, 0.0), // Near
    ];
    check_roundtrip(&stereo, &test_points);
}

#[test]
fn test_lcc_update_center() {
    let ellipsoid = Ellipsoid::wgs84();
    let mut lcc = LambertConformalConic::new(
        33.0_f64.to_radians(),
        45.0_f64.to_radians(),
        0.0_f64.to_radians(),
        -96.0_f64.to_radians(),
        ellipsoid,
    );

    // Initial center projects to (0, rho_0 - rho_0) = (0, 0)
    let origin = LatLon::new(0.0_f64.to_radians(), -96.0_f64.to_radians(), 0.0);
    let (x, y) = lcc.project(&origin).unwrap();
    assert!(x.abs() < 1e-6);
    assert!(y.abs() < 1e-6);

    // Update center to São Paulo
    let sp_lat = -23.5505_f64.to_radians();
    let sp_lon = -46.6333_f64.to_radians();
    lcc.update_center(sp_lat, sp_lon);

    // Old center should no longer project to (0, 0)
    let (x_old, y_old) = lcc.project(&origin).unwrap();
    assert!(x_old.abs() > 1000.0 || y_old.abs() > 1000.0);

    // New center should project to (0, 0)
    let sp_origin = LatLon::new(sp_lat, sp_lon, 0.0);
    let (x_new, y_new) = lcc.project(&sp_origin).unwrap();
    assert!(x_new.abs() < 1e-6);
    assert!(y_new.abs() < 1e-6);

    // Roundtrip should still work after moving the center
    let test_points = vec![
        LatLon::from_degrees(-23.5505, -46.6333, 0.0),
        LatLon::from_degrees(-23.4505, -46.5333, 0.0),
    ];
    check_roundtrip(&lcc, &test_points);
}
