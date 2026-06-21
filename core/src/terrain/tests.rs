use crate::geodesy::coords::LatLon;
use crate::terrain::errors::TerrainError;
use crate::terrain::engine::{TerrainEngine, TileKey};

fn create_mock_dted0(origin_lat: &str, origin_lon: &str, num_cols: usize, num_rows: usize) -> Vec<u8> {
    let mut data = vec![b' '; 3428];

    // UHL Sentinel
    data[0..4].copy_from_slice(b"UHL1");

    // Lon origin (e.g. 0480000W)
    let lon_bytes = format!("{: <8}", origin_lon);
    data[4..12].copy_from_slice(lon_bytes.as_bytes());

    // Lat origin (e.g. 230000S)
    let lat_bytes = format!("{: <8}", origin_lat);
    data[12..20].copy_from_slice(lat_bytes.as_bytes());

    // Spacing (30 arc-seconds)
    data[20..24].copy_from_slice(b"0300");
    data[24..28].copy_from_slice(b"0300");

    // Columns count
    let cols_str = format!("{:0>4}", num_cols);
    data[47..51].copy_from_slice(cols_str.as_bytes());

    // Rows count
    let rows_str = format!("{:0>4}", num_rows);
    data[51..55].copy_from_slice(rows_str.as_bytes());

    // Populate data columns
    let col_size = 11 + num_rows * 2;
    for c in 0..num_cols {
        let mut col = vec![0u8; col_size];
        col[0] = 0xAA; // Sentinel

        // Column indexes
        col[1..4].copy_from_slice(&[0, 0, c as u8]);
        col[4..7].copy_from_slice(&[0, 0, 0]);

        // Elevations: slope where height = c * 10 + r
        for r in 0..num_rows {
            let height = (c * 10 + r) as i16;
            let be = height.to_be_bytes();
            let idx = 7 + r * 2;
            col[idx] = be[0];
            col[idx + 1] = be[1];
        }

        data.extend_from_slice(&col);
    }

    data
}

/// Creates a mock tile where the centre cell contains a null sentinel (-32767).
fn create_mock_dted0_with_null(origin_lat: &str, origin_lon: &str, num_cols: usize, num_rows: usize) -> Vec<u8> {
    let mut data = vec![b' '; 3428];
    data[0..4].copy_from_slice(b"UHL1");
    let lon_bytes = format!("{: <8}", origin_lon);
    data[4..12].copy_from_slice(lon_bytes.as_bytes());
    let lat_bytes = format!("{: <8}", origin_lat);
    data[12..20].copy_from_slice(lat_bytes.as_bytes());
    data[20..24].copy_from_slice(b"0300");
    data[24..28].copy_from_slice(b"0300");
    let cols_str = format!("{:0>4}", num_cols);
    data[47..51].copy_from_slice(cols_str.as_bytes());
    let rows_str = format!("{:0>4}", num_rows);
    data[51..55].copy_from_slice(rows_str.as_bytes());

    let col_size = 11 + num_rows * 2;
    for c in 0..num_cols {
        let mut col = vec![0u8; col_size];
        col[0] = 0xAA;
        col[1..4].copy_from_slice(&[0, 0, c as u8]);
        col[4..7].copy_from_slice(&[0, 0, 0]);

        for r in 0..num_rows {
            let height = if c == num_cols / 2 && r == num_rows / 2 {
                -32767_i16 // null sentinel
            } else {
                (c * 10 + r) as i16
            };
            let be = height.to_be_bytes();
            let idx = 7 + r * 2;
            col[idx] = be[0];
            col[idx + 1] = be[1];
        }

        data.extend_from_slice(&col);
    }

    data
}

#[test]
fn test_parse_mock_dted0() {
    let mock_bytes = create_mock_dted0("230000S", "0480000W", 121, 121);
    let mut engine = TerrainEngine::new();

    let key = engine.load_tile(&mock_bytes).unwrap();
    assert_eq!(key, TileKey { lat_deg: -23, lon_deg: -48 });

    // Southwest corner (origin) — should be zero
    let el = engine.get_elevation(-23.0, -48.0).unwrap();
    assert_eq!(el, 0.0);

    // Northeast corner (near upper limit)
    let el_ne = engine.get_elevation(-22.0 - 1e-9, -47.0 - 1e-9).unwrap();
    // col = 120, row = 120. height = 120 * 10 + 120 = 1320
    assert!((el_ne - 1320.0).abs() < 1e-4);
}

