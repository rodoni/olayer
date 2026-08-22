use crate::geodesy::coords::LatLon;
use crate::volumetric::airspace_mesh::generate_airspace_volume_mesh;
use crate::volumetric::errors::VolumetricError;
use crate::volumetric::ribbon_mesh::generate_trajectory_ribbon_mesh;
use crate::volumetric::triangulation::{signed_area_2d, triangulate_polygon_2d};

#[test]
fn test_signed_area_and_triangulation() {
    // CCW Square: (0,0) -> (1,0) -> (1,1) -> (0,1)
    let square_ccw = vec![
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    ];
    let area_ccw = signed_area_2d(&square_ccw);
    assert!((area_ccw - 1.0).abs() < 1e-6);

    let tris = triangulate_polygon_2d(&square_ccw).unwrap();
    assert_eq!(tris.len(), 2); // N - 2 = 2 triangles for a quad

    // Concave L-shaped polygon (6 vertices)
    // (0,0) -> (2,0) -> (2,1) -> (1,1) -> (1,2) -> (0,2)
    let l_shape = vec![
        [0.0, 0.0],
        [2.0, 0.0],
        [2.0, 1.0],
        [1.0, 1.0],
        [1.0, 2.0],
        [0.0, 2.0],
    ];
    let l_tris = triangulate_polygon_2d(&l_shape).unwrap();
    assert_eq!(l_tris.len(), 4); // 6 - 2 = 4 triangles
}

#[test]
fn test_airspace_volume_mesh_generation() {
    let polygon = vec![
        LatLon::from_degrees(51.0, -0.5, 0.0),
        LatLon::from_degrees(51.0, 0.5, 0.0),
        LatLon::from_degrees(51.5, 0.5, 0.0),
        LatLon::from_degrees(51.5, -0.5, 0.0),
    ];

    let floor_m = 1000.0;
    let ceiling_m = 5000.0;

    let mesh = generate_airspace_volume_mesh(&polygon, floor_m, ceiling_m).unwrap();

    // 4 sidewalls * 4 vertices = 16 vertices
    // + 2 top cap triangles * 3 vertices = 6 vertices
    // + 2 bottom cap triangles * 3 vertices = 6 vertices
    // Total = 28 vertices
    assert_eq!(mesh.vertices.len(), 28);

    // 4 sidewalls * 2 triangles * 3 indices = 24 indices
    // + 2 top triangles * 3 = 6
    // + 2 bottom triangles * 3 = 6
    // Total = 36 indices
    assert_eq!(mesh.indices.len(), 36);

    // Verify flat f32 serialization (8 floats per vertex)
    let flat = mesh.to_flat_f32_vertices();
    assert_eq!(flat.len(), mesh.vertices.len() * 8);

    // Verify error when floor >= ceiling
    let err_alt = generate_airspace_volume_mesh(&polygon, 5000.0, 1000.0);
    assert!(matches!(err_alt, Err(VolumetricError::InvalidAltitudeBounds { .. })));

    // Verify error on < 3 vertices
    let err_vert = generate_airspace_volume_mesh(&polygon[0..2], 1000.0, 5000.0);
    assert!(matches!(err_vert, Err(VolumetricError::InsufficientVertices { .. })));
}

#[test]
fn test_trajectory_ribbon_mesh_generation() {
    let waypoints = vec![
        LatLon::from_degrees(40.0, -74.0, 1000.0),
        LatLon::from_degrees(40.5, -73.5, 5000.0),
        LatLon::from_degrees(41.0, -73.0, 10000.0),
        LatLon::from_degrees(41.5, -72.5, 12000.0),
    ];

    let ribbon_width_m = 500.0;
    let ribbon = generate_trajectory_ribbon_mesh(&waypoints, ribbon_width_m, None).unwrap();

    // 4 waypoints * 2 (left/right) = 8 vertices
    assert_eq!(ribbon.vertices.len(), 8);

    // 3 segments * 2 triangles * 3 indices = 18 indices
    assert_eq!(ribbon.indices.len(), 18);

    // Verify scalar gradient (first waypoint alt 1000m should be 0.0, last waypoint 12000m should be 1.0)
    assert_eq!(ribbon.vertices[0].scalar, 0.0);
    assert_eq!(ribbon.vertices[1].scalar, 0.0);
    assert_eq!(ribbon.vertices[6].scalar, 1.0);
    assert_eq!(ribbon.vertices[7].scalar, 1.0);

    // Flat f32 serialization (9 floats per vertex)
    let flat = ribbon.to_flat_f32_vertices();
    assert_eq!(flat.len(), ribbon.vertices.len() * 9);

    // Custom scalars test
    let custom_scalars = vec![0.1, 0.4, 0.8, 1.0];
    let custom_ribbon = generate_trajectory_ribbon_mesh(&waypoints, ribbon_width_m, Some(&custom_scalars)).unwrap();
    assert_eq!(custom_ribbon.vertices[0].scalar, 0.1);
    assert_eq!(custom_ribbon.vertices[2].scalar, 0.4);

    // Error on negative ribbon width
    let err_width = generate_trajectory_ribbon_mesh(&waypoints, -10.0, None);
    assert!(matches!(err_width, Err(VolumetricError::InvalidRibbonParameters(_))));
}
