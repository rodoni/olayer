use super::conversions::{ecef_to_lla, lla_to_ecef, lla_to_enu, enu_to_lla};
use super::coords::LatLon;
use super::errors::GeodesyError;
use super::ellipsoid::Ellipsoid;
use super::local_frame::{EnuPoint, LocalTangentFrame, NedPoint, STANDARD_RADAR_K_FACTOR};
use super::magnetic::MagneticModel;
use super::solvers::{GeodeticSolver, HaversineSolver, VincentySolver};
use super::spatial::{compute_route_deviation, geodesic_intersection, GeodesicPolygon};

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

// ============================================================================
// GIS-PROP-001 Tests: Local Tangent Frame (ENU / NED) & WMM-2025
// ============================================================================

#[test]
fn test_enu_ned_points_conversions() {
    let enu = EnuPoint::new(100.0, 200.0, 300.0);
    assert!((enu.distance_2d() - (100.0_f64.hypot(200.0))).abs() < 1e-9);
    assert!((enu.distance_3d() - (100.0_f64.hypot(200.0).hypot(300.0))).abs() < 1e-9);

    let ned = enu.to_ned();
    assert_eq!(ned.north_m, 200.0);
    assert_eq!(ned.east_m, 100.0);
    assert_eq!(ned.down_m, -300.0);

    let back_enu = ned.to_enu();
    assert_eq!(back_enu, enu);

    let ned2 = NedPoint::new(50.0, -75.0, 120.0);
    let enu2 = ned2.to_enu();
    assert_eq!(enu2.north_m, 50.0);
    assert_eq!(enu2.east_m, -75.0);
    assert_eq!(enu2.up_m, -120.0);
    assert_eq!(NedPoint::from_enu(&enu2), ned2);
}

#[test]
fn test_local_tangent_frame_transforms() {
    // London Heathrow Airport ARP (51.4775 N, 0.4614 W)
    let lhr = LatLon::from_degrees(51.4775, -0.4614, 25.0);
    let frame = LocalTangentFrame::new(lhr);

    // Origin itself should convert to (0, 0, 0)
    let enu_origin = frame.lla_to_enu(&lhr);
    assert!(enu_origin.east_m.abs() < 1e-6);
    assert!(enu_origin.north_m.abs() < 1e-6);
    assert!(enu_origin.up_m.abs() < 1e-6);

    // Target ~10 km East, ~5 km North, +500 m altitude
    let target = LatLon::from_degrees(51.5225, -0.3174, 525.0);
    let enu = frame.lla_to_enu(&target);
    assert!(enu.east_m > 9000.0 && enu.east_m < 11000.0);
    assert!(enu.north_m > 4000.0 && enu.north_m < 6000.0);

    // Round-trip ENU -> LLA
    let back_lla = frame.enu_to_lla(&enu);
    let (t_lat, t_lon, t_h) = target.to_degrees();
    let (b_lat, b_lon, b_h) = back_lla.to_degrees();
    assert!((t_lat - b_lat).abs() < 1e-9);
    assert!((t_lon - b_lon).abs() < 1e-9);
    assert!((t_h - b_h).abs() < 1e-3);

    // NED round-trip
    let ned = frame.lla_to_ned(&target);
    assert_eq!(ned.north_m, enu.north_m);
    assert_eq!(ned.east_m, enu.east_m);
    assert_eq!(ned.down_m, -enu.up_m);
    let back_lla_ned = frame.ned_to_lla(&ned);
    assert!((t_lat - back_lla_ned.lat.to_degrees()).abs() < 1e-9);
}

#[test]
fn test_radar_look_angles_and_refraction() {
    let radar_pos = LatLon::from_degrees(40.0, -75.0, 100.0);
    let frame = LocalTangentFrame::new(radar_pos);

    // Target directly North and above
    let target_north = LatLon::from_degrees(40.09, -75.0, 2100.0); // ~10 km North, 2000m higher
    let (slant, azimuth, elevation) = frame.radar_look_angles(&target_north);

    assert!(slant > 10000.0);
    // Azimuth should be ~0 rad (True North)
    assert!(!(0.05..=2.0 * std::f64::consts::PI - 0.05).contains(&azimuth));
    // Elevation should be positive (looking up)
    assert!(elevation > 0.1 && elevation < 0.3);

    // Test with standard 4/3 tropospheric refraction
    let (slant_ref, az_ref, elev_ref) = frame.radar_look_angles_refracted(&target_north, STANDARD_RADAR_K_FACTOR);
    assert_eq!(slant_ref, slant);
    assert_eq!(az_ref, azimuth);
    // Refracted apparent elevation should be slightly lower due to downward beam curvature
    assert!(elev_ref < elevation);
}

