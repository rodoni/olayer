use crate::geodesy::coords::LatLon;
use crate::terrain::errors::TerrainError;
use crate::terrain::engine::{MsawState, TerrainEngine, TileKey, UnknownTerrainPolicy};

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
fn test_null_sentinel_is_unknown_in_status_api() {
    let mut engine = TerrainEngine::new();
    let data = create_mock_dted0_with_null("230000S", "0480000W", 121, 121);
    engine.load_tile(&data).unwrap();
    let sample = engine.get_elevation_status((-22.5_f64).to_radians(), (-47.5_f64).to_radians()).unwrap();
    assert_eq!(sample.elevation_meters, None);
}

#[test]
fn test_unknown_terrain_policy_can_reject_or_propagate() {
    let mut engine = TerrainEngine::new();
    let data = create_mock_dted0_with_null("230000S", "0480000W", 121, 121);
    engine.load_tile(&data).unwrap();
    let lat = (-22.5_f64).to_radians();
    let lon = (-47.5_f64).to_radians();

    assert_eq!(engine.get_elevation_with_policy(lat, lon, UnknownTerrainPolicy::Propagate).unwrap(), None);
    assert!(engine.get_elevation_with_policy(lat, lon, UnknownTerrainPolicy::Reject).is_err());
}

#[test]
fn test_msaw_clearance_reports_safe_warning_and_unknown() {
    let mut engine = TerrainEngine::new();
    let data = create_mock_dted0("230000S", "0480000W", 121, 121);
    engine.load_tile(&data).unwrap();
    let lat = (-23.0_f64).to_radians();
    let lon = (-48.0_f64).to_radians();
    let ground = engine.get_elevation_status(lat, lon).unwrap().elevation_meters.unwrap();

    let safe = engine.calculate_clearance(lat, lon, ground + 200.0, 100.0, UnknownTerrainPolicy::Propagate).unwrap();
    assert_eq!(safe.state, MsawState::Safe);
    let warning = engine.calculate_clearance(lat, lon, ground + 50.0, 100.0, UnknownTerrainPolicy::Propagate).unwrap();
    assert_eq!(warning.state, MsawState::Warning);

    let unknown_data = create_mock_dted0_with_null("230000S", "0480000W", 121, 121);
    let mut unknown_engine = TerrainEngine::new();
    unknown_engine.load_tile(&unknown_data).unwrap();
    let unknown = unknown_engine.calculate_clearance(
        (-22.5_f64).to_radians(),
        (-47.5_f64).to_radians(),
        1000.0,
        100.0,
        UnknownTerrainPolicy::Propagate,
    ).unwrap();
    assert_eq!(unknown.state, MsawState::Unknown);
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
    assert_eq!(
        TerrainError::RgbDecodeError("fail".to_string()).to_string(),
        "RGB terrain decode error: fail"
    );
    assert_eq!(
        TerrainError::GeoTiffError("bad tiff".to_string()).to_string(),
        "GeoTIFF terrain error: bad tiff"
    );
}

#[test]
fn test_mapbox_rgb_decoding() {
    use crate::terrain::rgb_decoder::{decode_mapbox_rgb, decode_rgb_elevation, RgbElevationEncoding};

    // Sea level: height = 0m -> -10000 + (R*65536 + G*256 + B)*0.1 = 0
    // (R*65536 + G*256 + B) = 100,000 -> R = 1, G = 134, B = 160
    let elev_0 = decode_mapbox_rgb(1, 134, 160);
    assert!((elev_0 - 0.0).abs() < 0.1);

    // Everest peak (~8848.8m) -> 18848.8 * 10 = 188488 -> R = 2, G = 224, B = 72
    let elev_everest = decode_mapbox_rgb(2, 224, 72);
    assert!((elev_everest - 8848.8).abs() < 0.2);

    // Marianna Trench (-10000m) -> R = 0, G = 0, B = 0
    let elev_trench = decode_mapbox_rgb(0, 0, 0);
    assert!((elev_trench - (-10000.0)).abs() < 1e-6);

    let elev_via_enum = decode_rgb_elevation(1, 134, 160, RgbElevationEncoding::MapboxRgb);
    assert!((elev_via_enum - 0.0).abs() < 0.1);
}

