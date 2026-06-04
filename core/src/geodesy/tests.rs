use super::conversions::{ecef_to_lla, lla_to_ecef, lla_to_enu, enu_to_lla};
use super::coords::LatLon;
use super::errors::GeodesyError;
use super::ellipsoid::Ellipsoid;
use super::solvers::{GeodeticSolver, HaversineSolver, VincentySolver};

#[test]
fn test_coordinates_conversion_degrees_radians() {
    let lat_deg = 45.0;
    let lon_deg = 90.0;
    let height = 100.0;

    let coords = LatLon::from_degrees(lat_deg, lon_deg, height);
    assert!((coords.lat - lat_deg.to_radians()).abs() < 1e-12);
    assert!((coords.lon - lon_deg.to_radians()).abs() < 1e-12);
    assert_eq!(coords.height, height);

    let (lat_out, lon_out, height_out) = coords.to_degrees();
    assert!((lat_out - lat_deg).abs() < 1e-12);
    assert!((lon_out - lon_deg).abs() < 1e-12);
    assert_eq!(height_out, height);
}

#[test]
fn test_latlon_validation() {
    let valid = LatLon::from_degrees(45.0, 90.0, 100.0);
    assert_eq!(valid.validate(), Ok(()));

    let invalid_lat = LatLon::from_degrees(95.0, 0.0, 0.0);
    assert!(matches!(invalid_lat.validate(), Err(GeodesyError::LatitudeOutOfRange(_))));

    let invalid_lon = LatLon::from_degrees(0.0, 185.0, 0.0);
    assert!(matches!(invalid_lon.validate(), Err(GeodesyError::LongitudeOutOfRange(_))));
}

#[test]
fn test_lla_ecef_roundtrip() {
    let ellipsoid = Ellipsoid::wgs84();
    
    // Test points: Greenwich, North Pole, South Pole, Equator/Greenwich intersection
    let test_points = vec![
        LatLon::from_degrees(51.4778, -0.0015, 100.0), // Greenwich
        LatLon::from_degrees(90.0, 0.0, 50.0),        // North Pole
        LatLon::from_degrees(-90.0, 45.0, 10.0),       // South Pole
        LatLon::from_degrees(0.0, 0.0, 0.0),           // Equator Prime Meridian
        LatLon::from_degrees(-23.5505, -46.6333, 800.0), // São Paulo
    ];

    for p in test_points {
        let ecef = lla_to_ecef(&p, &ellipsoid);
        let back = ecef_to_lla(&ecef, &ellipsoid);

        let (lat_d, lon_d, h_d) = p.to_degrees();
        let (lat_b, lon_b, h_b) = back.to_degrees();

        // High precision checks
        assert!((lat_d - lat_b).abs() < 1e-9, "Latitude mismatch: {} vs {}", lat_d, lat_b);
        
        // For poles, longitude is singular, so only verify if latitude is 90
        if lat_d.abs() < 89.9999 {
            // Normalise longitude difference to handle wrapping
            let mut diff_lon = (lon_d - lon_b).abs();
            if diff_lon > 180.0 {
                diff_lon = 360.0 - diff_lon;
            }
            assert!(diff_lon < 1e-9, "Longitude mismatch: {} vs {}", lon_d, lon_b);
        }
        
        assert!((h_d - h_b).abs() < 1e-3, "Height mismatch: {} vs {}", h_d, h_b); // millimetric precision
    }
}