#[test]
fn test_world_magnetic_model_2025() {
    let model = MagneticModel::wmm2025();
    assert_eq!(model.epoch(), 2025.0);
    assert_eq!(model.model_name(), "WMM-2025");
    assert_eq!(model.max_degree(), 12);

    // 1. London (UK): 51.5 N, -0.1 W
    let london = LatLon::from_degrees(51.5, -0.1, 0.0);
    let elements_london = model.get_magnetic_elements(&london, 2025.0);
    let dec_london_deg = elements_london.declination_deg();
    println!("London Elements: X={:.1} nT, Y={:.1} nT, Z={:.1} nT, H={:.1} nT, F={:.1} nT, Declination={:.4} deg",
        elements_london.x_nt, elements_london.y_nt, elements_london.z_nt,
        elements_london.horizontal_intensity_nt, elements_london.total_intensity_nt, dec_london_deg);
    assert!(dec_london_deg > -1.0 && dec_london_deg < 5.0, "London declination: {dec_london_deg}");

    // 2. New York (JFK): 40.64 N, -73.78 W -> Declination ~ -12.5 to -13.5 deg West in 2025
    let jfk = LatLon::from_degrees(40.64, -73.78, 0.0);
    let dec_jfk = model.get_declination(&jfk, 2025.0);
    let dec_jfk_deg = dec_jfk.to_degrees();
    assert!(dec_jfk_deg > -15.0 && dec_jfk_deg < -10.0, "JFK declination: {dec_jfk_deg}");

    // 3. São Paulo (GRU): -23.43 S, -46.47 W -> Declination ~ -21.0 to -23.0 deg West in 2025
    let gru = LatLon::from_degrees(-23.43, -46.47, 750.0);
    let dec_gru = model.get_declination(&gru, 2025.0);
    let dec_gru_deg = dec_gru.to_degrees();
    assert!(dec_gru_deg > -25.0 && dec_gru_deg < -19.0, "GRU declination: {dec_gru_deg}");

    // 4. Tokyo (HND): 35.55 N, 139.78 E -> Declination ~ -7.5 to -9.0 deg West in 2025
    let hnd = LatLon::from_degrees(35.55, 139.78, 0.0);
    let dec_hnd = model.get_declination(&hnd, 2025.0);
    let dec_hnd_deg = dec_hnd.to_degrees();
    assert!(dec_hnd_deg > -11.0 && dec_hnd_deg < -6.0, "HND declination: {dec_hnd_deg}");

    // Bearing transformations roundtrip
    let true_bearing = 45.0_f64.to_radians();
    let mag_bearing = model.true_to_magnetic(true_bearing, &jfk, 2025.0);
    let back_true = model.magnetic_to_true(mag_bearing, &jfk, 2025.0);
    assert!((true_bearing - back_true).abs() < 1e-10);

    // Magnetic elements sanity check
    let elements = model.get_magnetic_elements(&jfk, 2025.0);
    assert!(elements.total_intensity_nt > 45000.0 && elements.total_intensity_nt < 55000.0);
    assert!(elements.horizontal_intensity_nt > 15000.0 && elements.horizontal_intensity_nt < 25000.0);
    assert_eq!(elements.declination_rad, dec_jfk);

    // Static convenience methods match instance methods
    assert_eq!(MagneticModel::get_default_declination(&jfk, 2025.0), dec_jfk);
}

fn generate_wmm_normal_cof_text() -> String {
    let mut s = String::from("    2025.0            WMM-2025        11/20/2024\n");
    for entry in &super::magnetic::WMM2025_COEFFS {
        s.push_str(&format!(
            "{:3} {:3} {:10.1} {:10.1} {:10.1} {:10.1}\n",
            entry.n, entry.m, entry.g, entry.h, entry.g_dot, entry.h_dot
        ));
    }
    s.push_str("999999999999999999999999999999999999999999999999\n");
    s
}

