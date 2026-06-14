#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::too_many_arguments)]

use std::os::raw::{c_char, c_int};
use olayer_core::geodesy::LatLon;
use olayer_core::terrain::TerrainEngine;
use olayer_core::interpolator::{InterpolationEngine, TargetState};

// --- C-COMPATIBLE DATA STRUCTURES ---

/// C representation of geodetic coordinate.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct C_LatLon {
    pub lat: f64,
    pub lon: f64,
    pub height: f64,
}

/// C representation of interpolated target.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct C_InterpolatedTarget {
    pub id: *mut c_char,
    pub lat: f64,
    pub lon: f64,
    pub height: f64,
    pub heading_rad: f64,
}

/// C representation of vertical profile point.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct C_ProfilePoint {
    pub distance_meters: f64,
    pub ground_elevation: f64,
    pub lat: f64,
    pub lon: f64,
    pub height: f64,
}

// --- TERRAIN ENGINE C-API ---

/// Creates a new TerrainEngine instance and returns an opaque pointer.
#[no_mangle]
pub extern "C" fn olayer_terrain_engine_create() -> *mut TerrainEngine {
    Box::into_raw(Box::new(TerrainEngine::new()))
}

/// Parses and registers a raw DTED buffer.
/// Returns 0 on success, or a negative code on error.
#[no_mangle]
pub unsafe extern "C" fn olayer_terrain_engine_load_tile(
    engine: *mut TerrainEngine,
    data: *const u8,
    length: usize,
    out_lat_deg: *mut i32,
    out_lon_deg: *mut i32,
) -> c_int {
    if engine.is_null() || data.is_null() {
        return -1; // Null pointer error
    }

    let data_slice = std::slice::from_raw_parts(data, length);
    let engine_ref = &mut *engine;

    // Use catch_unwind to prevent unwinding across FFI boundary
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine_ref.load_tile(data_slice)
    }));

    match result {
        Ok(Ok(key)) => {
            if !out_lat_deg.is_null() {
                *out_lat_deg = key.lat_deg;
            }
            if !out_lon_deg.is_null() {
                *out_lon_deg = key.lon_deg;
            }
            0
        }
        Ok(Err(_)) => -2, // Format/parsing error
        Err(_) => -99,    // Panic caught
    }
}

/// Unloads a terrain tile. Returns 1 if tile existed, 0 if not, or negative error.
#[no_mangle]
pub unsafe extern "C" fn olayer_terrain_engine_unload_tile(
    engine: *mut TerrainEngine,
    lat_deg: i32,
    lon_deg: i32,
) -> c_int {
    if engine.is_null() {
        return -1;
    }
    let engine_ref = &mut *engine;
    let key = olayer_core::terrain::engine::TileKey { lat_deg, lon_deg };
    
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine_ref.unload_tile(&key)
    }));

    match result {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(_) => -99,
    }
}

/// Resolves elevation at coordinate degrees. Returns 0 on success, negative error.
#[no_mangle]
pub unsafe extern "C" fn olayer_terrain_engine_get_elevation(
    engine: *mut TerrainEngine,
    lat_deg: f64,
    lon_deg: f64,
    out_elevation: *mut f64,
) -> c_int {
    if engine.is_null() || out_elevation.is_null() {
        return -1;
    }
    let engine_ref = &mut *engine;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine_ref.get_elevation(lat_deg, lon_deg)
    }));

    match result {
        Ok(Ok(elev)) => {
            *out_elevation = elev;
            0
        }
        Ok(Err(_)) => -2, // Tile not loaded
        Err(_) => -99,
    }
}