#[test]
fn test_lla_enu_roundtrip() {
    let ellipsoid = Ellipsoid::wgs84();
    let origin = LatLon::from_degrees(-23.5505, -46.6333, 800.0); // São Paulo Center
    
    // Nearby point (approx 10km away north-east and 200m up)
    let target = LatLon::from_degrees(-23.4505, -46.5333, 1000.0);
    
    let enu = lla_to_enu(&target, &origin, &ellipsoid);
    let back = enu_to_lla(&enu, &origin, &ellipsoid);

    let (lat_t, lon_t, h_t) = target.to_degrees();
    let (lat_b, lon_b, h_b) = back.to_degrees();

    assert!((lat_t - lat_b).abs() < 1e-9);
    assert!((lon_t - lon_b).abs() < 1e-9);
    assert!((h_t - h_b).abs() < 1e-3);
    
    // Verify displacement values are logical (moving North/East increases coordinates)
    assert!(enu.east > 0.0);
    assert!(enu.north > 0.0);
    
    // The Up component equals the height difference minus the Earth curvature drop
    // over the horizontal ENU distance (≈15.1 km). Drop ≈ d² / (2·a).
    let expected_up = 200.0 - (enu.distance_2d().powi(2) / (2.0 * ellipsoid.a));
    assert!((enu.up - expected_up).abs() < 1.0,
        "ENU up mismatch: expected ~{}, got {}", expected_up, enu.up);
}

#[test]
fn test_haversine_solver() {
    let ellipsoid = Ellipsoid::wgs84();
    let solver = HaversineSolver;

    // JFK to London Heathrow
    let jfk = LatLon::from_degrees(40.639722, -73.778889, 0.0);
    let lhr = LatLon::from_degrees(51.4775, -0.461389, 0.0);

    let result = solver.inverse(&jfk, &lhr, &ellipsoid).unwrap();
    
    // Spherical distance should be around 5560 km for mean radius
    assert!(result.distance > 5_500_000.0 && result.distance < 5_600_000.0, "Haversine distance was {}", result.distance);
    
    // Bearing from NY to London should be northeast (approx 51 degrees)
    let bearing_deg = result.initial_bearing.to_degrees();
    assert!(bearing_deg > 45.0 && bearing_deg < 60.0, "Initial bearing was {}", bearing_deg);

    // Direct solver roundtrip
    let projected = solver.direct(&jfk, result.initial_bearing, result.distance, &ellipsoid).unwrap();
    let (lat_p, lon_p, _) = projected.to_degrees();
    let (lat_l, lon_l, _) = lhr.to_degrees();

    assert!((lat_p - lat_l).abs() < 1e-5);
    assert!((lon_p - lon_l).abs() < 1e-5);
}

#[test]
fn test_vincenty_solver_precision() {
    let ellipsoid = Ellipsoid::wgs84();
    let solver = VincentySolver;

    // Munich to Zurich
    let munich = LatLon::from_degrees(48.137154, 11.576124, 0.0);
    let zurich = LatLon::from_degrees(47.376887, 8.541694, 0.0);

    let result = solver.inverse(&munich, &zurich, &ellipsoid).unwrap();
    
    // Reference distance for Munich to Zurich coordinates is 242682.04 meters on WGS84
    let expected_distance = 242682.04;
    assert!((result.distance - expected_distance).abs() < 1.0, "Vincenty distance delta: {}", (result.distance - expected_distance).abs());

    // Direct solver projection
    let projected = solver.direct(&munich, result.initial_bearing, result.distance, &ellipsoid).unwrap();
    let (lat_p, lon_p, _) = projected.to_degrees();
    let (lat_z, lon_z, _) = zurich.to_degrees();

    // Vincenty Direct should land exactly on Zurich
    assert!((lat_p - lat_z).abs() < 1e-9);
    assert!((lon_p - lon_z).abs() < 1e-9);
}

#[test]
fn test_vincenty_antipodal_fallback() {
    let ellipsoid = Ellipsoid::wgs84();
    let solver = VincentySolver;

    // Antipodal points (Equator intersection with Prime Meridian vs 180th meridian)
    // Vincenty fails to converge for differences of longitude very close to 180 degrees.
    let p1 = LatLon::from_degrees(0.0, 0.0, 0.0);
    let p2 = LatLon::from_degrees(0.0, 180.0, 0.0);

    // This call should fallback to Haversine instead of failing or looping forever
    let result = solver.inverse(&p1, &p2, &ellipsoid);
    assert!(result.is_ok());
    
    let res = result.unwrap();
    // Distance should be approximately half of earth circumference (approx 20,015 km)
    assert!(res.distance > 20_000_000.0 && res.distance < 20_100_000.0, "Distance: {}", res.distance);
}