fn generate_wmmhr_highres_cof_text(max_degree: usize) -> String {
    let mut s = String::from("    2025.0            WMMHR-2025      11/20/2024\n");
    // Degree 1..12: core model coefficients
    for entry in &super::magnetic::WMM2025_COEFFS {
        s.push_str(&format!(
            "{:3} {:3} {:10.1} {:10.1} {:10.1} {:10.1}\n",
            entry.n, entry.m, entry.g, entry.h, entry.g_dot, entry.h_dot
        ));
    }
    // Degree 13..max_degree: high resolution crustal harmonics (degree up to e.g. 133)
    for n in 13..=max_degree {
        for m in 0..=n {
            let g = 50.0 / ((n * n) as f64) * (m as f64).cos();
            let h = if m == 0 { 0.0 } else { 50.0 / ((n * n) as f64) * (m as f64).sin() };
            s.push_str(&format!(
                "{:3} {:3} {:10.4} {:10.4}        0.0        0.0\n",
                n, m, g, h
            ));
        }
    }
    s.push_str("999999999999999999999999999999999999999999999999\n");
    s
}

#[test]
fn test_wmm_normal_resolution_cof_load_and_evaluate() {
    let cof_text = generate_wmm_normal_cof_text();

    // 1. Test string parser
    let model_from_str = MagneticModel::from_cof_str(&cof_text).unwrap();
    assert_eq!(model_from_str.epoch(), 2025.0);
    assert_eq!(model_from_str.model_name(), "WMM-2025");
    assert_eq!(model_from_str.release_date(), "11/20/2024");
    assert_eq!(model_from_str.max_degree(), 12);
    assert_eq!(model_from_str.coefficients().entries.len(), 90);

    // 2. Test file loader
    let temp_file_path = std::env::temp_dir().join(format!("test_wmm_normal_{}.cof", std::process::id()));
    std::fs::write(&temp_file_path, &cof_text).unwrap();

    let model_from_file = MagneticModel::from_cof_file(&temp_file_path).unwrap();
    let _ = std::fs::remove_file(&temp_file_path);

    assert_eq!(model_from_file.epoch(), 2025.0);
    assert_eq!(model_from_file.model_name(), "WMM-2025");
    assert_eq!(model_from_file.max_degree(), 12);
    assert_eq!(model_from_file.coefficients().entries.len(), 90);

    // 3. Verify equality with built-in WMM-2025 across global test points
    let built_in = MagneticModel::wmm2025();
    let test_points = [
        LatLon::from_degrees(51.5, -0.1, 0.0),      // London
        LatLon::from_degrees(40.64, -73.78, 0.0),   // New York JFK
        LatLon::from_degrees(-23.43, -46.47, 750.0), // São Paulo GRU
        LatLon::from_degrees(35.55, 139.78, 0.0),   // Tokyo HND
        LatLon::from_degrees(-33.86, 151.20, 10.0), // Sydney
        LatLon::from_degrees(90.0, 0.0, 0.0),       // North Pole
        LatLon::from_degrees(-90.0, 0.0, 0.0),      // South Pole
        LatLon::from_degrees(0.0, 0.0, 0.0),        // Equator Prime Meridian
    ];

    for pt in &test_points {
        let el_builtin = built_in.get_magnetic_elements(pt, 2025.0);
        let el_file = model_from_file.get_magnetic_elements(pt, 2025.0);
        let el_str = model_from_str.get_magnetic_elements(pt, 2025.0);

        assert!((el_builtin.declination_rad - el_file.declination_rad).abs() < 1e-9);
        assert!((el_builtin.inclination_rad - el_file.inclination_rad).abs() < 1e-9);
        assert!((el_builtin.horizontal_intensity_nt - el_file.horizontal_intensity_nt).abs() < 1e-6);
        assert!((el_builtin.total_intensity_nt - el_file.total_intensity_nt).abs() < 1e-6);
        assert!((el_builtin.x_nt - el_file.x_nt).abs() < 1e-6);
        assert!((el_builtin.y_nt - el_file.y_nt).abs() < 1e-6);
        assert!((el_builtin.z_nt - el_file.z_nt).abs() < 1e-6);

        assert_eq!(el_file, el_str);

        // Verify bearing conversions
        let true_bearing = 1.2345;
        let mag_b = model_from_file.true_to_magnetic(true_bearing, pt, 2025.0);
        let back_tb = model_from_file.magnetic_to_true(mag_b, pt, 2025.0);
        assert!((true_bearing - back_tb).abs() < 1e-10);
    }
}