#[test]
fn test_bilinear_interpolation() {
    // 121 rows → intervals = 120. Spacing = 1/120 degree.
    let mock_bytes = create_mock_dted0("230000S", "0480000W", 121, 121);
    let mut engine = TerrainEngine::new();
    let _ = engine.load_tile(&mock_bytes).unwrap();

    // Query exactly at the middle of the first cell:
    // col = 0.5, row = 0.5
    let lat = -23.0 + 0.5 / 120.0;
    let lon = -48.0 + 0.5 / 120.0;

    let el = engine.get_elevation(lat, lon).unwrap();
    // Expected: average of (0, 10, 1, 11) = 5.5
    assert!((el - 5.5).abs() < 1e-6);

    // Query at 1/4 of the cell
    let lat2 = -23.0 + 0.25 / 120.0;
    let lon2 = -48.0 + 0.25 / 120.0;
    let el2 = engine.get_elevation(lat2, lon2).unwrap();
    // z00 = 0, z01 = 10, z10 = 1, z11 = 11
    // z_left = 0 * 0.75 + 1 * 0.25 = 0.25
    // z_right = 10 * 0.75 + 11 * 0.25 = 10.25
    // z_final = 0.25 * 0.75 + 10.25 * 0.25 = 0.1875 + 2.5625 = 2.75
    assert!((el2 - 2.75).abs() < 1e-6);
}

#[test]
fn test_vertical_profile_generation() {
    let mock_bytes = create_mock_dted0("230000S", "0480000W", 121, 121);
    let mut engine = TerrainEngine::new();
    let _ = engine.load_tile(&mock_bytes).unwrap();

    // Route from -22.9 lat, -48.0 lon to -22.9 lat, -47.9 lon (inside the tile)
    let p1 = LatLon::from_degrees(-22.9, -48.0, 0.0);
    let p2 = LatLon::from_degrees(-22.9, -47.9, 0.0);
    let route = vec![p1, p2];

    let profile = engine.get_vertical_profile(&route, 2000.0).unwrap();
    assert!(profile.len() >= 2);

    // Accumulated distances must be increasing and the first point must be zero
    assert_eq!(profile[0].distance_meters, 0.0);
    assert!(profile[1].distance_meters > 0.0);

    // Coordinates must be correct
    assert!((profile[0].coords.lat.to_degrees() - -22.9).abs() < 1e-6);
    assert!((profile[0].coords.lon.to_degrees() - -48.0).abs() < 1e-6);
}

#[test]
fn test_malformed_dted() {
    let mut engine = TerrainEngine::new();

    // Buffer too short
    let short_data = vec![0u8; 100];
    let res = engine.load_tile(&short_data);
    assert!(matches!(res, Err(TerrainError::InvalidHeader(_))));

    // Invalid signature
    let bad_signature = vec![0u8; 3500];
    let res2 = engine.load_tile(&bad_signature);
    assert!(matches!(res2, Err(TerrainError::InvalidHeader(_))));
}

#[test]
fn test_null_sentinel() {
    let mock_bytes = create_mock_dted0_with_null("230000S", "0480000W", 121, 121);
    let mut engine = TerrainEngine::new();
    let _ = engine.load_tile(&mock_bytes).unwrap();

    // Query the exact centre cell (row 60, col 60) which contains -32767
    let lat = -23.0 + 60.0 / 120.0;
    let lon = -48.0 + 60.0 / 120.0;
    let el = engine.get_elevation(lat, lon).unwrap();

    // Null sentinel should be treated as 0.0 metres
    assert!((el - 0.0).abs() < 1e-6);
}

#[test]
fn test_unload_tile() {
    let mock_bytes = create_mock_dted0("230000S", "0480000W", 121, 121);
    let mut engine = TerrainEngine::new();
    let key = engine.load_tile(&mock_bytes).unwrap();

    // Tile exists and is queryable
    assert!(engine.get_elevation(-23.0, -48.0).is_ok());

    // Unload returns true when the tile existed
    assert!(engine.unload_tile(&key));

    // After unloading, queries fail
    assert!(matches!(
        engine.get_elevation(-23.0, -48.0),
        Err(TerrainError::TileNotLoaded(_, _))
    ));

    // Unloading again returns false
    assert!(!engine.unload_tile(&key));
}

