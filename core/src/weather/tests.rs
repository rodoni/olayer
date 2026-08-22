use crate::geodesy::coords::LatLon;
use crate::weather::errors::WeatherError;
use crate::weather::isoline::{generate_isolines_rad, isolines_to_flat_array_deg};
use crate::weather::radar_palette::{colorize_dbz_grid, dbz_to_rgba, RadarColorPalette};
use crate::weather::sigmet::{SigmetDataset, SigmetFeature, SigmetHazardType, SigmetSeverity};
use crate::weather::wind_barb::{generate_wind_barb, wind_barb_to_flat_lines_deg};

#[test]
fn test_dbz_to_rgba() {
    // Under 5 dBZ is transparent
    assert_eq!(dbz_to_rgba(0.0, RadarColorPalette::Nexrad), [0, 0, 0, 0]);
    assert_eq!(dbz_to_rgba(4.9, RadarColorPalette::Nexrad), [0, 0, 0, 0]);

    // 28 dBZ in Nexrad should be medium green
    let c_28 = dbz_to_rgba(28.0, RadarColorPalette::Nexrad);
    assert_eq!(c_28[0], 1); // R
    assert_eq!(c_28[1], 197); // G
    assert_eq!(c_28[3], 230); // Alpha

    // 32 dBZ in Nexrad should be dark green
    let c_32 = dbz_to_rgba(32.0, RadarColorPalette::Nexrad);
    assert_eq!(c_32[0], 0); // R
    assert_eq!(c_32[1], 142); // G
    assert_eq!(c_32[3], 240); // Alpha

    // 52 dBZ in Nexrad should be red
    let c_52 = dbz_to_rgba(52.0, RadarColorPalette::Nexrad);
    assert_eq!(c_52[0], 253);
    assert_eq!(c_52[1], 0);

    // ICAO palette
    assert_eq!(dbz_to_rgba(15.0, RadarColorPalette::Icao), [0, 0, 0, 0]);
    let c_icao_mod = dbz_to_rgba(30.0, RadarColorPalette::Icao);
    assert_eq!(c_icao_mod, [0, 230, 0, 210]); // Green

    let c_icao_sev = dbz_to_rgba(60.0, RadarColorPalette::Icao);
    assert_eq!(c_icao_sev, [255, 0, 255, 255]); // Magenta
}

#[test]
fn test_colorize_dbz_grid() {
    let grid = vec![0.0, 25.0, 45.0, 65.0];
    let rgba = colorize_dbz_grid(&grid, 2, 2, RadarColorPalette::Nexrad).unwrap();
    assert_eq!(rgba.len(), 16); // 4 pixels * 4 channels

    // Pixel 0 is 0 dBZ (transparent)
    assert_eq!(&rgba[0..4], &[0, 0, 0, 0]);

    // Mismatched grid dimensions error
    let err = colorize_dbz_grid(&grid, 3, 3, RadarColorPalette::Nexrad);
    assert!(matches!(err, Err(WeatherError::InvalidGridDimensions { .. })));
}

#[test]
fn test_wind_barb_calm() {
    let origin = LatLon::from_degrees(51.5, -0.1, 0.0);
    let geom = generate_wind_barb(&origin, 1.5, 0.0, 1000.0, false).unwrap();

    assert!(geom.calm_circle_radius_m.is_some());
    assert_eq!(geom.barbs.len(), 0);
    assert_eq!(geom.pennants.len(), 0);
}

#[test]
fn test_wind_barb_5kt_and_65kt() {
    let origin = LatLon::from_degrees(51.5, -0.1, 0.0);

    // 5 knots: 1 half barb
    let geom_5kt = generate_wind_barb(&origin, 5.0, 0.0, 1000.0, false).unwrap();
    assert_eq!(geom_5kt.pennants.len(), 0);
    assert_eq!(geom_5kt.barbs.len(), 1);

    // 65 knots: 1 pennant (50kt) + 1 full barb (10kt) + 1 half barb (5kt)
    let geom_65kt = generate_wind_barb(&origin, 65.0, std::f64::consts::FRAC_PI_2, 1000.0, false).unwrap();
    assert_eq!(geom_65kt.pennants.len(), 1);
    assert_eq!(geom_65kt.barbs.len(), 2); // 1 full + 1 half = 2 barb segments

    let flat = wind_barb_to_flat_lines_deg(&geom_65kt);
    assert!(flat.len() > 10);
}