#[test]
fn test_wmm_high_resolution_cof_load_and_evaluate() {
    // Degree 133 WMMHR standard model (9,044 coefficients)
    let high_res_degree = 133;
    let expected_coeff_count = 9044; // sum_{n=1}^{133} (n + 1) = 9044
    let cof_text = generate_wmmhr_highres_cof_text(high_res_degree);

    // 1. Test string parser
    let model_from_str = MagneticModel::from_cof_str(&cof_text).unwrap();
    assert_eq!(model_from_str.epoch(), 2025.0);
    assert_eq!(model_from_str.model_name(), "WMMHR-2025");
    assert_eq!(model_from_str.release_date(), "11/20/2024");
    assert_eq!(model_from_str.max_degree(), high_res_degree);
    assert_eq!(model_from_str.coefficients().entries.len(), expected_coeff_count);

    // 2. Test file loader
    let temp_file_path = std::env::temp_dir().join(format!("test_wmmhr_highres_{}.cof", std::process::id()));
    std::fs::write(&temp_file_path, &cof_text).unwrap();

    let model_from_file = MagneticModel::from_cof_file(&temp_file_path).unwrap();
    let _ = std::fs::remove_file(&temp_file_path);

    assert_eq!(model_from_file.epoch(), 2025.0);
    assert_eq!(model_from_file.model_name(), "WMMHR-2025");
    assert_eq!(model_from_file.max_degree(), high_res_degree);
    assert_eq!(model_from_file.coefficients().entries.len(), expected_coeff_count);

    // 3. Evaluate high resolution spherical harmonic expansion
    let test_points = [
        LatLon::from_degrees(51.5, -0.1, 0.0),      // London
        LatLon::from_degrees(40.64, -73.78, 0.0),   // New York JFK
        LatLon::from_degrees(-23.43, -46.47, 750.0), // São Paulo GRU
        LatLon::from_degrees(35.55, 139.78, 0.0),   // Tokyo HND
        LatLon::from_degrees(0.0, 0.0, 0.0),        // Equator Prime Meridian
    ];

    for pt in &test_points {
        let el = model_from_file.get_magnetic_elements(pt, 2026.5);

        // Verify valid geomagnetic ranges
        assert!(el.total_intensity_nt > 20000.0 && el.total_intensity_nt < 70000.0,
            "Total intensity {} nT out of reasonable Earth bounds", el.total_intensity_nt);
        assert!(el.horizontal_intensity_nt > 10000.0 && el.horizontal_intensity_nt < 45000.0,
            "Horizontal intensity {} nT out of bounds", el.horizontal_intensity_nt);

        // Vector magnitude consistency: H = sqrt(X^2 + Y^2), F = sqrt(H^2 + Z^2)
        let computed_h = el.x_nt.hypot(el.y_nt);
        let computed_f = computed_h.hypot(el.z_nt);
        assert!((el.horizontal_intensity_nt - computed_h).abs() < 1e-6);
        assert!((el.total_intensity_nt - computed_f).abs() < 1e-6);

        // Angular bounds
        assert!(el.declination_rad >= -std::f64::consts::PI && el.declination_rad <= std::f64::consts::PI);
        assert!(el.inclination_rad >= -std::f64::consts::FRAC_PI_2 && el.inclination_rad <= std::f64::consts::FRAC_PI_2);

        // Bearing transformations roundtrip
        let true_bearing = 45.0_f64.to_radians();
        let mag_bearing = model_from_file.true_to_magnetic(true_bearing, pt, 2026.5);
        let back_true = model_from_file.magnetic_to_true(mag_bearing, pt, 2026.5);
        assert!((true_bearing - back_true).abs() < 1e-10);
    }
}

#[test]
fn test_dynamic_magnetic_model_from_cof_parsing() {
    let mock_cof = r#"
    # Test WMM COF format with comments and whitespace
    2030.0            WMM-2030        11/20/2029
      1   0  -29396.6       0.0      11.6       0.0
      1   1   -1404.9    4589.6      12.3     -23.4
      2   0   -2499.7       0.0     -12.4       0.0
      2   1    2997.5   -3013.9       0.8     -20.6
      2   2    1686.2    -664.1      -2.1     -20.1
    999999999999999999999999999999999999999999999999
    "#;

    let model = MagneticModel::from_cof_str(mock_cof).unwrap();
    assert_eq!(model.epoch(), 2030.0);
    assert_eq!(model.model_name(), "WMM-2030");
    assert_eq!(model.release_date(), "11/20/2029");
    assert_eq!(model.max_degree(), 2);
    assert_eq!(model.coefficients().entries.len(), 5);

    let jfk = LatLon::from_degrees(40.64, -73.78, 0.0);
    let dec = model.get_declination(&jfk, 2030.0);
    assert!(dec.to_degrees() < 0.0); // West declination
}

