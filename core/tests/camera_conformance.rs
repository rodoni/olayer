//! Shared analytical references also consumed through the real WASM binding in Vitest.
use olayer_core::camera::CameraState;
use olayer_core::geodesy::{coords::LatLon, ellipsoid::Ellipsoid};
use olayer_core::projections::{Projection, Stereographic, WebMercator};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    reference: String,
    matrix_absolute_tolerance: f64,
    matrix_relative_tolerance: f64,
    projection_tolerance_meters: f64,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum ProjectionKind {
    Mercator,
    Stereographic,
}

#[derive(Deserialize)]
enum Mode {
    #[serde(rename = "2d")]
    Flat,
    #[serde(rename = "25d")]
    Tilted,
    #[serde(rename = "3d")]
    Globe,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    projection: ProjectionKind,
    mode: Mode,
    center: [f64; 3],
    attitude: [f64; 3],
    zoom: f64,
    aspect: f64,
    width: f64,
    matrix: [f64; 16],
    points: Vec<Point>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Point {
    lla: [f64; 3],
    xy: [f64; 2],
}

fn coordinates([lat, lon, height]: [f64; 3]) -> LatLon {
    LatLon::new(lat.to_radians(), lon.to_radians(), height)
}

fn close(actual: f64, expected: f64, absolute: f64, relative: f64, context: &str) {
    assert!(
        actual.is_finite() && expected.is_finite(),
        "{context}: nonfinite value"
    );
    let tolerance = absolute + relative * expected.abs();
    assert!(
        (actual - expected).abs() <= tolerance,
        "{context}: actual {actual}, expected {expected}, tolerance {tolerance}"
    );
}

#[test]
fn public_camera_and_projections_match_shared_analytical_references() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("../../tests/fixtures/camera/conformance.json"))
            .expect("valid typed camera fixture");
    assert!(!fixture.reference.is_empty());
    assert!(fixture.cases.len() >= 7);
    for case in fixture.cases {
        let center = coordinates(case.center);
        let projection: Box<dyn Projection> = match case.projection {
            ProjectionKind::Mercator => Box::new(WebMercator::default()),
            ProjectionKind::Stereographic => {
                Box::new(Stereographic::new(center.lat, center.lon, Ellipsoid::wgs84()).unwrap())
            }
        };
        let [heading, pitch, roll] = case.attitude.map(f64::to_radians);
        let camera = CameraState::with_attitude(
            center,
            case.zoom,
            heading,
            pitch,
            roll,
            case.aspect,
            case.width,
        );
        let matrix = match case.mode {
            Mode::Flat => camera.get_2d_view_proj_matrix(projection.as_ref()),
            Mode::Tilted => camera.get_25d_view_proj_matrix(projection.as_ref()),
            Mode::Globe => camera.get_3d_view_proj_matrix(),
        }
        .unwrap_or_else(|error| panic!("{}: {error}", case.name));
        for (index, (actual, expected)) in matrix.into_iter().zip(case.matrix).enumerate() {
            // Do not let the zero-entry noise budget swallow small map scale coefficients.
            let absolute = if expected == 0.0 {
                fixture.matrix_absolute_tolerance
            } else {
                fixture.matrix_absolute_tolerance * expected.abs().min(1.0)
            };
            close(
                f64::from(actual),
                expected,
                absolute,
                fixture.matrix_relative_tolerance,
                &format!("{} matrix[{index}]", case.name),
            );
        }
        for point in case.points {
            let (x, y) = projection.project(&coordinates(point.lla)).unwrap();
            for (actual, expected) in [x, y].into_iter().zip(point.xy) {
                close(
                    actual,
                    expected,
                    fixture.projection_tolerance_meters,
                    0.0,
                    &case.name,
                );
            }
        }
    }
}
