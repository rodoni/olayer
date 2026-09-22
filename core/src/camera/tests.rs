use super::*;
use crate::geodesy::coords::LatLon;
use crate::geodesy::ellipsoid::Ellipsoid;
use crate::projections::errors::ProjectionError;
use crate::projections::mercator::WebMercator;
use crate::projections::Projection;

struct StubProjection {
    result: Result<(f64, f64), ProjectionError>,
}

impl Projection for StubProjection {
    fn project(&self, _lla: &LatLon) -> Result<(f64, f64), ProjectionError> {
        self.result
    }

    fn unproject(&self, _x: f64, _y: f64) -> Result<LatLon, ProjectionError> {
        Err(ProjectionError::InvalidInput)
    }
}

fn assert_finite_matrix(matrix: &[f32; 16]) {
    assert!(
        matrix.iter().all(|value| value.is_finite()),
        "matrix: {matrix:?}"
    );
}

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

    let bad_center = CameraState::new(LatLon::new(f64::NAN, 0.0, 0.0), 1.0, 0.0, 1.0, 100_000.0);
    assert_eq!(bad_center.validate(), Err(CameraError::InvalidCenter));

    let bad_attitude =
        CameraState::with_attitude(center, 1.0, f64::INFINITY, 0.0, 0.0, 1.0, 100_000.0);
    assert_eq!(bad_attitude.validate(), Err(CameraError::InvalidAttitude));

    let bad_zoom_nan = CameraState::new(center, f64::NAN, 0.0, 1.0, 100_000.0);
    assert_eq!(bad_zoom_nan.validate(), Err(CameraError::InvalidZoom));
}

#[test]
fn test_camera_2d_view_proj_matrix() {
    let ellipsoid = Ellipsoid::wgs84();
    let wm = WebMercator::new(ellipsoid).unwrap();
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
    let wm = WebMercator::new(ellipsoid).unwrap();
    let center = LatLon::from_degrees(0.0, 0.0, 0.0);

    let camera =
        CameraState::with_attitude(center, 1.0, 0.0, 35.0_f64.to_radians(), 0.0, 1.0, 100_000.0);
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
    assert_eq!(
        CameraError::InvalidCenter.to_string(),
        "Invalid camera center"
    );
}