#[test]
fn test_ecef_lla_poles() {
    let ellipsoid = Ellipsoid::wgs84();
    
    // North Pole — longitude is singular; ecef_to_lla should return lon = 0
    let np = LatLon::from_degrees(90.0, 123.0, 100.0);
    let ecef_np = lla_to_ecef(&np, &ellipsoid);
    let back_np = ecef_to_lla(&ecef_np, &ellipsoid);
    assert!((back_np.lat.to_degrees() - 90.0).abs() < 1e-9);
    assert!(back_np.lon.abs() < 1e-12, "Longitude at North Pole should be 0, got {}", back_np.lon);
    assert!((back_np.height - 100.0).abs() < 1e-3);

    // South Pole
    let sp = LatLon::from_degrees(-90.0, -45.0, 50.0);
    let ecef_sp = lla_to_ecef(&sp, &ellipsoid);
    let back_sp = ecef_to_lla(&ecef_sp, &ellipsoid);
    assert!((back_sp.lat.to_degrees() + 90.0).abs() < 1e-9);
    assert!(back_sp.lon.abs() < 1e-12, "Longitude at South Pole should be 0, got {}", back_sp.lon);
    assert!((back_sp.height - 50.0).abs() < 1e-3);
}

#[test]
fn test_ecef_lla_antimeridian() {
    let ellipsoid = Ellipsoid::wgs84();
    
    let p = LatLon::from_degrees(0.0, 179.999999, 0.0);
    let ecef = lla_to_ecef(&p, &ellipsoid);
    let back = ecef_to_lla(&ecef, &ellipsoid);
    
    assert!((back.lat.to_degrees() - 0.0).abs() < 1e-9);
    assert!((back.lon.to_degrees() - 179.999999).abs() < 1e-9);
    assert!((back.height - 0.0).abs() < 1e-3);
}

#[test]
fn test_ecef_lla_high_altitude() {
    let ellipsoid = Ellipsoid::wgs84();
    
    // Satellite-like altitude
    let sat = LatLon::from_degrees(45.0, 45.0, 400_000.0);
    let ecef = lla_to_ecef(&sat, &ellipsoid);
    let back = ecef_to_lla(&ecef, &ellipsoid);
    
    // Bowring's closed-form method loses a small amount of precision at very high
    // altitudes (satellite orbits). Tolerances are relaxed accordingly.
    assert!((back.lat.to_degrees() - 45.0).abs() < 1e-7);
    assert!((back.lon.to_degrees() - 45.0).abs() < 1e-9);
    assert!((back.height - 400_000.0).abs() < 1e-2);
}

#[test]
fn test_vincenty_coincident_points() {
    let ellipsoid = Ellipsoid::wgs84();
    let solver = VincentySolver;
    
    let p = LatLon::from_degrees(10.0, 20.0, 0.0);
    let result = solver.inverse(&p, &p, &ellipsoid).unwrap();
    
    assert_eq!(result.distance, 0.0);
    assert_eq!(result.initial_bearing, 0.0);
    assert_eq!(result.final_bearing, 0.0);
}

#[test]
fn test_vincenty_sub_meter_roundtrip() {
    let ellipsoid = Ellipsoid::wgs84();
    let solver = VincentySolver;
    
    let p1 = LatLon::from_degrees(0.0, 0.0, 0.0);
    let p2 = LatLon::from_degrees(0.0, 0.000001, 0.0); // ~0.11 meters
    
    let result = solver.inverse(&p1, &p2, &ellipsoid).unwrap();
    let projected = solver.direct(&p1, result.initial_bearing, result.distance, &ellipsoid).unwrap();
    
    assert!((projected.lat - p2.lat).abs() < 1e-12);
    assert!((projected.lon - p2.lon).abs() < 1e-12);
}

#[test]
fn test_solver_metadata() {
    assert_eq!(HaversineSolver::EXPECTED_ACCURACY_METERS, 1.0);
    assert_eq!(VincentySolver::EXPECTED_ACCURACY_METERS, 1e-3);
}