/// Generates a vertical profile. Fills out_profile and out_count.
/// Returns 0 on success, negative error.
#[no_mangle]
pub unsafe extern "C" fn olayer_terrain_engine_get_vertical_profile(
    engine: *mut TerrainEngine,
    route_lat: *const f64,
    route_lon: *const f64,
    route_height: *const f64,
    route_len: usize,
    step_meters: f64,
    out_profile: *mut *mut C_ProfilePoint,
    out_count: *mut usize,
) -> c_int {
    if engine.is_null() || route_lat.is_null() || route_lon.is_null() || route_height.is_null() || out_profile.is_null() || out_count.is_null() {
        return -1;
    }

    let mut route = Vec::with_capacity(route_len);
    for i in 0..route_len {
        route.push(LatLon::from_degrees(
            *route_lat.add(i),
            *route_lon.add(i),
            *route_height.add(i),
        ));
    }

    let engine_ref = &mut *engine;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine_ref.get_vertical_profile(&route, step_meters)
    }));

    match result {
        Ok(Ok(profile)) => {
            let mut c_points: Vec<C_ProfilePoint> = profile.into_iter().map(|p| {
                C_ProfilePoint {
                    distance_meters: p.distance_meters,
                    ground_elevation: p.ground_elevation,
                    lat: p.coords.lat.to_degrees(),
                    lon: p.coords.lon.to_degrees(),
                    height: p.coords.height,
                }
            }).collect();

            c_points.shrink_to_fit();
            let count = c_points.len();
            let ptr = c_points.as_mut_ptr();
            std::mem::forget(c_points); // Leak vector allocation to C host control

            *out_profile = ptr;
            *out_count = count;
            0
        }
        Ok(Err(_)) => -2, // Missing tile or malformed route
        Err(_) => -99,
    }
}

/// Frees profile point array allocated by Rust.
#[no_mangle]
pub unsafe extern "C" fn olayer_profile_points_free(points: *mut C_ProfilePoint, count: usize) {
    if !points.is_null() && count > 0 {
        let _ = Vec::from_raw_parts(points, count, count);
    }
}

/// Destroys a TerrainEngine instance.
#[no_mangle]
pub unsafe extern "C" fn olayer_terrain_engine_free(engine: *mut TerrainEngine) {
    if !engine.is_null() {
        let _ = Box::from_raw(engine);
    }
}

// --- INTERPOLATOR ENGINE C-API ---

/// Creates a new InterpolationEngine instance.
#[no_mangle]
pub extern "C" fn olayer_interpolator_create() -> *mut InterpolationEngine {
    Box::into_raw(Box::new(InterpolationEngine::new()))
}

/// Creates a new InterpolationEngine instance with custom stale threshold.
#[no_mangle]
pub extern "C" fn olayer_interpolator_create_with_threshold(stale_threshold: f64) -> *mut InterpolationEngine {
    Box::into_raw(Box::new(InterpolationEngine::with_stale_threshold(stale_threshold)))
}

/// Updates or inserts a target state. Returns 0 on success, negative error.
#[no_mangle]
pub unsafe extern "C" fn olayer_interpolator_update(
    engine: *mut InterpolationEngine,
    id: *const c_char,
    lat: f64,
    lon: f64,
    height: f64,
    speed_mps: f64,
    track_heading_rad: f64,
    vertical_rate_mps: f64,
    time: f64,
) -> c_int {
    if engine.is_null() || id.is_null() {
        return -1;
    }

    let id_str = match std::ffi::CStr::from_ptr(id).to_str() {
        Ok(s) => s,
        Err(_) => return -3,
    };

    let engine_ref = &mut *engine;
    let state = TargetState {
        id: id_str.to_string(),
        last_position: LatLon::new(lat, lon, height),
        speed_mps,
        track_heading_rad,
        vertical_rate_mps,
        last_ping_time: time,
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine_ref.update_target(state)
    }));

    match result {
        Ok(Ok(_)) => 0,
        Ok(Err(_)) => -4, // Invalid target state
        Err(_) => -99,
    }
}

/// Removes a target. Returns 1 if present, 0 if not, or negative error.
#[no_mangle]
pub unsafe extern "C" fn olayer_interpolator_remove(
    engine: *mut InterpolationEngine,
    id: *const c_char,
) -> c_int {
    if engine.is_null() || id.is_null() {
        return -1;
    }

    let id_str = match std::ffi::CStr::from_ptr(id).to_str() {
        Ok(s) => s,
        Err(_) => return -3,
    };

    let engine_ref = &mut *engine;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine_ref.remove_target(id_str)
    }));

    match result {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(_) => -99,
    }
}