#[test]
fn test_terrarium_rgb_decoding() {
    use crate::terrain::rgb_decoder::{decode_terrarium_rgb, decode_rgb_elevation, RgbElevationEncoding};

    // Sea level (0m) in Terrarium: (R*256 + G + B/256) = 32768 -> R = 128, G = 0, B = 0
    let elev_0 = decode_terrarium_rgb(128, 0, 0);
    assert!((elev_0 - 0.0).abs() < 1e-6);

    // +1000m: 33768 -> R = 131, G = 232, B = 0
    let elev_1000 = decode_terrarium_rgb(131, 232, 0);
    assert!((elev_1000 - 1000.0).abs() < 1e-3);

    let elev_via_enum = decode_rgb_elevation(128, 0, 0, RgbElevationEncoding::Terrarium);
    assert!((elev_via_enum - 0.0).abs() < 1e-6);
}

#[test]
fn test_rgb_tile_bounds_and_elevation() {
    use crate::terrain::rgb_decoder::RgbElevationEncoding;
    use crate::terrain::rgb_tile::{RgbElevationTile, SlippyTileKey};

    // Tile at z=10, x=512, y=512 (Center of world around equator and prime meridian)
    let key = SlippyTileKey::new(10, 512, 512);
    let (min_lat, min_lon, max_lat, max_lon) = key.bounds_rad();

    assert!(max_lat > min_lat);
    assert!(max_lon > min_lon);
    assert!(min_lon.abs() < 0.01);

    // Create a 2x2 RGBA buffer with constant 500m elevation
    // 500m Mapbox RGB: 10500 * 10 = 105000 -> R = 1, G = 154, B = 40
    let mut rgba_buf = Vec::new();
    for _ in 0..4 {
        rgba_buf.extend_from_slice(&[1, 154, 40, 255]);
    }

    let tile = RgbElevationTile::from_rgba(key, 2, 2, &rgba_buf, RgbElevationEncoding::MapboxRgb).unwrap();
    assert_eq!(tile.width, 2);
    assert_eq!(tile.height, 2);

    let mid_lat = (min_lat + max_lat) / 2.0;
    let mid_lon = (min_lon + max_lon) / 2.0;

    let elev = tile.get_elevation_rad(mid_lat, mid_lon);
    assert!(elev.is_some());
    assert!((elev.unwrap() - 500.0).abs() < 0.2);

    // Outside the tile should return None
    assert!(tile.get_elevation_rad(1.0, 1.0).is_none());
}

fn create_mock_geotiff_float32(width: u32, height: u32, min_lon_deg: f64, max_lat_deg: f64, pixel_scale_deg: f64, elevation_val: f32) -> Vec<u8> {
    let mut bytes = Vec::new();

    // TIFF Header (8 bytes)
    bytes.extend_from_slice(b"II"); // Little-endian
    bytes.extend_from_slice(&42u16.to_le_bytes());
    bytes.extend_from_slice(&8u32.to_le_bytes()); // First IFD at byte 8

    // IFD: 7 entries (2 + 7 * 12 + 4 = 90 bytes -> ends at 98)
    bytes.extend_from_slice(&7u16.to_le_bytes());

    let scale_offset = 120u32;
    let tiepoint_offset = scale_offset + 24; // 144
    let raster_offset = tiepoint_offset + 48; // 192

    // Tag 256: ImageWidth (LONG)
    bytes.extend_from_slice(&256u16.to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&width.to_le_bytes());

    // Tag 257: ImageLength (LONG)
    bytes.extend_from_slice(&257u16.to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());

    // Tag 258: BitsPerSample (SHORT)
    bytes.extend_from_slice(&258u16.to_le_bytes());
    bytes.extend_from_slice(&3u16.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());

    // Tag 273: StripOffsets (LONG)
    bytes.extend_from_slice(&273u16.to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&raster_offset.to_le_bytes());

    // Tag 339: SampleFormat (SHORT: 3 = IEEE float)
    bytes.extend_from_slice(&339u16.to_le_bytes());
    bytes.extend_from_slice(&3u16.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&3u16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());

    // Tag 33550: ModelPixelScaleTag (DOUBLE: 3 doubles)
    bytes.extend_from_slice(&33550u16.to_le_bytes());
    bytes.extend_from_slice(&12u16.to_le_bytes());
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&scale_offset.to_le_bytes());

    // Tag 33922: ModelTiepointTag (DOUBLE: 6 doubles)
    bytes.extend_from_slice(&33922u16.to_le_bytes());
    bytes.extend_from_slice(&12u16.to_le_bytes());
    bytes.extend_from_slice(&6u32.to_le_bytes());
    bytes.extend_from_slice(&tiepoint_offset.to_le_bytes());

    // Next IFD = 0
    bytes.extend_from_slice(&0u32.to_le_bytes());

    // Pad up to scale_offset (120)
    while bytes.len() < scale_offset as usize {
        bytes.push(0);
    }

    // Scale data: [pixel_scale, pixel_scale, 0.0]
    bytes.extend_from_slice(&pixel_scale_deg.to_le_bytes());
    bytes.extend_from_slice(&pixel_scale_deg.to_le_bytes());
    bytes.extend_from_slice(&0.0_f64.to_le_bytes());

    // Pad up to tiepoint_offset (144)
    while bytes.len() < tiepoint_offset as usize {
        bytes.push(0);
    }

    // Tiepoint data: [0.0, 0.0, 0.0, min_lon, max_lat, 0.0]
    bytes.extend_from_slice(&0.0_f64.to_le_bytes());
    bytes.extend_from_slice(&0.0_f64.to_le_bytes());
    bytes.extend_from_slice(&0.0_f64.to_le_bytes());
    bytes.extend_from_slice(&min_lon_deg.to_le_bytes());
    bytes.extend_from_slice(&max_lat_deg.to_le_bytes());
    bytes.extend_from_slice(&0.0_f64.to_le_bytes());

    // Pad up to raster_offset (192)
    while bytes.len() < raster_offset as usize {
        bytes.push(0);
    }

    // Raster data
    for _ in 0..(width * height) {
        bytes.extend_from_slice(&elevation_val.to_le_bytes());
    }

    bytes
}