#[test]
fn test_wind_barb_southern_hemisphere() {
    let origin = LatLon::from_degrees(-23.5, -46.6, 0.0);
    let geom_nh = generate_wind_barb(&origin, 20.0, 0.0, 1000.0, false).unwrap();
    let geom_sh = generate_wind_barb(&origin, 20.0, 0.0, 1000.0, true).unwrap();

    // End points of barbs in NH and SH should have opposite longitudinal signs
    let barb_nh_end = geom_nh.barbs[0].1;
    let barb_sh_end = geom_sh.barbs[0].1;

    let d_lon_nh = barb_nh_end.lon - origin.lon;
    let d_lon_sh = barb_sh_end.lon - origin.lon;
    assert!(d_lon_nh * d_lon_sh < 0.0);
}

#[test]
fn test_sigmet_feature_and_dataset() {
    let mut dataset = SigmetDataset::new();

    // Define convective hazard polygon around London
    let polygon = vec![
        LatLon::from_degrees(51.0, -1.0, 0.0),
        LatLon::from_degrees(51.0, 1.0, 0.0),
        LatLon::from_degrees(52.0, 1.0, 0.0),
        LatLon::from_degrees(52.0, -1.0, 0.0),
        LatLon::from_degrees(51.0, -1.0, 0.0),
    ];

    dataset.add_feature(SigmetFeature {
        id: "SIGMET_01".to_string(),
        name: "SEVERE CONVECTIVE TS".to_string(),
        hazard_type: SigmetHazardType::Thunderstorm,
        severity: SigmetSeverity::Severe,
        polygon,
        floor_m: Some(1000.0),
        ceiling_m: Some(10000.0),
        valid_from_epoch_s: None,
        valid_until_epoch_s: None,
    });

    assert_eq!(dataset.len(), 1);

    // Inside polygon and within altitude window (FL150 ~ 4500m)
    let hazards_inside = dataset.find_hazards_at_point(
        51.5_f64.to_radians(),
        0.0_f64.to_radians(),
        Some(4500.0),
    );
    assert_eq!(hazards_inside.len(), 1);
    assert_eq!(hazards_inside[0].id, "SIGMET_01");

    // Outside altitude window (500m < floor 1000m)
    let hazards_too_low = dataset.find_hazards_at_point(
        51.5_f64.to_radians(),
        0.0_f64.to_radians(),
        Some(500.0),
    );
    assert_eq!(hazards_too_low.len(), 0);

    // Outside polygon footprint (Paris ~ 48.8, 2.3)
    let hazards_outside = dataset.find_hazards_at_point(
        48.8_f64.to_radians(),
        2.3_f64.to_radians(),
        Some(4500.0),
    );
    assert_eq!(hazards_outside.len(), 0);

    // Test GeoJSON roundtrip
    let geojson = dataset.to_geojson().unwrap();
    assert!(geojson.contains("SIGMET_01"));
    assert!(geojson.contains("THUNDERSTORM"));

    let imported = SigmetDataset::from_geojson(&geojson).unwrap();
    assert_eq!(imported.len(), 1);
    assert_eq!(imported.features[0].id, "SIGMET_01");
}

#[test]
fn test_marching_squares_isolines() {
    // 3x3 scalar grid representing a concentric pressure low / mountain peak:
    // 10  20  10
    // 20  50  20
    // 10  20  10
    let grid = vec![
        10.0, 20.0, 10.0,
        20.0, 50.0, 20.0,
        10.0, 20.0, 10.0,
    ];

    let bounds_rad = (
        0.0_f64.to_radians(),
        0.0_f64.to_radians(),
        2.0_f64.to_radians(),
        2.0_f64.to_radians(),
    );

    // Extract contour line at isovalue 30.0
    let segments = generate_isolines_rad(&grid, 3, 3, bounds_rad, &[30.0]).unwrap();
    assert!(!segments.is_empty());
    for s in &segments {
        assert_eq!(s.isovalue, 30.0);
    }

    let flat = isolines_to_flat_array_deg(&segments);
    assert_eq!(flat.len(), segments.len() * 5);
}