/// Interpolates all targets. Fills out_targets and out_count.
/// Returns 0 on success, negative error.
#[no_mangle]
pub unsafe extern "C" fn olayer_interpolator_interpolate_all(
    engine: *mut InterpolationEngine,
    current_time: f64,
    out_targets: *mut *mut C_InterpolatedTarget,
    out_count: *mut usize,
) -> c_int {
    if engine.is_null() || out_targets.is_null() || out_count.is_null() {
        return -1;
    }

    let engine_ref = &mut *engine;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine_ref.interpolate_all(current_time)
    }));

    match result {
        Ok(Ok(targets)) => {
            let mut c_targets: Vec<C_InterpolatedTarget> = Vec::with_capacity(targets.len());
            for t in targets {
                // Skip targets whose ID contains an embedded null byte
                let id = match std::ffi::CString::new(t.id) {
                    Ok(cstr) => cstr.into_raw(),
                    Err(_) => continue,
                };
                c_targets.push(C_InterpolatedTarget {
                    id,
                    lat: t.position.lat,
                    lon: t.position.lon,
                    height: t.position.height,
                    heading_rad: t.heading_rad,
                });
            }

            let count = c_targets.len();
            let ptr = c_targets.as_mut_ptr();
            std::mem::forget(c_targets); // Leak allocation to C host control

            *out_targets = ptr;
            *out_count = count;
            0
        }
        Ok(Err(_)) => -2, // Interpolation failed
        Err(_) => -99,
    }
}

/// Frees interpolated targets allocated by Rust.
#[no_mangle]
pub unsafe extern "C" fn olayer_interpolated_targets_free(targets: *mut C_InterpolatedTarget, count: usize) {
    if !targets.is_null() && count > 0 {
        let vec = Vec::from_raw_parts(targets, count, count);
        for t in vec {
            if !t.id.is_null() {
                let _ = std::ffi::CString::from_raw(t.id);
            }
        }
    }
}