#[test]
fn test_geotiff_parsing_and_elevation() {
    use crate::terrain::geotiff::GeoTiffTile;

    let geotiff_bytes = create_mock_geotiff_float32(4, 4, 10.0, 50.0, 0.25, 750.5);
    let tile = GeoTiffTile::from_bytes(&geotiff_bytes).expect("GeoTIFF parse failed");

    assert_eq!(tile.width, 4);
    assert_eq!(tile.height, 4);

    let (min_lat, min_lon, max_lat, max_lon) = tile.bounds_rad;
    assert!((min_lon.to_degrees() - 10.0).abs() < 1e-6);
    assert!((max_lon.to_degrees() - 11.0).abs() < 1e-6);
    assert!((max_lat.to_degrees() - 50.0).abs() < 1e-6);
    assert!((min_lat.to_degrees() - 49.0).abs() < 1e-6);

    let elev = tile.get_elevation_rad(49.5_f64.to_radians(), 10.5_f64.to_radians());
    assert!(elev.is_some());
    assert!((elev.unwrap() - 750.5).abs() < 1e-3);
}

#[test]
fn test_multi_source_terrain_engine() {
    use crate::terrain::rgb_decoder::RgbElevationEncoding;

    let engine = TerrainEngine::new();

    // 1. Load an RGB tile covering z=10, x=512, y=512 (lat ~0.0, lon ~0.0)
    let mut rgba_buf = Vec::new();
    for _ in 0..4 {
        // Mapbox RGB 1200m: 11200 * 10 = 112000 -> R = 1, G = 181, B = 128
        rgba_buf.extend_from_slice(&[1, 181, 128, 255]);
    }
    engine.load_rgb_tile(10, 512, 512, 2, 2, &rgba_buf, RgbElevationEncoding::MapboxRgb).unwrap();
    assert_eq!(engine.rgb_cache_size(), 1);

    // 2. Load a GeoTIFF covering lat 49..50, lon 10..11 with 820m elevation
    let geotiff_bytes = create_mock_geotiff_float32(2, 2, 10.0, 50.0, 0.5, 820.0);
    engine.load_geotiff_tile(&geotiff_bytes).unwrap();
    assert_eq!(engine.geotiff_cache_size(), 1);

    // Query RGB tile coverage at -0.05 lat, 0.05 lon (within tile bounds)
    let elev_rgb = engine.get_elevation(-0.05, 0.05).unwrap();
    assert!((elev_rgb - 1200.0).abs() < 0.5);

    // Query GeoTIFF coverage at 49.5 lat, 10.5 lon
    let elev_gt = engine.get_elevation(49.5, 10.5).unwrap();
    assert!((elev_gt - 820.0).abs() < 0.5);

    // Query unloaded area
    assert!(engine.get_elevation(80.0, 80.0).is_err());
}