#[test]
fn test_dynamic_magnetic_model_error_handling() {
    // Empty COF
    assert!(MagneticModel::from_cof_str("").is_err());
    assert!(MagneticModel::from_cof_str("# only comments").is_err());

    // Invalid epoch
    assert!(MagneticModel::from_cof_str("NOT_AN_EPOCH WMM-2025").is_err());

    // Incomplete line
    let bad_line = "2025.0 WMM-2025 11/20/2024\n1 0 -29396.6";
    assert!(MagneticModel::from_cof_str(bad_line).is_err());

    // Order m > degree n
    let bad_order = "2025.0 WMM-2025 11/20/2024\n1 2 -29396.6 0.0 0.0 0.0";
    assert!(MagneticModel::from_cof_str(bad_order).is_err());

    // Non-existent file path for from_cof_file
    let missing_file_err = MagneticModel::from_cof_file("this_path_does_not_exist_at_all_wmm.cof");
    assert!(matches!(missing_file_err, Err(GeodesyError::MagneticModelError(_))));
}

// ============================================================================
// GIS-PROP-002 Tests: Geodesic Spatial Analysis Engine
// ============================================================================

#[test]
fn test_cross_track_error_and_along_track_distance() {
    let ell = Ellipsoid::wgs84();
    let solver = VincentySolver;

    // Segment from Equator/Prime Meridian Eastward along Equator (0N, 0E -> 0N, 10E)
    let start = LatLon::from_degrees(0.0, 0.0, 0.0);
    let end = LatLon::from_degrees(0.0, 10.0, 0.0);

    // Position 1: Exactly on the track at 0N, 5E
    let pos_on_track = LatLon::from_degrees(0.0, 5.0, 0.0);
    let dev1 = compute_route_deviation(&start, &end, &pos_on_track);
    assert!(dev1.cross_track_error_meters.abs() < 1.0);
    let expected_atd = solver.inverse(&start, &pos_on_track, &ell).unwrap().distance;
    assert!((dev1.along_track_distance_meters - expected_atd).abs() < 5.0);

    // Position 2: Displaced South (Right of Eastbound track) -> Positive XTK
    // ~1 degree South of Equator (approx 111 km)
    let pos_right = LatLon::from_degrees(-1.0, 5.0, 0.0);
    let dev2 = compute_route_deviation(&start, &end, &pos_right);
    assert!(dev2.cross_track_error_meters > 110_000.0 && dev2.cross_track_error_meters < 112_000.0);

    // Position 3: Displaced North (Left of Eastbound track) -> Negative XTK
    let pos_left = LatLon::from_degrees(1.0, 5.0, 0.0);
    let dev3 = compute_route_deviation(&start, &end, &pos_left);
    assert!(dev3.cross_track_error_meters < -110_000.0 && dev3.cross_track_error_meters > -112_000.0);

    // Position 4: Displaced behind start (e.g. 0N, -1E) -> Negative ATD
    let pos_behind = LatLon::from_degrees(0.0, -1.0, 0.0);
    let dev4 = compute_route_deviation(&start, &end, &pos_behind);
    assert!(dev4.along_track_distance_meters < -100_000.0);
}

#[test]
fn test_geodesic_intersection() {
    // Equator segment: (0N, -10E) -> (0N, 10E)
    let seg1_start = LatLon::from_degrees(0.0, -10.0, 0.0);
    let seg1_end = LatLon::from_degrees(0.0, 10.0, 0.0);

    // Prime Meridian segment: (-10N, 0E) -> (10N, 0E)
    let seg2_start = LatLon::from_degrees(-10.0, 0.0, 0.0);
    let seg2_end = LatLon::from_degrees(10.0, 0.0, 0.0);

    let intersection = geodesic_intersection(&seg1_start, &seg1_end, &seg2_start, &seg2_end);
    assert!(intersection.is_some());
    let inter_pt = intersection.unwrap();
    let (lat_d, lon_d, _) = inter_pt.to_degrees();
    assert!(lat_d.abs() < 1e-6);
    assert!(lon_d.abs() < 1e-6);

    // Disjoint parallel segments (no intersection)
    let seg3_start = LatLon::from_degrees(20.0, -10.0, 0.0);
    let seg3_end = LatLon::from_degrees(20.0, 10.0, 0.0);
    let no_inter = geodesic_intersection(&seg1_start, &seg1_end, &seg3_start, &seg3_end);
    assert!(no_inter.is_none());
}