/// Destroys an InterpolationEngine instance.
#[no_mangle]
pub unsafe extern "C" fn olayer_interpolator_free(engine: *mut InterpolationEngine) {
    if !engine.is_null() {
        let _ = Box::from_raw(engine);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal mock DTED Level 0 tile (4x4) for FFI tests.
    fn create_mock_dted0(origin_lat: &str, origin_lon: &str, num_cols: usize, num_rows: usize) -> Vec<u8> {
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

    #[test]
    fn test_c_ffi_interpolator_flow() {
        unsafe {
            let engine = olayer_interpolator_create();
            assert!(!engine.is_null());

            let id_str = std::ffi::CString::new("FL123").unwrap();
            let update_res = olayer_interpolator_update(
                engine,
                id_str.as_ptr(),
                -0.41, // lat
                -0.81, // lon
                10000.0, // height
                250.0, // speed
                1.57, // heading
                0.0, // vertical rate
                1000.0, // time
            );
            assert_eq!(update_res, 0);

            let mut targets_ptr: *mut C_InterpolatedTarget = std::ptr::null_mut();
            let mut count: usize = 0;
            let interp_res = olayer_interpolator_interpolate_all(
                engine,
                1010.0, // current time
                &mut targets_ptr,
                &mut count,
            );
            assert_eq!(interp_res, 0);
            assert_eq!(count, 1);
            assert!(!targets_ptr.is_null());

            let target = &*targets_ptr;
            let target_id = std::ffi::CStr::from_ptr(target.id).to_str().unwrap();
            assert_eq!(target_id, "FL123");
            assert!(target.lat != -0.41); // Should have moved
            assert_eq!(target.height, 10000.0);

            olayer_interpolated_targets_free(targets_ptr, count);
            olayer_interpolator_free(engine);
        }
    }

    #[test]
    fn test_c_ffi_terrain_error_handling() {
        unsafe {
            let engine = olayer_terrain_engine_create();
            assert!(!engine.is_null());

            // Try to resolve elevation for not loaded tile
            let mut elev = 0.0;
            let elev_res = olayer_terrain_engine_get_elevation(engine, -23.0, -46.0, &mut elev);
            assert_eq!(elev_res, -2, "Should fail because tile is not loaded");

            // Try to load invalid DTED file data
            let fake_data = [0u8; 100];
            let mut out_lat = 0;
            let mut out_lon = 0;
            let load_res = olayer_terrain_engine_load_tile(
                engine,
                fake_data.as_ptr(),
                fake_data.len(),
                &mut out_lat,
                &mut out_lon,
            );
            assert_eq!(load_res, -2, "Should fail on invalid DTED bytes");

            olayer_terrain_engine_free(engine);
        }
    }

    #[test]
    fn test_c_ffi_null_pointers() {
        unsafe {
            // Terrain create is the only one that doesn't take a pointer
            let engine = olayer_terrain_engine_create();
            assert!(!engine.is_null());

            // Null engine pointers
            let fake_data = [0u8; 100];
            let mut out_lat = 0;
            let mut out_lon = 0;
            assert_eq!(olayer_terrain_engine_load_tile(
                std::ptr::null_mut(), fake_data.as_ptr(), 10, &mut out_lat, &mut out_lon,
            ), -1);
            assert_eq!(olayer_terrain_engine_unload_tile(
                std::ptr::null_mut(), 0, 0,
            ), -1);
            let mut elev = 0.0;
            assert_eq!(olayer_terrain_engine_get_elevation(
                std::ptr::null_mut(), 0.0, 0.0, &mut elev,
            ), -1);
            assert_eq!(olayer_terrain_engine_get_elevation(
                engine, 0.0, 0.0, std::ptr::null_mut(),
            ), -1);

            // Null route pointers for vertical profile
            let mut out_profile: *mut C_ProfilePoint = std::ptr::null_mut();
            let mut count: usize = 0;
            assert_eq!(olayer_terrain_engine_get_vertical_profile(
                engine,
                std::ptr::null(), std::ptr::null(), std::ptr::null(),
                0, 100.0,
                &mut out_profile, &mut count,
            ), -1);

            // Null interpolator pointers
            let id = std::ffi::CString::new("X").unwrap();
            let mut ptr: *mut C_InterpolatedTarget = std::ptr::null_mut();
            let mut cnt: usize = 0;
            assert_eq!(olayer_interpolator_update(
                std::ptr::null_mut(), id.as_ptr(), 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            ), -1);
            assert_eq!(olayer_interpolator_update(
                engine as *mut InterpolationEngine, std::ptr::null(), 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            ), -1);
            assert_eq!(olayer_interpolator_remove(
                std::ptr::null_mut(), id.as_ptr(),
            ), -1);
            assert_eq!(olayer_interpolator_interpolate_all(
                std::ptr::null_mut(), 0.0, &mut ptr, &mut cnt,
            ), -1);
            assert_eq!(olayer_interpolator_interpolate_all(
                engine as *mut InterpolationEngine, 0.0, std::ptr::null_mut(), &mut cnt,
            ), -1);

            olayer_terrain_engine_free(engine);
        }
    }

    #[test]
    fn test_c_ffi_terrain_load_and_query() {
        unsafe {
            let engine = olayer_terrain_engine_create();
            assert!(!engine.is_null());

            let mock = create_mock_dted0("230000S", "0480000W", 4, 4);
            let mut out_lat = 0;
            let mut out_lon = 0;
            let load_res = olayer_terrain_engine_load_tile(
                engine, mock.as_ptr(), mock.len(), &mut out_lat, &mut out_lon,
            );
            assert_eq!(load_res, 0);
            assert_eq!(out_lat, -23);
            assert_eq!(out_lon, -48);

            // Query southwest corner (origin) → elevation 0
            let mut elev = -1.0;
            let q1 = olayer_terrain_engine_get_elevation(engine, -23.0, -48.0, &mut elev);
            assert_eq!(q1, 0);
            assert!((elev - 0.0).abs() < 1e-6);

            // Query exact grid cell (col=1, row=1) → elevation = 1*10+1 = 11
            let mut elev2 = -1.0;
            let q2 = olayer_terrain_engine_get_elevation(
                engine, -23.0 + 1.0 / 3.0, -48.0 + 1.0 / 3.0, &mut elev2,
            );
            assert_eq!(q2, 0);
            assert!((elev2 - 11.0).abs() < 1e-3);

            // Unload and verify it is gone
            let ul = olayer_terrain_engine_unload_tile(engine, -23, -48);
            assert_eq!(ul, 1);
            let mut elev3 = -1.0;
            let q3 = olayer_terrain_engine_get_elevation(engine, -23.0, -48.0, &mut elev3);
            assert_eq!(q3, -2);

            olayer_terrain_engine_free(engine);
        }
    }

    #[test]
    fn test_c_ffi_vertical_profile() {
        unsafe {
            let engine = olayer_terrain_engine_create();
            assert!(!engine.is_null());

            let mock = create_mock_dted0("230000S", "0480000W", 121, 121);
            let mut out_lat = 0;
            let mut out_lon = 0;
            let load_res = olayer_terrain_engine_load_tile(
                engine, mock.as_ptr(), mock.len(), &mut out_lat, &mut out_lon,
            );
            assert_eq!(load_res, 0);

            // Route inside the tile
            let route_lat = [-22.9_f64, -22.9];
            let route_lon = [-48.0_f64, -47.9];
            let route_height = [0.0_f64, 0.0];
            let mut out_profile: *mut C_ProfilePoint = std::ptr::null_mut();
            let mut count: usize = 0;

            let prof_res = olayer_terrain_engine_get_vertical_profile(
                engine,
                route_lat.as_ptr(), route_lon.as_ptr(), route_height.as_ptr(),
                2, 2000.0,
                &mut out_profile, &mut count,
            );
            assert_eq!(prof_res, 0);
            assert!(count >= 2);
            assert!(!out_profile.is_null());

            let first = &*out_profile;
            assert!((first.lat - -22.9).abs() < 1e-5);
            assert!((first.lon - -48.0).abs() < 1e-5);

            olayer_profile_points_free(out_profile, count);
            olayer_terrain_engine_free(engine);
        }
    }

    #[test]
    fn test_c_ffi_interpolator_remove() {
        unsafe {
            let engine = olayer_interpolator_create();
            assert!(!engine.is_null());

            let id = std::ffi::CString::new("REMOVE_ME").unwrap();
            let update = olayer_interpolator_update(
                engine, id.as_ptr(), 0.0, 0.0, 100.0, 10.0, 0.0, 0.0, 0.0,
            );
            assert_eq!(update, 0);

            // Remove existing target → 1
            let rem1 = olayer_interpolator_remove(engine, id.as_ptr());
            assert_eq!(rem1, 1);

            // Remove again → 0
            let rem2 = olayer_interpolator_remove(engine, id.as_ptr());
            assert_eq!(rem2, 0);

            // Interpolate should yield empty result
            let mut ptr: *mut C_InterpolatedTarget = std::ptr::null_mut();
            let mut cnt: usize = 0;
            let interp = olayer_interpolator_interpolate_all(engine, 10.0, &mut ptr, &mut cnt);
            assert_eq!(interp, 0);
            assert_eq!(cnt, 0);

            olayer_interpolator_free(engine);
        }
    }

    #[test]
    fn test_c_ffi_null_byte_id_skipped() {
        unsafe {
            let engine = olayer_interpolator_create();
            assert!(!engine.is_null());

            // Create a target with a normal ID
            let id_ok = std::ffi::CString::new("OK").unwrap();
            let r1 = olayer_interpolator_update(
                engine, id_ok.as_ptr(), 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            );
            assert_eq!(r1, 0);

            // Directly insert a target with an embedded null byte via Rust API
            let bad_state = TargetState {
                id: "BAD\x00TARGET".to_string(),
                last_position: LatLon::new(0.0, 0.0, 0.0),
                speed_mps: 0.0,
                track_heading_rad: 0.0,
                vertical_rate_mps: 0.0,
                last_ping_time: 0.0,
            };
            let engine_ref = &mut *engine;
            engine_ref.update_target(bad_state).unwrap();

            // Interpolate should skip the bad ID but keep the good one
            let mut ptr: *mut C_InterpolatedTarget = std::ptr::null_mut();
            let mut count: usize = 0;
            let interp = olayer_interpolator_interpolate_all(engine, 10.0, &mut ptr, &mut count);
            assert_eq!(interp, 0);
            assert_eq!(count, 1);
            assert!(!ptr.is_null());

            let target = &*ptr;
            let target_id = std::ffi::CStr::from_ptr(target.id).to_str().unwrap();
            assert_eq!(target_id, "OK");

            olayer_interpolated_targets_free(ptr, count);
            olayer_interpolator_free(engine);
        }
    }

    #[test]
    fn test_c_ffi_invalid_utf8_id() {
        unsafe {
            let engine = olayer_interpolator_create();
            assert!(!engine.is_null());

            // Invalid UTF-8 sequence
            let bad_bytes = [0x80u8, 0x81, 0x82, 0x00];
            let r = olayer_interpolator_update(
                engine, bad_bytes.as_ptr() as *const c_char,
                0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            );
            assert_eq!(r, -3, "Should reject invalid UTF-8 ID");

            olayer_interpolator_free(engine);
        }
    }
}