#[test]
fn test_elevation_exact_boundary() {
    let mock_bytes = create_mock_dted0("230000S", "0480000W", 121, 121);
    let mut engine = TerrainEngine::new();
    let _ = engine.load_tile(&mock_bytes).unwrap();

    // Query exactly at the northern/eastern boundary of the tile
    // lat = -22.0, lon = -47.0  (the tile spans [-23, -22) × [-48, -47))
    // This point is outside the tile, so the engine should return an error.
    let res = engine.get_elevation(-22.0, -47.0);
    assert!(matches!(res, Err(TerrainError::TileNotLoaded(-22, -47))));
}

#[test]
fn test_elevation_rad_matches_degrees() {
    let mock_bytes = create_mock_dted0("230000S", "0480000W", 121, 121);
    let mut engine = TerrainEngine::new();
    let _ = engine.load_tile(&mock_bytes).unwrap();

    let lat_deg = -23.0;
    let lon_deg = -48.0;
    let elev_deg = engine.get_elevation(lat_deg, lon_deg).unwrap();
    let elev_rad = engine.get_elevation_rad(lat_deg.to_radians(), lon_deg.to_radians()).unwrap();
    assert!((elev_deg - elev_rad).abs() < 1e-12);
}

#[test]
fn test_lru_cache_capacity_and_clear() {
    let mock_bytes = create_mock_dted0("230000S", "0480000W", 121, 121);
    let mut engine = TerrainEngine::with_capacity(2);

    engine.load_tile(&mock_bytes).unwrap();
    assert_eq!(engine.cache_size(), 1);

    engine.set_cache_capacity(1);
    assert_eq!(engine.cache_size(), 1);

    engine.clear_cache();
    assert_eq!(engine.cache_size(), 0);
    assert!(engine.get_elevation(-23.0, -48.0).is_err());
}

#[test]
fn test_vertical_profile_single_point() {
    let engine = TerrainEngine::new();
    let route = vec![LatLon::from_degrees(0.0, 0.0, 0.0)];
    let res = engine.get_vertical_profile(&route, 1000.0);
    assert!(matches!(res, Err(TerrainError::MalformedData(_))));
}

#[test]
fn test_vertical_profile_missing_tile() {
    let mock_bytes = create_mock_dted0("230000S", "0480000W", 121, 121);
    let mut engine = TerrainEngine::new();
    let _ = engine.load_tile(&mock_bytes).unwrap();

    // Route goes from inside the loaded tile to outside it
    let p1 = LatLon::from_degrees(-23.5, -48.0, 0.0);
    let p2 = LatLon::from_degrees(-20.0, -48.0, 0.0); // Outside tile
    let route = vec![p1, p2];

    let res = engine.get_vertical_profile(&route, 2000.0);
    assert!(matches!(res, Err(TerrainError::TileNotLoaded(_, _))));
}

#[test]
fn test_parse_uhl_decimal_degrees() {
    // UHL strings may use decimal degrees (e.g. "48.500W")
    let mock_bytes = create_mock_dted0("23.500S", "48.500W", 4, 4);
    let mut engine = TerrainEngine::new();
    let key = engine.load_tile(&mock_bytes).unwrap();
    // 23.500S = -23.5 degrees, floor = -24
    assert_eq!(key.lat_deg, -24);
    // 48.500W = -48.5 degrees, floor = -49
    assert_eq!(key.lon_deg, -49);
}

#[test]
fn test_parse_uhl_invalid_direction() {
    // Invalid direction character should be treated as positive (non W/S)
    let mock_bytes = create_mock_dted0("230000X", "0480000Y", 4, 4);
    let mut engine = TerrainEngine::new();
    let key = engine.load_tile(&mock_bytes).unwrap();
    // X and Y are not W/S, so treated as positive (N/E)
    assert_eq!(key.lat_deg, 23);
    assert_eq!(key.lon_deg, 48);
}

#[test]
fn test_tile_key_copy() {
    let k1 = TileKey { lat_deg: -23, lon_deg: -48 };
    let k2 = k1;
    // k1 must still be usable because TileKey is Copy
    assert_eq!(k1.lat_deg, -23);
    assert_eq!(k2.lon_deg, -48);
}

#[test]
fn test_terrain_error_display() {
    assert_eq!(
        TerrainError::InvalidHeader("bad".to_string()).to_string(),
        "Invalid DTED header: bad"
    );
    assert_eq!(
        TerrainError::MalformedData("corrupt".to_string()).to_string(),
        "Corrupted DTED data: corrupt"
    );
    assert_eq!(
        TerrainError::TileNotLoaded(-23, -48).to_string(),
        "DTED tile not loaded for coordinate (-23, -48)"
    );
}