#[test]
fn test_geodesic_polygon_containment_and_antimeridian() {
    // 1. Standard Continental Polygon (São Paulo state bounding area)
    let sp_sector = GeodesicPolygon::new(vec![
        LatLon::from_degrees(-20.0, -50.0, 0.0),
        LatLon::from_degrees(-20.0, -44.0, 0.0),
        LatLon::from_degrees(-25.0, -44.0, 0.0),
        LatLon::from_degrees(-25.0, -50.0, 0.0),
    ]);

    let inside_pt = LatLon::from_degrees(-23.55, -46.63, 0.0); // São Paulo city
    let outside_pt = LatLon::from_degrees(-15.0, -47.0, 0.0);  // Brasília (North)

    assert!(sp_sector.contains_point(&inside_pt));
    assert!(!sp_sector.contains_point(&outside_pt));

    // Distance to boundary check
    let dist_boundary = sp_sector.distance_to_boundary(&inside_pt);
    assert!(dist_boundary > 100_000.0 && dist_boundary < 300_000.0);

    // 2. Antimeridian Spanning Polygon (Fiji / Pacific region: spans +170E to -170W = 190E)
    let pacific_sector = GeodesicPolygon::new(vec![
        LatLon::from_degrees(-10.0, 170.0, 0.0),
        LatLon::from_degrees(-10.0, -170.0, 0.0),
        LatLon::from_degrees(-25.0, -170.0, 0.0),
        LatLon::from_degrees(-25.0, 170.0, 0.0),
    ]);

    // Fiji (~18S, 178E) and Tonga (~21S, -175W) are inside
    let fiji = LatLon::from_degrees(-18.0, 178.0, 0.0);
    let tonga = LatLon::from_degrees(-21.0, -175.0, 0.0);
    let sydney = LatLon::from_degrees(-33.86, 151.20, 0.0); // Outside

    assert!(pacific_sector.contains_point(&fiji), "Fiji should be contained in Pacific sector");
    assert!(pacific_sector.contains_point(&tonga), "Tonga should be contained in Pacific sector");
    assert!(!pacific_sector.contains_point(&sydney), "Sydney should NOT be contained");

    // 3. North Polar Polygon
    let polar_sector = GeodesicPolygon::new(vec![
        LatLon::from_degrees(80.0, -180.0, 0.0),
        LatLon::from_degrees(80.0, -90.0, 0.0),
        LatLon::from_degrees(80.0, 0.0, 0.0),
        LatLon::from_degrees(80.0, 90.0, 0.0),
    ]);
    let north_pole = LatLon::from_degrees(90.0, 0.0, 0.0);
    let equator_pt = LatLon::from_degrees(0.0, 0.0, 0.0);
    assert!(polar_sector.contains_point(&north_pole));
    assert!(!polar_sector.contains_point(&equator_pt));
}

#[test]
fn test_geodesic_polygon_buffering() {
    let polygon = GeodesicPolygon::new(vec![
        LatLon::from_degrees(0.0, 0.0, 0.0),
        LatLon::from_degrees(0.0, 2.0, 0.0),
        LatLon::from_degrees(2.0, 2.0, 0.0),
        LatLon::from_degrees(2.0, 0.0, 0.0),
    ]);

    let buffer_radius = 50_000.0; // 50 km buffer
    let buffered = polygon.generate_buffer(buffer_radius, 4);

    // Buffered polygon must have more vertices due to rounded arcs
    assert!(buffered.vertices.len() > polygon.vertices.len());

    // Center point is inside both
    let center = LatLon::from_degrees(1.0, 1.0, 0.0);
    assert!(polygon.contains_point(&center));
    assert!(buffered.contains_point(&center));

    // A point 20km outside original boundary should be INSIDE the 50km buffered polygon
    let near_outside = LatLon::from_degrees(2.15, 1.0, 0.0); // ~17 km North of 2.0N
    assert!(!polygon.contains_point(&near_outside));
    assert!(buffered.contains_point(&near_outside));

    // A point 100km outside should be OUTSIDE both
    let far_outside = LatLon::from_degrees(3.5, 1.0, 0.0);
    assert!(!polygon.contains_point(&far_outside));
    assert!(!buffered.contains_point(&far_outside));
}