#[test]
fn test_camera_rejects_non_finite_projection_values() {
    let projection = WebMercator::new(Ellipsoid::wgs84()).unwrap();
    let camera = CameraState::new(
        LatLon::new(0.0, 0.0, 0.0),
        f64::MIN_POSITIVE,
        0.0,
        1.0,
        100_000.0,
    );

    assert_eq!(
        camera.get_2d_view_proj_matrix(&projection),
        Err(CameraError::InvalidProjectionValue {
            name: "viewport width"
        })
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
    let wm = WebMercator::new(ellipsoid).unwrap();
    let center = LatLon::from_degrees(0.0, 0.0, 0.0);

    let camera = CameraState::with_attitude(
        center,
        1.0,
        std::f64::consts::PI / 2.0,
        0.0,
        0.0,
        1.0,
        100_000.0,
    );
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

#[test]
fn test_camera_validation_geographic_boundaries() {
    let valid_south = CameraState::new(
        LatLon::new(-std::f64::consts::FRAC_PI_2, -std::f64::consts::PI, 0.0),
        1.0,
        0.0,
        1.0,
        100_000.0,
    );
    let valid_north = CameraState::new(
        LatLon::new(std::f64::consts::FRAC_PI_2, std::f64::consts::PI, 0.0),
        1.0,
        0.0,
        1.0,
        100_000.0,
    );
    assert_eq!(valid_south.validate(), Ok(()));
    assert_eq!(valid_north.validate(), Ok(()));

    for center in [
        LatLon::new(-std::f64::consts::FRAC_PI_2 - 1e-12, 0.0, 0.0),
        LatLon::new(std::f64::consts::FRAC_PI_2 + 1e-12, 0.0, 0.0),
        LatLon::new(0.0, -std::f64::consts::PI - 1e-12, 0.0),
        LatLon::new(0.0, std::f64::consts::PI + 1e-12, 0.0),
        LatLon::new(0.0, 0.0, f64::NAN),
        LatLon::new(0.0, 0.0, f64::INFINITY),
    ] {
        let camera = CameraState::new(center, 1.0, 0.0, 1.0, 100_000.0);
        assert_eq!(camera.validate(), Err(CameraError::InvalidCenter));
    }
}

#[test]
fn test_camera_validation_rejects_non_finite_attitude_components() {
    let center = LatLon::from_degrees(0.0, 0.0, 0.0);
    for (rotation, pitch, roll) in [
        (f64::NAN, 0.0, 0.0),
        (0.0, f64::NAN, 0.0),
        (0.0, f64::INFINITY, 0.0),
        (0.0, 0.0, f64::NAN),
        (0.0, 0.0, f64::NEG_INFINITY),
    ] {
        let camera = CameraState::with_attitude(center, 1.0, rotation, pitch, roll, 1.0, 100_000.0);
        assert_eq!(camera.validate(), Err(CameraError::InvalidAttitude));
    }
}

#[test]
fn test_camera_validation_rejects_non_finite_scale_components() {
    let center = LatLon::from_degrees(0.0, 0.0, 0.0);
    assert_eq!(
        CameraState::new(center, f64::INFINITY, 0.0, 1.0, 100_000.0).validate(),
        Err(CameraError::InvalidZoom)
    );
    assert_eq!(
        CameraState::new(center, 1.0, 0.0, f64::NAN, 100_000.0).validate(),
        Err(CameraError::InvalidAspectRatio)
    );
    assert_eq!(
        CameraState::new(center, 1.0, 0.0, 1.0, f64::INFINITY).validate(),
        Err(CameraError::InvalidViewportBase)
    );
}

#[test]
fn test_camera_propagates_projection_errors() {
    let center = LatLon::from_degrees(0.0, 0.0, 0.0);
    let camera = CameraState::new(center, 1.0, 0.0, 1.0, 100_000.0);

    for projection_error in [ProjectionError::Singularity, ProjectionError::InvalidInput] {
        let projection = StubProjection {
            result: Err(projection_error),
        };
        assert_eq!(
            camera.get_2d_view_proj_matrix(&projection),
            Err(CameraError::Projection(projection_error))
        );
    }
}

#[test]
fn test_camera_rejects_non_finite_projected_center() {
    let camera = CameraState::new(
        LatLon::from_degrees(0.0, 0.0, 0.0),
        1.0,
        0.0,
        1.0,
        100_000.0,
    );

    for (x, y, name) in [
        (f64::NAN, 0.0, "center x"),
        (0.0, f64::INFINITY, "center y"),
    ] {
        let projection = StubProjection { result: Ok((x, y)) };
        assert_eq!(
            camera.get_2d_view_proj_matrix(&projection),
            Err(CameraError::InvalidProjectionValue { name })
        );
    }
}

#[test]
fn test_camera_matrix_outputs_are_finite() {
    let projection = WebMercator::new(Ellipsoid::wgs84()).unwrap();
    let camera = CameraState::with_attitude(
        LatLon::from_degrees(45.0, 90.0, 1000.0),
        2.0,
        0.4,
        0.35,
        0.1,
        1.6,
        250_000.0,
    );

    assert_finite_matrix(&camera.get_2d_view_proj_matrix(&projection).unwrap());
    assert_finite_matrix(&camera.get_25d_view_proj_matrix(&projection).unwrap());
    assert_finite_matrix(&camera.get_3d_view_proj_matrix().unwrap());
}

#[test]
fn test_camera_25d_attitude_changes_matrix() {
    let projection = WebMercator::new(Ellipsoid::wgs84()).unwrap();
    let center = LatLon::from_degrees(0.0, 0.0, 0.0);
    let nadir = CameraState::new(center, 1.0, 0.0, 1.0, 100_000.0);
    let tilted = CameraState::with_attitude(
        center,
        1.0,
        0.0,
        35.0_f64.to_radians(),
        20.0_f64.to_radians(),
        1.0,
        100_000.0,
    );

    let nadir_matrix = nadir.get_25d_view_proj_matrix(&projection).unwrap();
    let tilted_matrix = tilted.get_25d_view_proj_matrix(&projection).unwrap();
    assert!(nadir_matrix
        .iter()
        .zip(tilted_matrix.iter())
        .any(|(left, right)| (left - right).abs() > 1e-6));
}

#[test]
fn test_camera_error_display_covers_wrapped_errors() {
    assert_eq!(
        CameraError::InvalidAttitude.to_string(),
        "Invalid camera attitude"
    );
    assert_eq!(
        CameraError::InvalidProjectionValue { name: "center x" }.to_string(),
        "Invalid projection value: center x"
    );
    assert_eq!(
        CameraError::Projection(ProjectionError::Singularity).to_string(),
        "Projection error: Projection singularity encountered"
    );
}
