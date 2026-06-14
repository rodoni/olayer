use super::*;
use crate::geodesy::coords::LatLon;
use crate::geodesy::ellipsoid::Ellipsoid;
use crate::projections::mercator::WebMercator;

#[test]
fn test_camera_state_validation() {
    let center = LatLon::from_degrees(0.0, 0.0, 0.0);

    // Valid state should succeed
    let valid = CameraState::new(center, 1.0, 0.0, 1.0, 100_000.0);
    assert_eq!(valid.validate(), Ok(()));

    // zoom = 0 should fail
    let bad_zoom = CameraState::new(center, 0.0, 0.0, 1.0, 100_000.0);
    assert_eq!(bad_zoom.validate(), Err(CameraError::InvalidZoom));

    // aspect_ratio = 0 should fail
    let bad_aspect = CameraState::new(center, 1.0, 0.0, 0.0, 100_000.0);
    assert_eq!(bad_aspect.validate(), Err(CameraError::InvalidAspectRatio));

    // viewport_base_meters = 0 should fail
    let bad_base = CameraState::new(center, 1.0, 0.0, 1.0, 0.0);
    assert_eq!(bad_base.validate(), Err(CameraError::InvalidViewportBase));
}

#[test]
fn test_camera_2d_view_proj_matrix() {
    let ellipsoid = Ellipsoid::wgs84();
    let wm = WebMercator::new(ellipsoid);
    let center = LatLon::from_degrees(0.0, 0.0, 0.0);

    let camera = CameraState::new(center, 1.0, 0.0, 1.0, 100_000.0);
    let matrix = camera.get_2d_view_proj_matrix(&wm);
    assert!(matrix.is_ok());

    let m = matrix.unwrap();
    // Center point projection (0,0,0,1) * VP
    let cx = m[12];
    let cy = m[13];
    assert!(cx.abs() < 1e-4);
    assert!(cy.abs() < 1e-4);
}

#[test]
fn test_camera_25d_view_proj_matrix() {
    let ellipsoid = Ellipsoid::wgs84();
    let wm = WebMercator::new(ellipsoid);
    let center = LatLon::from_degrees(0.0, 0.0, 0.0);

    let camera = CameraState::with_attitude(center, 1.0, 0.0, 35.0_f64.to_radians(), 0.0, 1.0, 100_000.0);
    let matrix = camera.get_25d_view_proj_matrix(&wm);
    assert!(matrix.is_ok());

    let m = matrix.unwrap();
    // Verify it is a valid 4x4 matrix
    assert!(m[15].abs() > 0.0);
}

#[test]
fn test_camera_3d_view_proj_matrix() {
    let center = LatLon::from_degrees(-23.5505, -46.6333, 0.0);
    let camera = CameraState::with_attitude(center, 1.0, 0.0, 0.0, 0.0, 1.33, 100_000.0);
    let matrix = camera.get_3d_view_proj_matrix();
    assert!(matrix.is_ok());

    let m = matrix.unwrap();
    assert!(m[15].abs() > 0.0);
}

#[test]
fn test_camera_error_display() {
    assert_eq!(
        CameraError::InvalidZoom.to_string(),
        "Invalid camera state: zoom must be greater than zero"
    );
    assert_eq!(
        CameraError::InvalidAspectRatio.to_string(),
        "Invalid camera state: aspect ratio must be greater than zero"
    );
    assert_eq!(
        CameraError::InvalidViewportBase.to_string(),
        "Invalid camera state: viewport base meters must be greater than zero"
    );
}

#[test]
fn test_camera_with_attitude() {
    let center = LatLon::from_degrees(45.0, 90.0, 1000.0);
    let cam = CameraState::with_attitude(center, 2.0, 0.5, 0.35, 0.1, 1.6, 250000.0);
    assert_eq!(cam.center, center);
    assert_eq!(cam.zoom, 2.0);
    assert_eq!(cam.rotation, 0.5);
    assert_eq!(cam.pitch, 0.35);
    assert_eq!(cam.roll, 0.1);
    assert_eq!(cam.aspect_ratio, 1.6);
    assert_eq!(cam.viewport_base_meters, 250000.0);
}

#[test]
fn test_camera_2d_with_rotation() {
    let ellipsoid = Ellipsoid::wgs84();
    let wm = WebMercator::new(ellipsoid);
    let center = LatLon::from_degrees(0.0, 0.0, 0.0);

    let camera = CameraState::with_attitude(center, 1.0, std::f64::consts::PI / 2.0, 0.0, 0.0, 1.0, 100_000.0);
    let matrix = camera.get_2d_view_proj_matrix(&wm);
    assert!(matrix.is_ok());
    let m = matrix.unwrap();
    // With 90° rotation, the matrix should still be valid (non-zero determinant)
    assert!(m[15].abs() > 0.0);
}

#[test]
fn test_camera_3d_at_pole() {
    let center = LatLon::from_degrees(90.0, 0.0, 0.0);
    let camera = CameraState::with_attitude(center, 1.0, 0.0, 0.0, 0.0, 1.0, 100_000.0);
    let matrix = camera.get_3d_view_proj_matrix();
    assert!(matrix.is_ok());
    let m = matrix.unwrap();
    assert!(m[15].abs() > 0.0);
}

#[test]
fn test_camera_3d_negative_zoom_fails() {
    let center = LatLon::from_degrees(0.0, 0.0, 0.0);
    let camera = CameraState::with_attitude(center, -1.0, 0.0, 0.0, 0.0, 1.0, 100_000.0);
    let matrix = camera.get_3d_view_proj_matrix();
    assert!(matrix.is_err());
}

#[test]
fn test_camera_validation_negative_zoom() {
    let center = LatLon::from_degrees(0.0, 0.0, 0.0);
    let cam = CameraState::new(center, -1.0, 0.0, 1.0, 100_000.0);
    assert_eq!(cam.validate(), Err(CameraError::InvalidZoom));
}

#[test]
fn test_camera_validation_negative_aspect() {
    let center = LatLon::from_degrees(0.0, 0.0, 0.0);
    let cam = CameraState::new(center, 1.0, 0.0, -1.0, 100_000.0);
    assert_eq!(cam.validate(), Err(CameraError::InvalidAspectRatio));
}

#[test]
fn test_camera_validation_negative_viewport() {
    let center = LatLon::from_degrees(0.0, 0.0, 0.0);
    let cam = CameraState::new(center, 1.0, 0.0, 1.0, -100_000.0);
    assert_eq!(cam.validate(), Err(CameraError::InvalidViewportBase));
}
