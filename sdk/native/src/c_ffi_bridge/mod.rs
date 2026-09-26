use crate::tools::{
    HoldingPatternConfig, IlsConeConfig, RangeRingsConfig, TacticalToolsManager, TurnDirection,
    MAX_HOLDING_PATTERN_POINTS_PER_TURN, MAX_ILS_ARC_STEPS, MAX_RANGE_RING_POINTS,
};
use olayer_core::aeronautical::{
    export_dataset_to_geojson, parse_aixm_51_str, parse_geojson_aviation_str, AeronauticalDataset,
    NavaidType,
};
use olayer_core::geodesy::{
    compute_route_deviation, geodesic_intersection, EnuPoint, GeodesicPolygon, LatLon,
    LocalTangentFrame, MagneticModel,
};
use olayer_core::interpolator::{InterpolationEngine, TargetState};
use olayer_core::terrain::TerrainEngine;
use std::os::raw::{c_char, c_int};

const MAX_FFI_BUFFER_BYTES: usize = 256 * 1024 * 1024;
const MAX_FFI_TEXT_BYTES: usize = 16 * 1024 * 1024;
const MAX_TARGET_ID_BYTES: usize = 1_024;
const MAX_FFI_ARRAY_ITEMS: usize = 65_536;
const MAX_ROUTE_POINTS: usize = 4_096;
const MAX_PROFILE_SAMPLES: usize = 1_000_000;
const MAX_GRID_CELLS: usize = 1_000_000;
const MAX_ISOLINE_WORK: usize = 1_000_000;
const MAX_ISOVALUES: usize = 64;
const MAX_LABEL_TARGETS: usize = 256;

// --- C-COMPATIBLE DATA STRUCTURES ---

/// C representation of geodetic coordinate.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct C_LatLon {
    pub lat: f64,
    pub lon: f64,
    pub height: f64,
}

/// C representation of topocentric East-North-Up coordinate.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct C_EnuPoint {
    pub east_m: f64,
    pub north_m: f64,
    pub up_m: f64,
}

/// C representation of magnetic field elements.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct C_MagneticElements {
    pub declination_rad: f64,
    pub inclination_rad: f64,
    pub horizontal_intensity_nt: f64,
    pub total_intensity_nt: f64,
    pub x_nt: f64,
    pub y_nt: f64,
    pub z_nt: f64,
}

/// C representation of route deviation metrics (XTK / ATD).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct C_RouteDeviation {
    pub cross_track_error_meters: f64,
    pub along_track_distance_meters: f64,
    pub nearest_point: C_LatLon,
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
    pub quality: c_int,
}

/// Prediction quality: 0 valid, 1 stale, 2 clock-skewed, 3 unavailable.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum C_PredictionQuality {
    Valid = 0,
    Stale = 1,
    ClockSkewed = 2,
    Unavailable = 3,
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

/// C representation of Range and Bearing Line (RBL / CRSR) measurement.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct C_RblMeasurement {
    pub from_lat_deg: f64,
    pub from_lon_deg: f64,
    pub to_lat_deg: f64,
    pub to_lon_deg: f64,
    pub distance_nm: f64,
    pub distance_km: f64,
    pub true_bearing_deg: f64,
    pub magnetic_bearing_deg: f64,
    pub reciprocal_true_bearing_deg: f64,
    pub reciprocal_magnetic_bearing_deg: f64,
    pub estimated_time_enroute_sec: f64, // -1.0 if speed not provided
}

/// C representation of a Projected Position Leader (PPL) tick mark.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct C_PplTick {
    pub time_minutes: f64,
    pub distance_nm: f64,
    pub lat_deg: f64,
    pub lon_deg: f64,
}

/// C representation of a radio navigation aid summary.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct C_NavaidSummary {
    pub lat: f64,
    pub lon: f64,
    pub elevation_m: f64,
    pub frequency_mhz: f64,
    pub navaid_type_code: c_int,
}

// --- TERRAIN ENGINE C-API ---

/// Creates a new TerrainEngine instance and returns an opaque pointer.
#[no_mangle]
pub extern "C" fn olayer_terrain_engine_create() -> *mut TerrainEngine {
    Box::into_raw(Box::new(TerrainEngine::new()))
}

/// Parses and registers a raw DTED buffer.
/// Returns 0 on success, or a negative code on error.
///
/// # Safety
/// `engine` must be a live exclusive handle from `olayer_terrain_engine_create`.
/// `data` must be aligned and readable for `length` bytes; the byte range must
/// not exceed `isize::MAX` and must not overlap the engine or output locations.
/// Each non-null output pointer must be aligned, writable for one `i32`, and
/// disjoint from every other referenced range for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn olayer_terrain_engine_load_tile(
    engine: *mut TerrainEngine,
    data: *const u8,
    length: usize,
    out_lat_deg: *mut i32,
    out_lon_deg: *mut i32,
) -> c_int {
    // SAFETY: The caller must provide a live engine, a readable `data` buffer
    // of `length` bytes, and writable output pointers when non-null.
    if engine.is_null()
        || data.is_null()
        || length > MAX_FFI_BUFFER_BYTES
        || length > isize::MAX as usize
    {
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
///
/// # Safety
/// `engine` must be a live exclusive handle returned by
/// `olayer_terrain_engine_create`, not freed or concurrently accessed.
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
    let key = olayer_core::terrain::TileKey { lat_deg, lon_deg };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine_ref.unload_tile(&key)
    }));

    match result {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(_) => -99,
    }
}

/// Loads and registers a Web Mercator (Z, X, Y) RGB elevation tile.
/// encoding_code: 0 = MapboxRgb, 1 = Terrarium.
/// Returns 0 on success, or a negative code on error.
///
/// # Safety
/// `engine` must be a live shared handle returned by
/// `olayer_terrain_engine_create`. `rgba_data` must be aligned and readable for
/// `rgba_len` bytes (no more than `isize::MAX`), and must not overlap `engine`.
#[no_mangle]
pub unsafe extern "C" fn olayer_terrain_engine_load_rgb_tile(
    engine: *mut TerrainEngine,
    z: u32,
    x: u32,
    y: u32,
    encoding_code: c_int,
    rgba_data: *const u8,
    rgba_len: usize,
    width: usize,
    height: usize,
) -> c_int {
    if engine.is_null()
        || rgba_data.is_null()
        || rgba_len > MAX_FFI_BUFFER_BYTES
        || rgba_len > isize::MAX as usize
    {
        return -1;
    }
    let Some(expected_rgba_len) = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
    else {
        return -1;
    };
    if expected_rgba_len != rgba_len {
        return -1;
    }
    let encoding = match encoding_code {
        0 => olayer_core::terrain::RgbElevationEncoding::MapboxRgb,
        1 => olayer_core::terrain::RgbElevationEncoding::Terrarium,
        _ => return -1,
    };
    let data_slice = std::slice::from_raw_parts(rgba_data, rgba_len);
    let engine_ref = &*engine;
    match engine_ref.load_rgb_tile(z, x, y, width, height, data_slice, encoding) {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

/// Loads a GeoTIFF / Cloud-Optimized GeoTIFF raster.
/// Returns 0 on success, or negative error.
///
/// # Safety
/// `engine` must be a live shared handle returned by
/// `olayer_terrain_engine_create`. `data` must be aligned and readable for
/// `length` bytes, with a range no larger than `isize::MAX`. Every non-null
/// output must be aligned and writable for one `f64`; outputs must not overlap
/// the input or one another.
#[no_mangle]
pub unsafe extern "C" fn olayer_terrain_engine_load_geotiff(
    engine: *mut TerrainEngine,
    data: *const u8,
    length: usize,
    out_min_lat_deg: *mut f64,
    out_min_lon_deg: *mut f64,
    out_max_lat_deg: *mut f64,
    out_max_lon_deg: *mut f64,
) -> c_int {
    if engine.is_null()
        || data.is_null()
        || length > MAX_FFI_BUFFER_BYTES
        || length > isize::MAX as usize
    {
        return -1;
    }
    let data_slice = std::slice::from_raw_parts(data, length);
    let engine_ref = &*engine;
    match engine_ref.load_geotiff_tile(data_slice) {
        Ok(bounds_rad) => {
            if !out_min_lat_deg.is_null() {
                *out_min_lat_deg = bounds_rad.0.to_degrees();
            }
            if !out_min_lon_deg.is_null() {
                *out_min_lon_deg = bounds_rad.1.to_degrees();
            }
            if !out_max_lat_deg.is_null() {
                *out_max_lat_deg = bounds_rad.2.to_degrees();
            }
            if !out_max_lon_deg.is_null() {
                *out_max_lon_deg = bounds_rad.3.to_degrees();
            }
            0
        }
        Err(_) => -2,
    }
}

/// Decodes Mapbox Terrain-RGB pixel value to elevation in meters.
///
/// # Safety
/// `out_elevation` must be non-null, aligned, and writable for one `f64`.
#[no_mangle]
pub unsafe extern "C" fn olayer_terrain_decode_mapbox_rgb(
    r: u8,
    g: u8,
    b: u8,
    out_elevation: *mut f64,
) -> c_int {
    if out_elevation.is_null() {
        return -1;
    }
    *out_elevation = olayer_core::terrain::decode_mapbox_rgb(r, g, b);
    0
}

/// Decodes Mapzen / Nextzen Terrarium RGB pixel value to elevation in meters.
///
/// # Safety
/// `out_elevation` must be non-null, aligned, and writable for one `f64`.
#[no_mangle]
pub unsafe extern "C" fn olayer_terrain_decode_terrarium_rgb(
    r: u8,
    g: u8,
    b: u8,
    out_elevation: *mut f64,
) -> c_int {
    if out_elevation.is_null() {
        return -1;
    }
    *out_elevation = olayer_core::terrain::decode_terrarium_rgb(r, g, b);
    0
}

/// Resolves elevation at coordinate degrees. Returns 0 on success, negative error.
///
/// # Safety
/// `engine` must be a live exclusive handle from `olayer_terrain_engine_create`.
/// `out_elevation` must be non-null, aligned, writable for one `f64`, and must
/// not alias the engine.
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

/// Resolves elevation at coordinate radians. Returns 0 on success, negative error.
///
/// # Safety
/// `engine` must be a live exclusive handle from `olayer_terrain_engine_create`.
/// `out_elevation` must be non-null, aligned, writable for one `f64`, and must
/// not alias the engine.
#[no_mangle]
pub unsafe extern "C" fn olayer_terrain_engine_get_elevation_rad(
    engine: *mut TerrainEngine,
    lat_rad: f64,
    lon_rad: f64,
    out_elevation: *mut f64,
) -> c_int {
    if engine.is_null() || out_elevation.is_null() {
        return -1;
    }
    let engine_ref = &mut *engine;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine_ref.get_elevation_rad(lat_rad, lon_rad)
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

/// Resolves elevation at radians while preserving DTED null samples.
/// Returns 0 for valid data, 1 for unknown elevation, or a negative error.
///
/// # Safety
/// `engine` must be a live exclusive handle from `olayer_terrain_engine_create`.
/// `out_elevation` must be non-null, aligned, writable for one `f64`, and must
/// not alias the engine.
#[no_mangle]
pub unsafe extern "C" fn olayer_terrain_engine_get_elevation_status(
    engine: *mut TerrainEngine,
    lat_rad: f64,
    lon_rad: f64,
    out_elevation: *mut f64,
) -> c_int {
    if engine.is_null() || out_elevation.is_null() {
        return -1;
    }
    let engine_ref = &mut *engine;
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine_ref.get_elevation_status(lat_rad, lon_rad)
    })) {
        Ok(Ok(sample)) => match sample.elevation_meters {
            Some(elevation) => {
                *out_elevation = elevation;
                0
            }
            None => 1,
        },
        Ok(Err(_)) => -2,
        Err(_) => -99,
    }
}

/// Resolves an object height against terrain.
/// `mode`: 0 absolute, 1 clamp-to-ground, 2 relative-to-ground, 3 relative-to-mesh.
/// `unknown_policy`: 0 reject, 1 use-absolute, 2 use-zero.
///
/// # Safety
/// `engine` must be a live exclusive handle from `olayer_terrain_engine_create`.
/// `out_height` must be non-null, aligned, writable for one `f64`, and must not
/// alias the engine.
#[no_mangle]
pub unsafe extern "C" fn olayer_terrain_engine_resolve_altitude(
    engine: *mut TerrainEngine,
    lat_rad: f64,
    lon_rad: f64,
    input_height: f64,
    mode: c_int,
    unknown_policy: c_int,
    mesh_height: f64,
    out_height: *mut f64,
) -> c_int {
    if engine.is_null() || out_height.is_null() {
        return -1;
    }
    let altitude_mode = match mode {
        0 => olayer_core::terrain::AltitudeMode::Absolute,
        1 => olayer_core::terrain::AltitudeMode::ClampToGround,
        2 => olayer_core::terrain::AltitudeMode::RelativeToGround,
        3 => olayer_core::terrain::AltitudeMode::RelativeToMesh,
        _ => return -3,
    };
    let policy = match unknown_policy {
        0 => olayer_core::terrain::AltitudeUnknownPolicy::Reject,
        1 => olayer_core::terrain::AltitudeUnknownPolicy::UseAbsolute,
        2 => olayer_core::terrain::AltitudeUnknownPolicy::UseZero,
        _ => return -3,
    };
    let mesh = if mesh_height.is_finite() {
        Some(mesh_height)
    } else {
        None
    };
    let engine_ref = &mut *engine;
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine_ref.resolve_altitude(lat_rad, lon_rad, input_height, altitude_mode, policy, mesh)
    })) {
        Ok(Ok(height)) => {
            *out_height = height;
            0
        }
        Ok(Err(_)) => -2,
        Err(_) => -99,
    }
}

/// Computes MSAW clearance. Returns 0 safe, 1 warning, 2 unknown terrain, or a negative error.
///
/// # Safety
/// `engine` must be a live exclusive handle from `olayer_terrain_engine_create`.
/// `out_clearance` must be non-null, aligned, writable for one `f64`, and must
/// not alias the engine.
#[no_mangle]
pub unsafe extern "C" fn olayer_terrain_engine_calculate_clearance(
    engine: *mut TerrainEngine,
    lat_rad: f64,
    lon_rad: f64,
    aircraft_height_meters: f64,
    minimum_clearance_meters: f64,
    reject_unknown: bool,
    out_clearance: *mut f64,
) -> c_int {
    if engine.is_null() || out_clearance.is_null() {
        return -1;
    }
    let engine_ref = &mut *engine;
    let policy = if reject_unknown {
        olayer_core::terrain::UnknownTerrainPolicy::Reject
    } else {
        olayer_core::terrain::UnknownTerrainPolicy::Propagate
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine_ref.calculate_clearance(
            lat_rad,
            lon_rad,
            aircraft_height_meters,
            minimum_clearance_meters,
            policy,
        )
    })) {
        Ok(Ok(result)) => {
            if let Some(clearance) = result.clearance_meters {
                *out_clearance = clearance;
            }
            match result.state {
                olayer_core::terrain::MsawState::Safe => 0,
                olayer_core::terrain::MsawState::Warning => 1,
                olayer_core::terrain::MsawState::Unknown => 2,
            }
        }
        Ok(Err(_)) => -2,
        Err(_) => -99,
    }
}

/// Generates a vertical profile. Fills out_profile and out_count.
/// Returns 0 on success, -1 for invalid or over-budget inputs, -2 for terrain
/// or route failures, and -99 if a panic is caught.
///
/// # Safety
/// The three route pointers must each be aligned and readable for `route_len`
/// initialized `f64` values; their ranges must not overlap each other. `engine`
/// must be a live exclusive handle from `olayer_terrain_engine_create`. Both
/// output pointers must be aligned, writable, and disjoint from the inputs and
/// each other (`out_profile` for one pointer and `out_count` for one `usize`).
/// On success, release the returned array once with its exact count using
/// `olayer_profile_points_free`.
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
    // SAFETY: The caller must provide readable route arrays of `route_len`
    // elements and writable output pointers. Arrays must not overlap outputs.
    if engine.is_null()
        || route_lat.is_null()
        || route_lon.is_null()
        || route_height.is_null()
        || out_profile.is_null()
        || out_count.is_null()
        || !(2..=MAX_ROUTE_POINTS).contains(&route_len)
        || route_len > isize::MAX as usize / std::mem::size_of::<f64>()
        || !step_meters.is_finite()
        || step_meters <= 0.0
    {
        return -1;
    }

    let mut route = Vec::with_capacity(route_len);
    for i in 0..route_len {
        // SAFETY: Each input array is documented to contain `route_len`
        // initialized f64 values, and the bounded index is within that range.
        let lat_deg = unsafe { *route_lat.add(i) };
        // SAFETY: `route_lon` has the same validated readable length.
        let lon_deg = unsafe { *route_lon.add(i) };
        // SAFETY: `route_height` has the same validated readable length.
        let height = unsafe { *route_height.add(i) };
        if !lat_deg.is_finite()
            || !(-90.0..=90.0).contains(&lat_deg)
            || !lon_deg.is_finite()
            || !(-180.0..=180.0).contains(&lon_deg)
            || !height.is_finite()
        {
            return -1;
        }
        route.push(LatLon::from_degrees(lat_deg, lon_deg, height));
    }

    let mut estimated_samples = route_len;
    for segment in route.windows(2) {
        if estimated_samples > MAX_PROFILE_SAMPLES {
            return -1;
        }
        let lat1 = segment[0].lat;
        let lat2 = segment[1].lat;
        let delta_lat = lat2 - lat1;
        let delta_lon = segment[1].lon - segment[0].lon;
        let haversine = (delta_lat * 0.5).sin().powi(2)
            + lat1.cos() * lat2.cos() * (delta_lon * 0.5).sin().powi(2);
        let angular_distance = 2.0 * haversine.clamp(0.0, 1.0).sqrt().asin();
        let segment_samples = angular_distance * 6_400_000.0 / step_meters;
        if !segment_samples.is_finite()
            || segment_samples > (MAX_PROFILE_SAMPLES - estimated_samples) as f64
        {
            return -1;
        }
        estimated_samples += segment_samples.ceil() as usize;
        if estimated_samples > MAX_PROFILE_SAMPLES {
            return -1;
        }
    }

    let engine_ref = &mut *engine;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine_ref.get_vertical_profile(&route, step_meters)
    }));

    match result {
        Ok(Ok(profile)) => {
            let c_points: Vec<C_ProfilePoint> = profile
                .into_iter()
                .map(|p| C_ProfilePoint {
                    distance_meters: p.distance_meters,
                    ground_elevation: p.ground_elevation,
                    lat: p.coords.lat.to_degrees(),
                    lon: p.coords.lon.to_degrees(),
                    height: p.coords.height,
                })
                .collect();

            let boxed_points = c_points.into_boxed_slice();
            let count = boxed_points.len();
            let ptr = Box::into_raw(boxed_points).cast::<C_ProfilePoint>();

            *out_profile = ptr;
            *out_count = count;
            0
        }
        Ok(Err(_)) => -2, // Missing tile or malformed route
        Err(_) => -99,
    }
}

/// Frees profile point array allocated by Rust.
///
/// # Safety
/// `points` must be null or the exact pointer returned by a successful
/// `olayer_terrain_engine_get_vertical_profile`; `count` must be its exact
/// element count. A non-null pointer must be aligned and freed exactly once.
#[no_mangle]
pub unsafe extern "C" fn olayer_profile_points_free(points: *mut C_ProfilePoint, count: usize) {
    if !points.is_null() {
        // SAFETY: The paired allocator returns a boxed slice and the caller must
        // pass its original pointer and element count exactly once.
        let slice = std::ptr::slice_from_raw_parts_mut(points, count);
        // SAFETY: The slice pointer preserves the original boxed-slice layout.
        drop(unsafe { Box::from_raw(slice) });
    }
}

/// Sets the terrain tile cache capacity. Returns 0 on success, negative error.
///
/// # Safety
/// `engine` must be a live exclusive handle returned by
/// `olayer_terrain_engine_create`, not freed or concurrently accessed.
#[no_mangle]
pub unsafe extern "C" fn olayer_terrain_engine_set_cache_capacity(
    engine: *mut TerrainEngine,
    capacity: usize,
) -> c_int {
    if engine.is_null() || capacity == 0 {
        return -1;
    }
    let engine_ref = &mut *engine;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine_ref.set_cache_capacity(capacity);
    }));
    match result {
        Ok(()) => 0,
        Err(_) => -99,
    }
}

/// Returns the current number of cached terrain tiles.
///
/// # Safety
/// A non-null `engine` must be a live handle returned by
/// `olayer_terrain_engine_create` and must not be concurrently mutated or freed.
#[no_mangle]
pub unsafe extern "C" fn olayer_terrain_engine_cache_size(engine: *mut TerrainEngine) -> usize {
    if engine.is_null() {
        return 0;
    }
    let engine_ref = &mut *engine;
    engine_ref.cache_size()
}

/// Clears all cached terrain tiles.
///
/// # Safety
/// A non-null `engine` must be a live exclusive handle returned by
/// `olayer_terrain_engine_create`, not freed or concurrently accessed.
#[no_mangle]
pub unsafe extern "C" fn olayer_terrain_engine_clear_cache(engine: *mut TerrainEngine) {
    if engine.is_null() {
        return;
    }
    let engine_ref = &mut *engine;
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine_ref.clear_cache();
    }));
}

/// Destroys a TerrainEngine instance.
///
/// # Safety
/// `engine` must be null or the unique, still-live pointer returned by
/// `olayer_terrain_engine_create`; it must not have been freed or be in use.
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
pub extern "C" fn olayer_interpolator_create_with_threshold(
    stale_threshold: f64,
) -> *mut InterpolationEngine {
    match InterpolationEngine::with_stale_threshold(stale_threshold) {
        Ok(engine) => Box::into_raw(Box::new(engine)),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Updates or inserts a target state. Returns 0 on success, negative error.
///
/// # Safety
/// `engine` must be a live exclusive handle returned by an interpolator
/// constructor. `id` must point to a readable, aligned, NUL-terminated byte
/// string of at most `MAX_TARGET_ID_BYTES` bytes whose bytes remain valid for
/// this call and do not overlap the engine.
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
    if id_str.len() > MAX_TARGET_ID_BYTES {
        return -4;
    }

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
///
/// # Safety
/// `engine` must be a live exclusive handle returned by an interpolator
/// constructor. `id` must point to a readable, aligned, NUL-terminated byte
/// string of at most `MAX_TARGET_ID_BYTES` bytes whose bytes remain valid for
/// this call and do not overlap the engine.
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
    if id_str.len() > MAX_TARGET_ID_BYTES {
        return -1;
    }

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
///
/// # Safety
/// `engine` must be a live exclusive handle returned by an interpolator
/// constructor. Both output pointers must be aligned, writable for one value,
/// and disjoint from each other and the engine. On success, release the returned
/// array once with its exact count using `olayer_interpolated_targets_free`.
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
                let id = match std::ffi::CString::new(t.id.as_bytes()) {
                    Ok(cstr) => cstr.into_raw(),
                    Err(_) => continue,
                };
                c_targets.push(C_InterpolatedTarget {
                    id,
                    lat: t.position.lat,
                    lon: t.position.lon,
                    height: t.position.height,
                    heading_rad: t.heading_rad,
                    quality: match t.quality {
                        olayer_core::interpolator::PredictionQuality::Valid => 0,
                        olayer_core::interpolator::PredictionQuality::Stale => 1,
                        olayer_core::interpolator::PredictionQuality::ClockSkewed => 2,
                        olayer_core::interpolator::PredictionQuality::Unavailable => 3,
                    },
                });
            }

            let boxed_targets = c_targets.into_boxed_slice();
            let count = boxed_targets.len();
            let ptr = Box::into_raw(boxed_targets).cast::<C_InterpolatedTarget>();

            *out_targets = ptr;
            *out_count = count;
            0
        }
        Ok(Err(_)) => -2, // Interpolation failed
        Err(_) => -99,
    }
}

/// Frees interpolated targets allocated by Rust.
///
/// # Safety
/// `targets` must be null or the exact pointer returned by a successful
/// `olayer_interpolator_interpolate_all`; `count` must be its exact element
/// count. Each non-null ID must still be the original returned CString pointer.
/// The array and each ID must be freed exactly once and must not have been altered.
#[no_mangle]
pub unsafe extern "C" fn olayer_interpolated_targets_free(
    targets: *mut C_InterpolatedTarget,
    count: usize,
) {
    if !targets.is_null() {
        // SAFETY: The paired allocator returns a boxed slice and the caller must
        // pass its original pointer and element count exactly once.
        let slice = std::ptr::slice_from_raw_parts_mut(targets, count);
        // SAFETY: The slice pointer preserves the original boxed-slice layout.
        let boxed_targets = unsafe { Box::from_raw(slice) };
        for t in boxed_targets {
            if !t.id.is_null() {
                // SAFETY: Every non-null ID was created by CString::into_raw in
                // the paired interpolation function and is freed exactly once.
                drop(unsafe { std::ffi::CString::from_raw(t.id) });
            }
        }
    }
}

/// Destroys an InterpolationEngine instance.
///
/// # Safety
/// `engine` must be null or the unique, still-live pointer returned by an
/// interpolator constructor; it must not have been freed or be in use.
#[no_mangle]
pub unsafe extern "C" fn olayer_interpolator_free(engine: *mut InterpolationEngine) {
    if !engine.is_null() {
        let _ = Box::from_raw(engine);
    }
}

// --- LOCAL TANGENT FRAME C-API ---

/// Creates a new `LocalTangentFrame` at the given geodetic origin.
#[no_mangle]
pub extern "C" fn olayer_local_frame_create(
    origin_lat: f64,
    origin_lon: f64,
    origin_height: f64,
) -> *mut LocalTangentFrame {
    let origin = LatLon::new(origin_lat, origin_lon, origin_height);
    Box::into_raw(Box::new(LocalTangentFrame::new(origin)))
}

/// Converts LLA to local ENU coordinates. Returns 0 on success, negative error.
///
/// # Safety
/// `frame` must be a live handle returned by `olayer_local_frame_create`.
/// `out_enu` must be aligned and writable for one `C_EnuPoint`, and must not
/// overlap the frame.
#[no_mangle]
pub unsafe extern "C" fn olayer_local_frame_lla_to_enu(
    frame: *mut LocalTangentFrame,
    lat: f64,
    lon: f64,
    height: f64,
    out_enu: *mut C_EnuPoint,
) -> c_int {
    if frame.is_null() || out_enu.is_null() {
        return -1;
    }
    let frame_ref = &*frame;
    let lla = LatLon::new(lat, lon, height);
    let pt = frame_ref.lla_to_enu(&lla);
    *out_enu = C_EnuPoint {
        east_m: pt.east_m,
        north_m: pt.north_m,
        up_m: pt.up_m,
    };
    0
}

/// Converts local ENU coordinates to LLA. Returns 0 on success, negative error.
///
/// # Safety
/// `frame` must be a live handle returned by `olayer_local_frame_create`.
/// `out_lla` must be aligned and writable for one `C_LatLon`, and must not
/// overlap the frame.
#[no_mangle]
pub unsafe extern "C" fn olayer_local_frame_enu_to_lla(
    frame: *mut LocalTangentFrame,
    east_m: f64,
    north_m: f64,
    up_m: f64,
    out_lla: *mut C_LatLon,
) -> c_int {
    if frame.is_null() || out_lla.is_null() {
        return -1;
    }
    let frame_ref = &*frame;
    let enu = EnuPoint::new(east_m, north_m, up_m);
    let lla = frame_ref.enu_to_lla(&enu);
    *out_lla = C_LatLon {
        lat: lla.lat,
        lon: lla.lon,
        height: lla.height,
    };
    0
}

/// Calculates radar look angles without atmospheric refraction. Returns 0 on success, negative error.
///
/// # Safety
/// `frame` must be a live handle returned by `olayer_local_frame_create`. Each
/// output pointer must be aligned and writable for one `f64`; they must be
/// mutually disjoint and must not overlap the frame.
#[no_mangle]
pub unsafe extern "C" fn olayer_local_frame_radar_look_angles(
    frame: *mut LocalTangentFrame,
    target_lat: f64,
    target_lon: f64,
    target_height: f64,
    out_slant_range: *mut f64,
    out_azimuth_rad: *mut f64,
    out_elevation_rad: *mut f64,
) -> c_int {
    if frame.is_null()
        || out_slant_range.is_null()
        || out_azimuth_rad.is_null()
        || out_elevation_rad.is_null()
    {
        return -1;
    }
    let frame_ref = &*frame;
    let target = LatLon::new(target_lat, target_lon, target_height);
    let (slant, az, el) = frame_ref.radar_look_angles(&target);
    *out_slant_range = slant;
    *out_azimuth_rad = az;
    *out_elevation_rad = el;
    0
}

/// Calculates radar look angles with 4/3 tropospheric refraction. Returns 0 on success, negative error.
///
/// # Safety
/// `frame` must be a live handle returned by `olayer_local_frame_create`. Each
/// output pointer must be aligned and writable for one `f64`; they must be
/// mutually disjoint and must not overlap the frame.
#[no_mangle]
pub unsafe extern "C" fn olayer_local_frame_radar_look_angles_refracted(
    frame: *mut LocalTangentFrame,
    target_lat: f64,
    target_lon: f64,
    target_height: f64,
    k_factor: f64,
    out_slant_range: *mut f64,
    out_azimuth_rad: *mut f64,
    out_elevation_rad: *mut f64,
) -> c_int {
    if frame.is_null()
        || out_slant_range.is_null()
        || out_azimuth_rad.is_null()
        || out_elevation_rad.is_null()
    {
        return -1;
    }
    let frame_ref = &*frame;
    let target = LatLon::new(target_lat, target_lon, target_height);
    let (slant, az, el) = frame_ref.radar_look_angles_refracted(&target, k_factor);
    *out_slant_range = slant;
    *out_azimuth_rad = az;
    *out_elevation_rad = el;
    0
}

/// Destroys a `LocalTangentFrame` instance.
///
/// # Safety
/// `frame` must be null or the unique, still-live pointer returned by
/// `olayer_local_frame_create`; it must not have been freed or be in use.
#[no_mangle]
pub unsafe extern "C" fn olayer_local_frame_free(frame: *mut LocalTangentFrame) {
    if !frame.is_null() {
        let _ = Box::from_raw(frame);
    }
}

// --- MAGNETIC MODEL C-API ---

/// Creates a default `MagneticModel` instance initialized with built-in WMM-2025.
#[no_mangle]
pub extern "C" fn olayer_magnetic_model_create_default() -> *mut MagneticModel {
    Box::into_raw(Box::new(MagneticModel::wmm2025()))
}

/// Creates a `MagneticModel` from a null-terminated `WMM.COF` string.
/// Returns null pointer if parsing fails or string is invalid UTF-8.
///
/// # Safety
/// `cof_str` must be null or point to an aligned, readable, NUL-terminated byte
/// string no longer than `MAX_FFI_TEXT_BYTES` bytes that remains valid for this
/// call.
#[no_mangle]
pub unsafe extern "C" fn olayer_magnetic_model_create_from_cof(
    cof_str: *const c_char,
) -> *mut MagneticModel {
    if cof_str.is_null() {
        return std::ptr::null_mut();
    }
    let c_str = match std::ffi::CStr::from_ptr(cof_str).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    if c_str.len() > MAX_FFI_TEXT_BYTES {
        return std::ptr::null_mut();
    }
    match MagneticModel::from_cof_str(c_str) {
        Ok(model) => Box::into_raw(Box::new(model)),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Computes magnetic declination using a specific `MagneticModel` instance.
///
/// # Safety
/// `model` must be a live handle returned by a magnetic-model constructor.
/// `out_declination_rad` must be aligned and writable for one `f64`, without
/// overlapping the model.
#[no_mangle]
pub unsafe extern "C" fn olayer_magnetic_model_get_declination(
    model: *mut MagneticModel,
    lat: f64,
    lon: f64,
    height: f64,
    epoch: f64,
    out_declination_rad: *mut f64,
) -> c_int {
    if model.is_null() || out_declination_rad.is_null() {
        return -1;
    }
    let model_ref = &*model;
    let pt = LatLon::new(lat, lon, height);
    *out_declination_rad = model_ref.get_declination(&pt, epoch);
    0
}

/// Computes magnetic field elements using a specific `MagneticModel` instance.
///
/// # Safety
/// `model` must be a live handle returned by a magnetic-model constructor.
/// `out_elements` must be aligned and writable for one `C_MagneticElements`,
/// without overlapping the model.
#[no_mangle]
pub unsafe extern "C" fn olayer_magnetic_model_get_elements(
    model: *mut MagneticModel,
    lat: f64,
    lon: f64,
    height: f64,
    epoch: f64,
    out_elements: *mut C_MagneticElements,
) -> c_int {
    if model.is_null() || out_elements.is_null() {
        return -1;
    }
    let model_ref = &*model;
    let pt = LatLon::new(lat, lon, height);
    let el = model_ref.get_magnetic_elements(&pt, epoch);
    *out_elements = C_MagneticElements {
        declination_rad: el.declination_rad,
        inclination_rad: el.inclination_rad,
        horizontal_intensity_nt: el.horizontal_intensity_nt,
        total_intensity_nt: el.total_intensity_nt,
        x_nt: el.x_nt,
        y_nt: el.y_nt,
        z_nt: el.z_nt,
    };
    0
}

/// Converts True bearing to Magnetic bearing using a specific `MagneticModel` instance.
///
/// # Safety
/// `model` must be a live handle returned by a magnetic-model constructor.
/// `out_mag_bearing_rad` must be aligned and writable for one `f64`, without
/// overlapping the model.
#[no_mangle]
pub unsafe extern "C" fn olayer_magnetic_model_true_to_magnetic(
    model: *mut MagneticModel,
    true_bearing_rad: f64,
    lat: f64,
    lon: f64,
    height: f64,
    epoch: f64,
    out_mag_bearing_rad: *mut f64,
) -> c_int {
    if model.is_null() || out_mag_bearing_rad.is_null() {
        return -1;
    }
    let model_ref = &*model;
    let pt = LatLon::new(lat, lon, height);
    *out_mag_bearing_rad = model_ref.true_to_magnetic(true_bearing_rad, &pt, epoch);
    0
}

/// Converts Magnetic bearing to True bearing using a specific `MagneticModel` instance.
///
/// # Safety
/// `model` must be a live handle returned by a magnetic-model constructor.
/// `out_true_bearing_rad` must be aligned and writable for one `f64`, without
/// overlapping the model.
#[no_mangle]
pub unsafe extern "C" fn olayer_magnetic_model_magnetic_to_true(
    model: *mut MagneticModel,
    mag_bearing_rad: f64,
    lat: f64,
    lon: f64,
    height: f64,
    epoch: f64,
    out_true_bearing_rad: *mut f64,
) -> c_int {
    if model.is_null() || out_true_bearing_rad.is_null() {
        return -1;
    }
    let model_ref = &*model;
    let pt = LatLon::new(lat, lon, height);
    *out_true_bearing_rad = model_ref.magnetic_to_true(mag_bearing_rad, &pt, epoch);
    0
}

/// Destroys a `MagneticModel` instance.
///
/// # Safety
/// `model` must be null or the unique, still-live pointer returned by a
/// magnetic-model constructor; it must not have been freed or be in use.
#[no_mangle]
pub unsafe extern "C" fn olayer_magnetic_model_free(model: *mut MagneticModel) {
    if !model.is_null() {
        let _ = Box::from_raw(model);
    }
}

/// Computes magnetic declination in radians using default WMM-2025. Returns 0 on success, negative error.
///
/// # Safety
/// `out_declination_rad` must be non-null, aligned, and writable for one `f64`.
#[no_mangle]
pub unsafe extern "C" fn olayer_magnetic_get_declination(
    lat: f64,
    lon: f64,
    height: f64,
    epoch: f64,
    out_declination_rad: *mut f64,
) -> c_int {
    if out_declination_rad.is_null() {
        return -1;
    }
    let pt = LatLon::new(lat, lon, height);
    *out_declination_rad = MagneticModel::get_default_declination(&pt, epoch);
    0
}

/// Computes all magnetic field elements using default WMM-2025. Returns 0 on success, negative error.
///
/// # Safety
/// `out_elements` must be non-null, aligned, and writable for one `C_MagneticElements`.
#[no_mangle]
pub unsafe extern "C" fn olayer_magnetic_get_elements(
    lat: f64,
    lon: f64,
    height: f64,
    epoch: f64,
    out_elements: *mut C_MagneticElements,
) -> c_int {
    if out_elements.is_null() {
        return -1;
    }
    let pt = LatLon::new(lat, lon, height);
    let el = MagneticModel::get_default_elements(&pt, epoch);
    *out_elements = C_MagneticElements {
        declination_rad: el.declination_rad,
        inclination_rad: el.inclination_rad,
        horizontal_intensity_nt: el.horizontal_intensity_nt,
        total_intensity_nt: el.total_intensity_nt,
        x_nt: el.x_nt,
        y_nt: el.y_nt,
        z_nt: el.z_nt,
    };
    0
}

/// Converts True bearing to Magnetic bearing using default WMM-2025. Returns 0 on success, negative error.
///
/// # Safety
/// `out_mag_bearing_rad` must be non-null, aligned, and writable for one `f64`.
#[no_mangle]
pub unsafe extern "C" fn olayer_magnetic_true_to_magnetic(
    true_bearing_rad: f64,
    lat: f64,
    lon: f64,
    height: f64,
    epoch: f64,
    out_mag_bearing_rad: *mut f64,
) -> c_int {
    if out_mag_bearing_rad.is_null() {
        return -1;
    }
    let pt = LatLon::new(lat, lon, height);
    let model = MagneticModel::wmm2025();
    *out_mag_bearing_rad = model.true_to_magnetic(true_bearing_rad, &pt, epoch);
    0
}

/// Converts Magnetic bearing to True bearing using default WMM-2025. Returns 0 on success, negative error.
///
/// # Safety
/// `out_true_bearing_rad` must be non-null, aligned, and writable for one `f64`.
#[no_mangle]
pub unsafe extern "C" fn olayer_magnetic_magnetic_to_true(
    mag_bearing_rad: f64,
    lat: f64,
    lon: f64,
    height: f64,
    epoch: f64,
    out_true_bearing_rad: *mut f64,
) -> c_int {
    if out_true_bearing_rad.is_null() {
        return -1;
    }
    let pt = LatLon::new(lat, lon, height);
    let model = MagneticModel::wmm2025();
    *out_true_bearing_rad = model.magnetic_to_true(mag_bearing_rad, &pt, epoch);
    0
}

// --- SPATIAL ANALYSIS C-API ---

/// Computes route deviation (XTK and ATD). Returns 0 on success, negative error.
///
/// # Safety
/// `out_deviation` must be non-null, aligned, and writable for one
/// `C_RouteDeviation`.
#[no_mangle]
pub unsafe extern "C" fn olayer_spatial_compute_route_deviation(
    start: C_LatLon,
    end: C_LatLon,
    pos: C_LatLon,
    out_deviation: *mut C_RouteDeviation,
) -> c_int {
    if out_deviation.is_null() {
        return -1;
    }
    let s = LatLon::new(start.lat, start.lon, start.height);
    let e = LatLon::new(end.lat, end.lon, end.height);
    let p = LatLon::new(pos.lat, pos.lon, pos.height);
    let dev = compute_route_deviation(&s, &e, &p);
    *out_deviation = C_RouteDeviation {
        cross_track_error_meters: dev.cross_track_error_meters,
        along_track_distance_meters: dev.along_track_distance_meters,
        nearest_point: C_LatLon {
            lat: dev.nearest_point_on_route.lat,
            lon: dev.nearest_point_on_route.lon,
            height: dev.nearest_point_on_route.height,
        },
    };
    0
}

/// Computes geodesic line-line intersection. Returns 1 if intersects, 0 if disjoint, negative error.
///
/// # Safety
/// `out_intersection` must be non-null, aligned, and writable for one `C_LatLon`.
#[no_mangle]
pub unsafe extern "C" fn olayer_spatial_geodesic_intersection(
    p1: C_LatLon,
    p2: C_LatLon,
    p3: C_LatLon,
    p4: C_LatLon,
    out_intersection: *mut C_LatLon,
) -> c_int {
    if out_intersection.is_null() {
        return -1;
    }
    let p1_pt = LatLon::new(p1.lat, p1.lon, p1.height);
    let p2_pt = LatLon::new(p2.lat, p2.lon, p2.height);
    let p3_pt = LatLon::new(p3.lat, p3.lon, p3.height);
    let p4_pt = LatLon::new(p4.lat, p4.lon, p4.height);
    match geodesic_intersection(&p1_pt, &p2_pt, &p3_pt, &p4_pt) {
        Some(inter) => {
            *out_intersection = C_LatLon {
                lat: inter.lat,
                lon: inter.lon,
                height: inter.height,
            };
            1
        }
        None => 0,
    }
}

/// Evaluates spherical polygon point containment. Returns 0 on success, negative error.
/// `*out_contains` is set to 1 if contained, 0 if not.
///
/// # Safety
/// `poly_coords` must be aligned and readable for `num_coords` initialized
/// `C_LatLon` elements; that range must not exceed `isize::MAX` bytes.
/// `out_contains` must be aligned and writable for one `c_int`; it must not
/// overlap the coordinate array.
#[no_mangle]
pub unsafe extern "C" fn olayer_spatial_polygon_contains_point(
    poly_coords: *const C_LatLon,
    num_coords: usize,
    point: C_LatLon,
    out_contains: *mut c_int,
) -> c_int {
    if poly_coords.is_null()
        || out_contains.is_null()
        || !(3..=MAX_ROUTE_POINTS).contains(&num_coords)
    {
        return -1;
    }
    let slice = std::slice::from_raw_parts(poly_coords, num_coords);
    let vertices: Vec<LatLon> = slice
        .iter()
        .map(|c| LatLon::new(c.lat, c.lon, c.height))
        .collect();
    let poly = GeodesicPolygon::new(vertices);
    let pt = LatLon::new(point.lat, point.lon, point.height);
    *out_contains = if poly.contains_point(&pt) { 1 } else { 0 };
    0
}

// --- TACTICAL MEASUREMENT TOOLS C-API (GIS-PROP-003) ---

/// Computes Range and Bearing Line (RBL / CRSR) measurement. Returns 0 on success, negative error.
///
/// # Safety
/// `out_measurement` must be non-null, aligned, and writable for one
/// `C_RblMeasurement`.
#[no_mangle]
pub unsafe extern "C" fn olayer_tools_compute_rbl(
    from_lat_deg: f64,
    from_lon_deg: f64,
    to_lat_deg: f64,
    to_lon_deg: f64,
    speed_knots: f64,
    epoch_year: f64,
    out_measurement: *mut C_RblMeasurement,
) -> c_int {
    if out_measurement.is_null() {
        return -1;
    }
    let manager = TacticalToolsManager::new();
    let spd = if speed_knots > 0.001 {
        Some(speed_knots)
    } else {
        None
    };
    let ep = if epoch_year > 1900.0 {
        Some(epoch_year)
    } else {
        None
    };
    let res = match manager.compute_rbl(from_lat_deg, from_lon_deg, to_lat_deg, to_lon_deg, spd, ep)
    {
        Ok(measurement) => measurement,
        Err(_) => return -2,
    };

    *out_measurement = C_RblMeasurement {
        from_lat_deg: res.from_lat_deg,
        from_lon_deg: res.from_lon_deg,
        to_lat_deg: res.to_lat_deg,
        to_lon_deg: res.to_lon_deg,
        distance_nm: res.distance_nm,
        distance_km: res.distance_km,
        true_bearing_deg: res.true_bearing_deg,
        magnetic_bearing_deg: res.magnetic_bearing_deg,
        reciprocal_true_bearing_deg: res.reciprocal_true_bearing_deg,
        reciprocal_magnetic_bearing_deg: res.reciprocal_magnetic_bearing_deg,
        estimated_time_enroute_sec: res.estimated_time_enroute_sec.unwrap_or(-1.0),
    };
    0
}

/// Generates Projected Position Leader (PPL) vector ticks. Returns 0 on success,
/// -1 for invalid pointers or empty intervals, and -2 if `num_intervals` exceeds
/// the configured input limit.
///
/// # Safety
/// `intervals_minutes` must be aligned and readable for `num_intervals`
/// initialized `f64` values; the range must not exceed `isize::MAX` bytes.
/// `out_ticks` must be aligned and writable for `max_ticks` `C_PplTick`
/// elements, and `out_ticks_written` aligned/writable for one `usize`; output
/// ranges must be disjoint from the input and each other.
#[no_mangle]
pub unsafe extern "C" fn olayer_tools_generate_ppl(
    lat_deg: f64,
    lon_deg: f64,
    ground_speed_knots: f64,
    track_deg: f64,
    intervals_minutes: *const f64,
    num_intervals: usize,
    out_ticks: *mut C_PplTick,
    max_ticks: usize,
    out_ticks_written: *mut usize,
) -> c_int {
    if intervals_minutes.is_null()
        || out_ticks.is_null()
        || out_ticks_written.is_null()
        || num_intervals == 0
    {
        return -1;
    }
    if num_intervals > MAX_FFI_ARRAY_ITEMS
        || num_intervals > isize::MAX as usize / std::mem::size_of::<f64>()
    {
        return -2;
    }
    let intervals = std::slice::from_raw_parts(intervals_minutes, num_intervals);
    let manager = TacticalToolsManager::new();
    let ppl = manager.generate_ppl(lat_deg, lon_deg, ground_speed_knots, track_deg, intervals);

    let to_copy = ppl.ticks.len().min(max_ticks);
    for (i, tick) in ppl.ticks.iter().take(to_copy).enumerate() {
        *out_ticks.add(i) = C_PplTick {
            time_minutes: tick.time_minutes,
            distance_nm: tick.distance_nm,
            lat_deg: tick.lat_deg,
            lon_deg: tick.lon_deg,
        };
    }
    *out_ticks_written = to_copy;
    0
}

/// Generates racetrack holding pattern polyline coordinates. Returns 0 on
/// success, -1 for invalid pointers, and -2 if `points_per_turn` exceeds its
/// configured generation limit.
///
/// # Safety
/// `out_coords` must be aligned and writable for `max_coords` `C_LatLon`
/// elements; `out_coords_written` must be aligned and writable for one `usize`.
/// These ranges must not overlap. `max_coords` must describe the actual output
/// allocation, even when smaller than the generated result.
#[no_mangle]
pub unsafe extern "C" fn olayer_tools_generate_holding_pattern(
    fix_lat_deg: f64,
    fix_lon_deg: f64,
    inbound_bearing_deg: f64,
    is_standard_right_turn: c_int,
    leg_time_minutes: f64,
    airspeed_knots: f64,
    points_per_turn: usize,
    out_coords: *mut C_LatLon,
    max_coords: usize,
    out_coords_written: *mut usize,
) -> c_int {
    if out_coords.is_null() || out_coords_written.is_null() || max_coords == 0 {
        return -1;
    }
    if points_per_turn > MAX_HOLDING_PATTERN_POINTS_PER_TURN {
        return -2;
    }
    let config = HoldingPatternConfig {
        fix_lat_deg,
        fix_lon_deg,
        inbound_bearing_deg,
        turn_direction: if is_standard_right_turn != 0 {
            TurnDirection::StandardRight
        } else {
            TurnDirection::NonStandardLeft
        },
        leg_time_minutes: leg_time_minutes.max(0.1),
        airspeed_knots: airspeed_knots.max(10.0),
        points_per_turn: points_per_turn.max(4),
    };
    let manager = TacticalToolsManager::new();
    let poly = manager.generate_holding_pattern(&config);

    let to_copy = poly.len().min(max_coords);
    for (i, &(lat_d, lon_d)) in poly.iter().take(to_copy).enumerate() {
        *out_coords.add(i) = C_LatLon {
            lat: lat_d.to_radians(),
            lon: lon_d.to_radians(),
            height: 0.0,
        };
    }
    *out_coords_written = to_copy;
    0
}

/// Generates ILS approach funnel polygon coordinates. Returns 0 on success,
/// -1 for invalid pointers, and -2 if `arc_steps` exceeds its configured limit.
///
/// # Safety
/// `out_polygon_coords` must be aligned and writable for `max_polygon_coords`
/// `C_LatLon` elements; `out_polygon_coords_written` must be aligned and
/// writable for one `usize`. The ranges must not overlap.
#[no_mangle]
pub unsafe extern "C" fn olayer_tools_generate_ils_cone(
    threshold_lat_deg: f64,
    threshold_lon_deg: f64,
    runway_heading_deg: f64,
    length_nm: f64,
    fov_deg: f64,
    arc_steps: usize,
    out_polygon_coords: *mut C_LatLon,
    max_polygon_coords: usize,
    out_polygon_coords_written: *mut usize,
) -> c_int {
    if out_polygon_coords.is_null()
        || out_polygon_coords_written.is_null()
        || max_polygon_coords == 0
    {
        return -1;
    }
    if arc_steps > MAX_ILS_ARC_STEPS {
        return -2;
    }
    let config = IlsConeConfig {
        threshold_lat_deg,
        threshold_lon_deg,
        runway_heading_deg,
        length_nm: length_nm.max(0.5),
        fov_deg: fov_deg.max(1.0),
        extended_centerline_nm: length_nm * 1.5,
        arc_steps: arc_steps.max(2),
    };
    let manager = TacticalToolsManager::new();
    let ils = manager.generate_ils_cone(&config);

    let to_copy = ils.cone_polygon.len().min(max_polygon_coords);
    for (i, &(lat_d, lon_d)) in ils.cone_polygon.iter().take(to_copy).enumerate() {
        *out_polygon_coords.add(i) = C_LatLon {
            lat: lat_d.to_radians(),
            lon: lon_d.to_radians(),
            height: 0.0,
        };
    }
    *out_polygon_coords_written = to_copy;
    0
}

/// Generates concentric range rings coordinates. Returns 0 on success, -1 for
/// invalid pointers, and -2 if `points_per_ring` exceeds its configured limit.
///
/// # Safety
/// `out_ring_coords` must be aligned and writable for `max_coords` `C_LatLon`
/// elements; `out_coords_written` must be aligned and writable for one `usize`.
/// The ranges must not overlap.
#[no_mangle]
pub unsafe extern "C" fn olayer_tools_generate_range_rings(
    center_lat_deg: f64,
    center_lon_deg: f64,
    radius_nm: f64,
    points_per_ring: usize,
    out_ring_coords: *mut C_LatLon,
    max_coords: usize,
    out_coords_written: *mut usize,
) -> c_int {
    if out_ring_coords.is_null() || out_coords_written.is_null() || max_coords == 0 {
        return -1;
    }
    if points_per_ring > MAX_RANGE_RING_POINTS {
        return -2;
    }
    let config = RangeRingsConfig {
        center_lat_deg,
        center_lon_deg,
        radii_nm: vec![radius_nm],
        points_per_ring: points_per_ring.max(8),
    };
    let manager = TacticalToolsManager::new();
    let rings = manager.generate_range_rings(&config);
    if rings.is_empty() {
        *out_coords_written = 0;
        return 0;
    }
    let ring = &rings[0];
    let to_copy = ring.len().min(max_coords);
    for (i, &(lat_d, lon_d)) in ring.iter().take(to_copy).enumerate() {
        *out_ring_coords.add(i) = C_LatLon {
            lat: lat_d.to_radians(),
            lon: lon_d.to_radians(),
            height: 0.0,
        };
    }
    *out_coords_written = to_copy;
    0
}

// --- AERONAUTICAL DATASET C-API ---

/// Loads an `AeronauticalDataset` from an AIXM 5.1 XML null-terminated UTF-8 string.
/// Returns null pointer on error.
///
/// # Safety
/// `xml_utf8` must be null or point to readable, aligned, NUL-terminated bytes
/// valid for this call. The complete string must be no longer than
/// `MAX_FFI_TEXT_BYTES` bytes, excluding its terminator.
#[no_mangle]
pub unsafe extern "C" fn olayer_aeronautical_dataset_from_aixm(
    xml_utf8: *const c_char,
) -> *mut AeronauticalDataset {
    if xml_utf8.is_null() {
        return std::ptr::null_mut();
    }
    let c_str = match std::ffi::CStr::from_ptr(xml_utf8).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    if c_str.len() > MAX_FFI_TEXT_BYTES {
        return std::ptr::null_mut();
    }
    match parse_aixm_51_str(c_str) {
        Ok(ds) => Box::into_raw(Box::new(ds)),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Loads an `AeronauticalDataset` from a GeoJSON-Aviation null-terminated UTF-8 string.
/// Returns null pointer on error.
///
/// # Safety
/// `json_utf8` must be null or point to readable, aligned, NUL-terminated bytes
/// valid for this call. The complete string must be no longer than
/// `MAX_FFI_TEXT_BYTES` bytes, excluding its terminator.
#[no_mangle]
pub unsafe extern "C" fn olayer_aeronautical_dataset_from_geojson(
    json_utf8: *const c_char,
) -> *mut AeronauticalDataset {
    if json_utf8.is_null() {
        return std::ptr::null_mut();
    }
    let c_str = match std::ffi::CStr::from_ptr(json_utf8).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    if c_str.len() > MAX_FFI_TEXT_BYTES {
        return std::ptr::null_mut();
    }
    match parse_geojson_aviation_str(c_str) {
        Ok(ds) => Box::into_raw(Box::new(ds)),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Returns the counts of airspaces, navaids, airways, and airports in the dataset. Returns 0 on success.
///
/// # Safety
/// `ds` must be a live handle returned by an aeronautical dataset constructor.
/// Each non-null output must be aligned and writable for one `usize`; output
/// pointers must not overlap one another or the dataset.
#[no_mangle]
pub unsafe extern "C" fn olayer_aeronautical_dataset_counts(
    ds: *mut AeronauticalDataset,
    out_airspaces: *mut usize,
    out_navaids: *mut usize,
    out_airways: *mut usize,
    out_airports: *mut usize,
) -> c_int {
    if ds.is_null() {
        return -1;
    }
    let ds_ref = &*ds;
    if !out_airspaces.is_null() {
        *out_airspaces = ds_ref.airspaces.len();
    }
    if !out_navaids.is_null() {
        *out_navaids = ds_ref.navaids.len();
    }
    if !out_airways.is_null() {
        *out_airways = ds_ref.airways.len();
    }
    if !out_airports.is_null() {
        *out_airports = ds_ref.airports.len();
    }
    0
}

/// Finds a navaid by its identification code. Returns 0 on success, -1 on null pointer, -2 if not found.
///
/// # Safety
/// `ds` must be a live dataset handle. `ident_utf8` must point to readable,
/// aligned, NUL-terminated bytes of at most `MAX_TARGET_ID_BYTES` valid for
/// this call. `out_navaid` must be aligned and writable for one
/// `C_NavaidSummary`, disjoint from both inputs.
#[no_mangle]
pub unsafe extern "C" fn olayer_aeronautical_dataset_find_navaid(
    ds: *mut AeronauticalDataset,
    ident_utf8: *const c_char,
    out_navaid: *mut C_NavaidSummary,
) -> c_int {
    if ds.is_null() || ident_utf8.is_null() || out_navaid.is_null() {
        return -1;
    }
    let ident = match std::ffi::CStr::from_ptr(ident_utf8).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    if ident.len() > MAX_TARGET_ID_BYTES {
        return -1;
    }
    let ds_ref = &*ds;
    if let Some(nav) = ds_ref.find_navaid(ident) {
        let code = match nav.navaid_type {
            NavaidType::Vor => 0,
            NavaidType::Dme => 1,
            NavaidType::VorDme => 2,
            NavaidType::Tacan => 3,
            NavaidType::Vortac => 4,
            NavaidType::Ndb => 5,
            NavaidType::Fix => 6,
            NavaidType::Waypoint => 7,
        };
        *out_navaid = C_NavaidSummary {
            lat: nav.coords.lat,
            lon: nav.coords.lon,
            elevation_m: nav.elevation_m.unwrap_or(0.0),
            frequency_mhz: nav.frequency_mhz.unwrap_or(0.0),
            navaid_type_code: code,
        };
        0
    } else {
        -2
    }
}

/// Serializes the dataset into a standard GeoJSON FeatureCollection string into a C buffer.
/// Returns 0 on success, -1 on error, or -2 if buffer capacity is insufficient.
///
/// # Safety
/// `ds` must be a live dataset handle. `out_len` must be aligned and writable
/// for one `usize`. If `out_buf` is non-null, it must be aligned and writable
/// for `out_capacity` bytes; its range must not overlap the dataset or `out_len`.
#[no_mangle]
pub unsafe extern "C" fn olayer_aeronautical_dataset_to_geojson(
    ds: *mut AeronauticalDataset,
    out_buf: *mut c_char,
    out_capacity: usize,
    out_len: *mut usize,
) -> c_int {
    if ds.is_null() || out_len.is_null() {
        return -1;
    }
    let ds_ref = &*ds;
    let json_str = match export_dataset_to_geojson(ds_ref) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let bytes = json_str.as_bytes();
    *out_len = bytes.len();
    if out_buf.is_null() || out_capacity < bytes.len() + 1 {
        return -2;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len());
    *out_buf.add(bytes.len()) = 0; // null terminator
    0
}

/// Destroys an `AeronauticalDataset` instance.
///
/// # Safety
/// `ds` must be null or the unique, still-live pointer returned by an
/// aeronautical dataset constructor; it must not have been freed or be in use.
#[no_mangle]
pub unsafe extern "C" fn olayer_aeronautical_dataset_free(ds: *mut AeronauticalDataset) {
    if !ds.is_null() {
        let _ = Box::from_raw(ds);
    }
}

// ============================================================================
// METEOROLOGICAL GIS OVERLAYS (GIS-PROP-006)
// ============================================================================

/// Opaque pointer type for SIGMET dataset in C ABI.
pub type SigmetDataset = olayer_core::weather::SigmetDataset;

/// Maps a radar reflectivity value (dBZ) to 4-byte RGBA array.
/// palette_code: 0 = Nexrad, 1 = Icao, 2 = HighContrast.
///
/// # Safety
/// `out_rgba` must be non-null, aligned, and writable for four initialized
/// `u8` slots.
#[no_mangle]
pub unsafe extern "C" fn olayer_weather_dbz_to_rgba(
    dbz: f64,
    palette_code: c_int,
    out_rgba: *mut u8,
) -> c_int {
    if out_rgba.is_null() {
        return -1;
    }
    let palette = match palette_code {
        0 => olayer_core::weather::RadarColorPalette::Nexrad,
        1 => olayer_core::weather::RadarColorPalette::Icao,
        2 => olayer_core::weather::RadarColorPalette::HighContrast,
        _ => return -2,
    };
    let rgba = olayer_core::weather::dbz_to_rgba(dbz, palette);
    std::ptr::copy_nonoverlapping(rgba.as_ptr(), out_rgba, 4);
    0
}

/// Generates aviation-standard wind barb line coordinates in degrees.
/// Writes flat lines `[start_lat, start_lon, end_lat, end_lon, ...]` into `out_lines`.
///
/// # Safety
/// `out_lines` must be non-null, aligned, and writable for `max_floats` `f64`
/// elements. `out_count` must be non-null, aligned, and writable for one
/// `usize`; the output ranges must not overlap.
#[no_mangle]
pub unsafe extern "C" fn olayer_weather_generate_wind_barb(
    origin_lat_deg: f64,
    origin_lon_deg: f64,
    speed_knots: f64,
    direction_deg: f64,
    staff_length_meters: f64,
    is_southern_hemisphere: bool,
    out_lines: *mut f64,
    max_floats: usize,
    out_count: *mut usize,
) -> c_int {
    if out_lines.is_null() || out_count.is_null() {
        return -1;
    }
    let origin = LatLon::from_degrees(origin_lat_deg, origin_lon_deg, 0.0);
    let geom = match olayer_core::weather::generate_wind_barb(
        &origin,
        speed_knots,
        direction_deg.to_radians(),
        staff_length_meters,
        is_southern_hemisphere,
    ) {
        Ok(g) => g,
        Err(_) => return -2,
    };

    let flat = olayer_core::weather::wind_barb_to_flat_lines_deg(&geom);
    *out_count = flat.len();
    if max_floats < flat.len() {
        return -3; // Buffer too small
    }
    std::ptr::copy_nonoverlapping(flat.as_ptr(), out_lines, flat.len());
    0
}

/// Generates Marching Squares 2D isolines from a scalar grid.
/// Writes flat segments `[isovalue, start_lat_deg, start_lon_deg, end_lat_deg, end_lon_deg, ...]` into `out_segments`.
///
/// # Safety
/// `grid` must be aligned and readable for `width * height` initialized `f64`
/// values and `isovalues` for `isovalues_count` initialized `f64` values. The
/// element counts are bounded and multiplication is checked before slicing.
/// `out_segments` must be aligned and writable for `max_floats` `f64` values;
/// `out_count` must be aligned and writable for one `usize`. The input and
/// output ranges must be pairwise disjoint.
#[no_mangle]
pub unsafe extern "C" fn olayer_weather_generate_isolines(
    grid: *const f64,
    width: usize,
    height: usize,
    min_lat_deg: f64,
    min_lon_deg: f64,
    max_lat_deg: f64,
    max_lon_deg: f64,
    isovalues: *const f64,
    isovalues_count: usize,
    out_segments: *mut f64,
    max_floats: usize,
    out_count: *mut usize,
) -> c_int {
    // SAFETY: The caller must provide readable grid/isovalue buffers and a
    // writable output buffer of `max_floats` values.
    if grid.is_null() || isovalues.is_null() || out_segments.is_null() || out_count.is_null() {
        return -1;
    }
    let Some(grid_len) = width.checked_mul(height) else {
        return -2;
    };
    let cell_count = width
        .saturating_sub(1)
        .checked_mul(height.saturating_sub(1));
    if width < 2
        || height < 2
        || grid_len > MAX_GRID_CELLS
        || isovalues_count == 0
        || isovalues_count > MAX_ISOVALUES
        || cell_count
            .and_then(|cells| cells.checked_mul(isovalues_count))
            .is_none_or(|work| work > MAX_ISOLINE_WORK)
        || grid_len > isize::MAX as usize / std::mem::size_of::<f64>()
        || isovalues_count > isize::MAX as usize / std::mem::size_of::<f64>()
    {
        return -2;
    }
    let grid_slice = std::slice::from_raw_parts(grid, grid_len);
    let iso_slice = std::slice::from_raw_parts(isovalues, isovalues_count);
    let bounds_rad = (
        min_lat_deg.to_radians(),
        min_lon_deg.to_radians(),
        max_lat_deg.to_radians(),
        max_lon_deg.to_radians(),
    );

    let segments = match olayer_core::weather::generate_isolines_rad(
        grid_slice, width, height, bounds_rad, iso_slice,
    ) {
        Ok(s) => s,
        Err(_) => return -2,
    };

    let flat = olayer_core::weather::isolines_to_flat_array_deg(&segments);
    *out_count = flat.len();
    if max_floats < flat.len() {
        return -3;
    }
    std::ptr::copy_nonoverlapping(flat.as_ptr(), out_segments, flat.len());
    0
}

/// Parses a GeoJSON string into a heap-allocated `SigmetDataset`.
///
/// # Safety
/// `geojson_str` must be null or point to readable, aligned, NUL-terminated
/// bytes valid for this call and no longer than `MAX_FFI_TEXT_BYTES` bytes.
#[no_mangle]
pub unsafe extern "C" fn olayer_sigmet_dataset_from_geojson(
    geojson_str: *const c_char,
) -> *mut SigmetDataset {
    if geojson_str.is_null() {
        return std::ptr::null_mut();
    }
    let c_str = match std::ffi::CStr::from_ptr(geojson_str).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    if c_str.len() > MAX_FFI_TEXT_BYTES {
        return std::ptr::null_mut();
    }
    match olayer_core::weather::SigmetDataset::from_geojson(c_str) {
        Ok(ds) => Box::into_raw(Box::new(ds)),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Returns the total number of warnings in a `SigmetDataset`.
///
/// # Safety
/// `ds` must be a live dataset handle returned by
/// `olayer_sigmet_dataset_from_geojson`. `out_count` must be aligned, writable
/// for one `usize`, and disjoint from the dataset.
#[no_mangle]
pub unsafe extern "C" fn olayer_sigmet_dataset_total_count(
    ds: *const SigmetDataset,
    out_count: *mut usize,
) -> c_int {
    if ds.is_null() || out_count.is_null() {
        return -1;
    }
    *out_count = (*ds).len();
    0
}

/// Destroys a heap-allocated `SigmetDataset`.
///
/// # Safety
/// `ds` must be null or the unique, still-live pointer returned by
/// `olayer_sigmet_dataset_from_geojson`; it must not have been freed or be in use.
#[no_mangle]
pub unsafe extern "C" fn olayer_sigmet_dataset_free(ds: *mut SigmetDataset) {
    if !ds.is_null() {
        let _ = Box::from_raw(ds);
    }
}

// ============================================================================
// 3D VOLUMETRIC AIRSPACES & TRAJECTORY RIBBONS (GIS-PROP-007)
// ============================================================================

/// Generates an extruded 3D volumetric airspace mesh.
/// Writes flat interleaved vertices `[x, y, z, nx, ny, nz, height_ratio, is_edge, ...]` into `out_vertices`
/// and triangle indices into `out_indices`.
///
/// # Safety
/// `polygon_coords` must be aligned and readable for `polygon_len` initialized
/// `C_LatLon` values. `out_vertices` must be aligned and writable for
/// `max_vertices_floats` `f32` values and `out_indices` for `max_indices` `u32`
/// values. Each count output must be aligned and writable for one `usize`.
/// Input, outputs, and the two count locations must be pairwise disjoint.
#[no_mangle]
pub unsafe extern "C" fn olayer_volumetric_generate_airspace_mesh(
    polygon_coords: *const C_LatLon,
    polygon_len: usize,
    floor_m: f64,
    ceiling_m: f64,
    out_vertices: *mut f32,
    max_vertices_floats: usize,
    out_vertices_count: *mut usize,
    out_indices: *mut u32,
    max_indices: usize,
    out_indices_count: *mut usize,
) -> c_int {
    // SAFETY: The caller must provide readable polygon coordinates and
    // writable output buffers sized by their capacities.
    if polygon_coords.is_null()
        || out_vertices.is_null()
        || out_vertices_count.is_null()
        || out_indices.is_null()
        || out_indices_count.is_null()
    {
        return -1;
    }
    if !(3..=MAX_ROUTE_POINTS).contains(&polygon_len)
        || polygon_len > isize::MAX as usize / std::mem::size_of::<C_LatLon>()
    {
        return -2;
    }

    let c_slice = std::slice::from_raw_parts(polygon_coords, polygon_len);
    let mut polygon = Vec::with_capacity(polygon_len);
    for pt in c_slice {
        polygon.push(LatLon::from_degrees(pt.lat, pt.lon, pt.height));
    }

    let mesh = match olayer_core::volumetric::generate_airspace_volume_mesh(
        &polygon, floor_m, ceiling_m,
    ) {
        Ok(m) => m,
        Err(_) => return -3,
    };

    let flat_verts = mesh.to_flat_f32_vertices();
    *out_vertices_count = flat_verts.len();
    *out_indices_count = mesh.indices.len();

    if max_vertices_floats < flat_verts.len() || max_indices < mesh.indices.len() {
        return -4; // Buffer too small
    }

    std::ptr::copy_nonoverlapping(flat_verts.as_ptr(), out_vertices, flat_verts.len());
    std::ptr::copy_nonoverlapping(mesh.indices.as_ptr(), out_indices, mesh.indices.len());
    0
}

/// Generates a continuous 3D flight trajectory ribbon mesh in ECEF coordinates.
/// Writes flat interleaved vertices `[x, y, z, nx, ny, nz, u, v, scalar, ...]` into `out_vertices`
/// and triangle indices into `out_indices`.
///
/// # Safety
/// `waypoints` must be aligned and readable for `waypoints_len` initialized
/// `C_LatLon` values. A non-null `scalars` pointer must be aligned and readable
/// for `scalars_len` initialized `f64` values. Vertex/index buffers must be
/// aligned and writable for their supplied capacities, and both count outputs
/// aligned/writable for one `usize`; all referenced ranges must be disjoint.
#[no_mangle]
pub unsafe extern "C" fn olayer_volumetric_generate_trajectory_ribbon(
    waypoints: *const C_LatLon,
    waypoints_len: usize,
    ribbon_width_m: f64,
    scalars: *const f64,
    scalars_len: usize,
    out_vertices: *mut f32,
    max_vertices_floats: usize,
    out_vertices_count: *mut usize,
    out_indices: *mut u32,
    max_indices: usize,
    out_indices_count: *mut usize,
) -> c_int {
    // SAFETY: The caller must provide readable waypoint/scalar buffers and
    // writable output buffers sized by their capacities.
    if waypoints.is_null()
        || out_vertices.is_null()
        || out_vertices_count.is_null()
        || out_indices.is_null()
        || out_indices_count.is_null()
    {
        return -1;
    }
    if !(2..=MAX_FFI_ARRAY_ITEMS).contains(&waypoints_len)
        || scalars_len > MAX_FFI_ARRAY_ITEMS
        || waypoints_len > isize::MAX as usize / std::mem::size_of::<C_LatLon>()
    {
        return -2;
    }

    let wp_slice = std::slice::from_raw_parts(waypoints, waypoints_len);
    let mut poly_wp = Vec::with_capacity(waypoints_len);
    for pt in wp_slice {
        poly_wp.push(LatLon::from_degrees(pt.lat, pt.lon, pt.height));
    }

    let scalar_opt = if !scalars.is_null() && scalars_len == waypoints_len {
        Some(std::slice::from_raw_parts(scalars, scalars_len))
    } else {
        None
    };

    let ribbon = match olayer_core::volumetric::generate_trajectory_ribbon_mesh(
        &poly_wp,
        ribbon_width_m,
        scalar_opt,
    ) {
        Ok(r) => r,
        Err(_) => return -3,
    };

    let flat_verts = ribbon.to_flat_f32_vertices();
    *out_vertices_count = flat_verts.len();
    *out_indices_count = ribbon.indices.len();

    if max_vertices_floats < flat_verts.len() || max_indices < ribbon.indices.len() {
        return -4; // Buffer too small
    }

    std::ptr::copy_nonoverlapping(flat_verts.as_ptr(), out_vertices, flat_verts.len());
    std::ptr::copy_nonoverlapping(ribbon.indices.as_ptr(), out_indices, ribbon.indices.len());
    0
}

// ============================================================================
// 8-OCTANT LABEL ANTI-CLUTTERING ENGINE (GIS-PROP-008)
// ============================================================================

/// C-compatible target descriptor for label deconfliction.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct C_LabelTarget {
    pub x: f32,
    pub y: f32,
    pub heading_rad: f32, // negative if unknown/none
    pub width: f32,
    pub height: f32,
    pub priority: u8,
}

/// C-compatible solved placement for a label.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct C_LabelPlacement {
    pub rect_x: f32,
    pub rect_y: f32,
    pub rect_width: f32,
    pub rect_height: f32,
    pub leader_start_x: f32,
    pub leader_start_y: f32,
    pub leader_end_x: f32,
    pub leader_end_y: f32,
    pub octant: u8,
    pub cost: f32,
}

/// Solves optimal 8-octant non-overlapping label placements for a batch of screen targets.
///
/// # Safety
/// `targets` must be aligned and readable for `targets_len` initialized
/// `C_LabelTarget` values. `out_placements` must be aligned and writable for
/// `max_placements` placements and `out_placements_count` aligned/writable for
/// one `usize`; output ranges must not overlap each other or the input.
#[no_mangle]
pub unsafe extern "C" fn olayer_declutter_solve_labels(
    targets: *const C_LabelTarget,
    targets_len: usize,
    leader_length_px: f32,
    safety_margin_px: f32,
    out_placements: *mut C_LabelPlacement,
    max_placements: usize,
    out_placements_count: *mut usize,
) -> c_int {
    if targets.is_null() || out_placements.is_null() || out_placements_count.is_null() {
        return -1;
    }
    if targets_len == 0 {
        *out_placements_count = 0;
        return 0;
    }
    if targets_len > MAX_LABEL_TARGETS
        || targets_len > isize::MAX as usize / std::mem::size_of::<C_LabelTarget>()
    {
        return -2;
    }
    if max_placements < targets_len {
        return -2; // Output buffer too small
    }

    let t_slice = std::slice::from_raw_parts(targets, targets_len);
    let mut core_targets = Vec::with_capacity(targets_len);

    for (i, t) in t_slice.iter().enumerate() {
        let heading_rad = if t.heading_rad >= 0.0 {
            Some(t.heading_rad)
        } else {
            None
        };
        core_targets.push(olayer_core::declutter::LabelTarget {
            id: format!("target_{i}").into(),
            x: t.x,
            y: t.y,
            heading_rad,
            width: t.width,
            height: t.height,
            priority: t.priority,
        });
    }

    let mut config = olayer_core::declutter::DeclutterConfig::default();
    if leader_length_px > 0.0 {
        config.leader_length_px = leader_length_px;
    }
    if safety_margin_px >= 0.0 {
        config.safety_margin_px = safety_margin_px;
    }

    let engine = olayer_core::declutter::DeclutterEngine::new(config);
    let solved = engine.solve(&core_targets);

    let out_slice = std::slice::from_raw_parts_mut(out_placements, targets_len);
    for (i, p) in solved.into_iter().enumerate() {
        out_slice[i] = C_LabelPlacement {
            rect_x: p.rect.x,
            rect_y: p.rect.y,
            rect_width: p.rect.width,
            rect_height: p.rect.height,
            leader_start_x: p.leader_start[0],
            leader_start_y: p.leader_start[1],
            leader_end_x: p.leader_end[0],
            leader_end_y: p.leader_end[1],
            octant: p.octant as u8,
            cost: p.cost,
        };
    }

    *out_placements_count = targets_len;
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

    struct LayoutCheckingAllocator;

    static WATCHED_ALLOCATION: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());
    static LAYOUT_MISMATCHED: AtomicBool = AtomicBool::new(false);
    const TRACKED_ALLOCATION_SLOTS: usize = 8_192;
    static ALLOCATION_POINTERS: [AtomicPtr<()>; TRACKED_ALLOCATION_SLOTS] =
        [const { AtomicPtr::new(std::ptr::null_mut()) }; TRACKED_ALLOCATION_SLOTS];
    static ALLOCATION_SIZES: [AtomicUsize; TRACKED_ALLOCATION_SLOTS] =
        [const { AtomicUsize::new(0) }; TRACKED_ALLOCATION_SLOTS];
    static ALLOCATION_ALIGNS: [AtomicUsize; TRACKED_ALLOCATION_SLOTS] =
        [const { AtomicUsize::new(0) }; TRACKED_ALLOCATION_SLOTS];
    static NEXT_ALLOCATION_SLOT: AtomicUsize = AtomicUsize::new(0);
    std::thread_local! {
        static TRACK_ALLOCATIONS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    fn track_allocation(ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        let ptr = ptr.cast::<()>();
        let slot = ALLOCATION_POINTERS
            .iter()
            .position(|candidate| candidate.load(Ordering::Acquire) == ptr)
            .unwrap_or_else(|| {
                NEXT_ALLOCATION_SLOT.fetch_add(1, Ordering::Relaxed) % TRACKED_ALLOCATION_SLOTS
            });
        ALLOCATION_POINTERS[slot].store(std::ptr::null_mut(), Ordering::Release);
        ALLOCATION_SIZES[slot].store(layout.size(), Ordering::Relaxed);
        ALLOCATION_ALIGNS[slot].store(layout.align(), Ordering::Relaxed);
        ALLOCATION_POINTERS[slot].store(ptr, Ordering::Release);
    }

    fn untrack_allocation(ptr: *mut u8) {
        let ptr = ptr.cast::<()>();
        if let Some(slot) = ALLOCATION_POINTERS
            .iter()
            .position(|candidate| candidate.load(Ordering::Acquire) == ptr)
        {
            ALLOCATION_POINTERS[slot].store(std::ptr::null_mut(), Ordering::Release);
        }
    }

    fn recorded_layout(ptr: *mut u8) -> Option<(usize, usize)> {
        let ptr = ptr.cast::<()>();
        let slot = ALLOCATION_POINTERS
            .iter()
            .position(|candidate| candidate.load(Ordering::Acquire) == ptr)?;
        Some((
            ALLOCATION_SIZES[slot].load(Ordering::Relaxed),
            ALLOCATION_ALIGNS[slot].load(Ordering::Relaxed),
        ))
    }

    // SAFETY: The implementation forwards every allocation operation unchanged
    // to `System`; its tracking side table uses only atomics and never touches
    // memory owned by the caller.
    unsafe impl GlobalAlloc for LayoutCheckingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            // SAFETY: The exact layout is forwarded unchanged to the system allocator.
            let ptr = unsafe { System.alloc(layout) };
            if TRACK_ALLOCATIONS
                .try_with(std::cell::Cell::get)
                .unwrap_or(false)
            {
                track_allocation(ptr, layout);
            }
            ptr
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            if TRACK_ALLOCATIONS
                .try_with(std::cell::Cell::get)
                .unwrap_or(false)
            {
                if ptr.cast::<()>() == WATCHED_ALLOCATION.load(Ordering::SeqCst) {
                    let expected = recorded_layout(ptr);
                    LAYOUT_MISMATCHED.store(
                        expected != Some((layout.size(), layout.align())),
                        Ordering::SeqCst,
                    );
                    WATCHED_ALLOCATION.store(std::ptr::null_mut(), Ordering::SeqCst);
                }
                untrack_allocation(ptr);
            }
            // SAFETY: `ptr` and `layout` are passed unchanged from Rust's allocator contract.
            unsafe { System.dealloc(ptr, layout) };
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            // SAFETY: The original pointer, layout, and requested size are forwarded unchanged.
            let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
            if !new_ptr.is_null()
                && TRACK_ALLOCATIONS
                    .try_with(std::cell::Cell::get)
                    .unwrap_or(false)
            {
                untrack_allocation(ptr);
                if let Ok(new_layout) = Layout::from_size_align(new_size, layout.align()) {
                    track_allocation(new_ptr, new_layout);
                }
            }
            new_ptr
        }
    }

    #[global_allocator]
    static TEST_ALLOCATOR: LayoutCheckingAllocator = LayoutCheckingAllocator;

    /// Builds a minimal mock DTED Level 0 tile (4x4) for FFI tests.
    fn create_mock_dted0(
        origin_lat: &str,
        origin_lon: &str,
        num_cols: usize,
        num_rows: usize,
    ) -> Vec<u8> {
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
    fn interpolated_target_free_uses_the_allocation_layout_after_skipping_ids() {
        TRACK_ALLOCATIONS.with(|enabled| enabled.set(true));
        let engine = olayer_interpolator_create();
        assert!(!engine.is_null());
        let good_id = std::ffi::CString::new("GOOD").unwrap();

        // SAFETY: `engine` came from the matching constructor and `good_id` is NUL-terminated.
        assert_eq!(
            unsafe {
                olayer_interpolator_update(
                    engine,
                    good_id.as_ptr(),
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                )
            },
            0
        );

        let invalid_id = "BAD\0ID".to_owned();
        let engine_ref = unsafe {
            // SAFETY: The live engine handle is exclusively accessed in this test.
            &mut *engine
        };
        assert!(engine_ref
            .update_target(TargetState {
                id: invalid_id,
                last_position: LatLon::new(0.0, 0.0, 0.0),
                speed_mps: 0.0,
                track_heading_rad: 0.0,
                vertical_rate_mps: 0.0,
                last_ping_time: 0.0,
            })
            .is_ok());

        let mut targets = std::ptr::null_mut();
        let mut count = 0;
        // SAFETY: The engine is live and both output pointers are valid and distinct.
        assert_eq!(
            unsafe { olayer_interpolator_interpolate_all(engine, 1.0, &mut targets, &mut count) },
            0
        );
        assert_eq!(count, 1);
        assert!(!targets.is_null());

        let expected_layout = recorded_layout(targets.cast::<u8>())
            .expect("the returned target allocation must be recorded");
        assert_eq!(
            expected_layout.0,
            count * std::mem::size_of::<C_InterpolatedTarget>(),
            "returned slice allocation must have the same layout as its count"
        );
        LAYOUT_MISMATCHED.store(false, Ordering::SeqCst);
        WATCHED_ALLOCATION.store(targets.cast(), Ordering::SeqCst);
        // SAFETY: This pointer/count pair is exactly the array returned above.
        unsafe { olayer_interpolated_targets_free(targets, count) };
        assert!(!LAYOUT_MISMATCHED.load(Ordering::SeqCst));

        // SAFETY: The live engine handle came from `olayer_interpolator_create` and is freed once.
        unsafe { olayer_interpolator_free(engine) };
        TRACK_ALLOCATIONS.with(|enabled| enabled.set(false));
    }

    #[test]
    fn generators_reject_unbounded_work_before_touching_output_buffers() {
        let mut written = 0;
        let mut coord = C_LatLon {
            lat: 0.0,
            lon: 0.0,
            height: 0.0,
        };
        // SAFETY: Output pointers address one writable element; large generation parameters
        // must be rejected before output is accessed.
        assert_eq!(
            unsafe {
                olayer_tools_generate_holding_pattern(
                    0.0,
                    0.0,
                    0.0,
                    1,
                    1.0,
                    100.0,
                    usize::MAX,
                    &mut coord,
                    1,
                    &mut written,
                )
            },
            -2
        );
        // SAFETY: Same valid one-element output storage; the oversized arc must be rejected first.
        assert_eq!(
            unsafe {
                olayer_tools_generate_ils_cone(
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                    1.0,
                    usize::MAX,
                    &mut coord,
                    1,
                    &mut written,
                )
            },
            -2
        );
        // SAFETY: Same valid output storage; the oversized ring must be rejected first.
        assert_eq!(
            unsafe {
                olayer_tools_generate_range_rings(
                    0.0,
                    0.0,
                    1.0,
                    usize::MAX,
                    &mut coord,
                    1,
                    &mut written,
                )
            },
            -2
        );

        let intervals = vec![1.0; MAX_FFI_ARRAY_ITEMS + 1];
        let mut tick = C_PplTick {
            time_minutes: 0.0,
            distance_nm: 0.0,
            lat_deg: 0.0,
            lon_deg: 0.0,
        };
        // SAFETY: `intervals` contains every declared readable value; the cap
        // rejects the oversized input before generation.
        assert_eq!(
            unsafe {
                olayer_tools_generate_ppl(
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    intervals.as_ptr(),
                    intervals.len(),
                    &mut tick,
                    1,
                    &mut written,
                )
            },
            -2
        );
    }

    #[test]
    fn array_work_limits_reject_oversized_polygons_grids_labels_and_profiles() {
        let mut count = 0;
        let mut index_count = 0;
        let polygon = vec![
            C_LatLon {
                lat: 0.0,
                lon: 0.0,
                height: 0.0,
            };
            MAX_ROUTE_POINTS + 1
        ];
        let mut vertex = 0.0_f32;
        let mut index = 0_u32;
        // SAFETY: The input allocation contains the declared polygon length and
        // output pointers/counters refer to distinct writable locations.
        assert_eq!(
            unsafe {
                olayer_volumetric_generate_airspace_mesh(
                    polygon.as_ptr(),
                    polygon.len(),
                    0.0,
                    1.0,
                    &mut vertex,
                    1,
                    &mut count,
                    &mut index,
                    1,
                    &mut index_count,
                )
            },
            -2
        );

        let grid = vec![0.0; MAX_GRID_CELLS + 1];
        let mut segment = 0.0;
        let iso = 0.0;
        // SAFETY: The grid allocation contains the declared elements; isovalue
        // and output pointers refer to valid separate scalar locations.
        assert_eq!(
            unsafe {
                olayer_weather_generate_isolines(
                    grid.as_ptr(),
                    MAX_GRID_CELLS + 1,
                    1,
                    0.0,
                    0.0,
                    1.0,
                    1.0,
                    &iso,
                    1,
                    &mut segment,
                    1,
                    &mut count,
                )
            },
            -2
        );

        let labels = vec![
            C_LabelTarget {
                x: 0.0,
                y: 0.0,
                heading_rad: -1.0,
                width: 1.0,
                height: 1.0,
                priority: 0,
            };
            MAX_LABEL_TARGETS + 1
        ];
        let mut placements = vec![
            C_LabelPlacement {
                rect_x: 0.0,
                rect_y: 0.0,
                rect_width: 0.0,
                rect_height: 0.0,
                leader_start_x: 0.0,
                leader_start_y: 0.0,
                leader_end_x: 0.0,
                leader_end_y: 0.0,
                octant: 0,
                cost: 0.0,
            };
            MAX_LABEL_TARGETS + 1
        ];
        // SAFETY: Both arrays contain all declared elements and the count output
        // is a separate writable usize.
        assert_eq!(
            unsafe {
                olayer_declutter_solve_labels(
                    labels.as_ptr(),
                    labels.len(),
                    1.0,
                    1.0,
                    placements.as_mut_ptr(),
                    placements.len(),
                    &mut count,
                )
            },
            -2
        );

        let engine = olayer_terrain_engine_create();
        let route_lat = [-22.9, -22.8];
        let route_lon = [-48.0, -48.0];
        let route_height = [0.0, 0.0];
        let mut profile = std::ptr::null_mut();
        // SAFETY: All route arrays have two valid elements, the engine is live,
        // and output pointers are valid and distinct. The tiny step is rejected
        // by the sample budget before terrain sampling.
        assert_eq!(
            unsafe {
                olayer_terrain_engine_get_vertical_profile(
                    engine,
                    route_lat.as_ptr(),
                    route_lon.as_ptr(),
                    route_height.as_ptr(),
                    route_lat.len(),
                    1.0e-9,
                    &mut profile,
                    &mut count,
                )
            },
            -1
        );
        // SAFETY: This engine is the unique live handle created immediately above.
        unsafe { olayer_terrain_engine_free(engine) };
    }

    #[test]
    fn test_c_ffi_interpolator_flow() {
        // SAFETY: Every handle is created and freed by its matching API, all
        // strings and outputs are local valid storage, and returned arrays use
        // their exact pointer/count pairs for release.
        unsafe {
            let engine = olayer_interpolator_create();
            assert!(!engine.is_null());

            let id_str = std::ffi::CString::new("FL123").unwrap();
            let update_res = olayer_interpolator_update(
                engine,
                id_str.as_ptr(),
                -0.41,   // lat
                -0.81,   // lon
                10000.0, // height
                250.0,   // speed
                1.57,    // heading
                0.0,     // vertical rate
                1000.0,  // time
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
        // SAFETY: The engine and input/output buffers are locally owned and
        // satisfy each called API's documented length and aliasing requirements.
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
        // SAFETY: Non-null pointers reference live local storage; null pointers
        // are intentionally supplied only to APIs that validate them first.
        unsafe {
            // Terrain create is the only one that doesn't take a pointer
            let engine = olayer_terrain_engine_create();
            assert!(!engine.is_null());

            // Null engine pointers
            let fake_data = [0u8; 100];
            let mut out_lat = 0;
            let mut out_lon = 0;
            assert_eq!(
                olayer_terrain_engine_load_tile(
                    std::ptr::null_mut(),
                    fake_data.as_ptr(),
                    10,
                    &mut out_lat,
                    &mut out_lon,
                ),
                -1
            );
            assert_eq!(
                olayer_terrain_engine_unload_tile(std::ptr::null_mut(), 0, 0,),
                -1
            );
            let mut elev = 0.0;
            assert_eq!(
                olayer_terrain_engine_get_elevation(std::ptr::null_mut(), 0.0, 0.0, &mut elev,),
                -1
            );
            assert_eq!(
                olayer_terrain_engine_get_elevation(engine, 0.0, 0.0, std::ptr::null_mut(),),
                -1
            );

            // Null route pointers for vertical profile
            let mut out_profile: *mut C_ProfilePoint = std::ptr::null_mut();
            let mut count: usize = 0;
            assert_eq!(
                olayer_terrain_engine_get_vertical_profile(
                    engine,
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    100.0,
                    &mut out_profile,
                    &mut count,
                ),
                -1
            );

            // Null interpolator pointers
            let id = std::ffi::CString::new("X").unwrap();
            let mut ptr: *mut C_InterpolatedTarget = std::ptr::null_mut();
            let mut cnt: usize = 0;
            assert_eq!(
                olayer_interpolator_update(
                    std::ptr::null_mut(),
                    id.as_ptr(),
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                ),
                -1
            );
            assert_eq!(
                olayer_interpolator_update(
                    engine as *mut InterpolationEngine,
                    std::ptr::null(),
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                ),
                -1
            );
            assert_eq!(
                olayer_interpolator_remove(std::ptr::null_mut(), id.as_ptr(),),
                -1
            );
            assert_eq!(
                olayer_interpolator_interpolate_all(std::ptr::null_mut(), 0.0, &mut ptr, &mut cnt,),
                -1
            );
            assert_eq!(
                olayer_interpolator_interpolate_all(
                    engine as *mut InterpolationEngine,
                    0.0,
                    std::ptr::null_mut(),
                    &mut cnt,
                ),
                -1
            );

            olayer_terrain_engine_free(engine);
        }
    }

    #[test]
    fn test_c_ffi_terrain_load_and_query() {
        // SAFETY: The engine handle, DTED byte buffer, and scalar outputs remain
        // valid and non-overlapping throughout every call in this test.
        unsafe {
            let engine = olayer_terrain_engine_create();
            assert!(!engine.is_null());

            let mock = create_mock_dted0("230000S", "0480000W", 4, 4);
            let mut out_lat = 0;
            let mut out_lon = 0;
            let load_res = olayer_terrain_engine_load_tile(
                engine,
                mock.as_ptr(),
                mock.len(),
                &mut out_lat,
                &mut out_lon,
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
                engine,
                -23.0 + 1.0 / 3.0,
                -48.0 + 1.0 / 3.0,
                &mut elev2,
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
        // SAFETY: Route arrays have matching lengths; output storage and engine
        // handle are valid, and the returned profile is freed with its count.
        unsafe {
            let engine = olayer_terrain_engine_create();
            assert!(!engine.is_null());

            let mock = create_mock_dted0("230000S", "0480000W", 121, 121);
            let mut out_lat = 0;
            let mut out_lon = 0;
            let load_res = olayer_terrain_engine_load_tile(
                engine,
                mock.as_ptr(),
                mock.len(),
                &mut out_lat,
                &mut out_lon,
            );
            assert_eq!(load_res, 0);

            // Route inside the tile
            let route_lat = [-22.9_f64, -22.9];
            let route_lon = [-48.0_f64, -47.9];
            let route_height = [0.0_f64, 0.0];
            let mut out_profile: *mut C_ProfilePoint = std::ptr::null_mut();
            let mut count: usize = 0;

            TRACK_ALLOCATIONS.with(|enabled| enabled.set(true));
            let prof_res = olayer_terrain_engine_get_vertical_profile(
                engine,
                route_lat.as_ptr(),
                route_lon.as_ptr(),
                route_height.as_ptr(),
                2,
                2000.0,
                &mut out_profile,
                &mut count,
            );
            assert_eq!(prof_res, 0);
            assert!(count >= 2);
            assert!(!out_profile.is_null());

            let first = &*out_profile;
            assert!((first.lat - -22.9).abs() < 1e-5);
            assert!((first.lon - -48.0).abs() < 1e-5);

            let profile_layout = recorded_layout(out_profile.cast::<u8>())
                .expect("the returned profile allocation must be recorded");
            assert_eq!(
                profile_layout.0,
                count * std::mem::size_of::<C_ProfilePoint>()
            );
            olayer_profile_points_free(out_profile, count);
            TRACK_ALLOCATIONS.with(|enabled| enabled.set(false));
            olayer_terrain_engine_free(engine);
        }
    }

    #[test]
    fn test_c_ffi_interpolator_remove() {
        // SAFETY: The engine and ID string are live; every output is valid and
        // any returned array is paired with its API-specific release function.
        unsafe {
            let engine = olayer_interpolator_create();
            assert!(!engine.is_null());

            let id = std::ffi::CString::new("REMOVE_ME").unwrap();
            let update = olayer_interpolator_update(
                engine,
                id.as_ptr(),
                0.0,
                0.0,
                100.0,
                10.0,
                0.0,
                0.0,
                0.0,
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
        // SAFETY: The engine is live and uniquely borrowed for the direct Rust
        // insertion; all FFI inputs and returned outputs have valid lifetimes.
        unsafe {
            let engine = olayer_interpolator_create();
            assert!(!engine.is_null());

            // Create a target with a normal ID
            let id_ok = std::ffi::CString::new("OK").unwrap();
            let r1 = olayer_interpolator_update(
                engine,
                id_ok.as_ptr(),
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
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
        // SAFETY: The byte input includes a terminator and remains readable; the
        // engine handle comes from the matching constructor and is freed once.
        unsafe {
            let engine = olayer_interpolator_create();
            assert!(!engine.is_null());

            // Invalid UTF-8 sequence
            let bad_bytes = [0x80u8, 0x81, 0x82, 0x00];
            let r = olayer_interpolator_update(
                engine,
                bad_bytes.as_ptr() as *const c_char,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
            );
            assert_eq!(r, -3, "Should reject invalid UTF-8 ID");

            olayer_interpolator_free(engine);
        }
    }

    #[test]
    fn test_c_ffi_local_frame() {
        // SAFETY: The frame comes from its constructor, and every output pointer
        // addresses a distinct writable local value until the frame is freed.
        unsafe {
            let frame = olayer_local_frame_create(0.0, 0.0, 0.0);
            assert!(!frame.is_null());

            let mut out_enu = C_EnuPoint {
                east_m: 0.0,
                north_m: 0.0,
                up_m: 0.0,
            };
            let r1 = olayer_local_frame_lla_to_enu(frame, 0.01, 0.01, 100.0, &mut out_enu);
            assert_eq!(r1, 0);
            assert!(out_enu.east_m > 1000.0);

            let mut out_lla = C_LatLon {
                lat: 0.0,
                lon: 0.0,
                height: 0.0,
            };
            let r2 = olayer_local_frame_enu_to_lla(
                frame,
                out_enu.east_m,
                out_enu.north_m,
                out_enu.up_m,
                &mut out_lla,
            );
            assert_eq!(r2, 0);
            assert!((out_lla.lat - 0.01).abs() < 1e-6);

            let mut slant = 0.0;
            let mut az = 0.0;
            let mut el = 0.0;
            let r3 = olayer_local_frame_radar_look_angles(
                frame, 0.01, 0.01, 100.0, &mut slant, &mut az, &mut el,
            );
            assert_eq!(r3, 0);
            assert!(slant > 1000.0);

            let r4 = olayer_local_frame_radar_look_angles_refracted(
                frame, 0.01, 0.01, 100.0, 1.333, &mut slant, &mut az, &mut el,
            );
            assert_eq!(r4, 0);

            olayer_local_frame_free(frame);
        }
    }

    #[test]
    fn test_c_ffi_magnetic_model() {
        // SAFETY: Both model handles and the COF string are constructor-produced;
        // all output pointers address writable local values and handles are freed once.
        unsafe {
            let mut declination = 0.0;
            let r1 = olayer_magnetic_get_declination(
                40.64_f64.to_radians(),
                -73.78_f64.to_radians(),
                0.0,
                2025.0,
                &mut declination,
            );
            assert_eq!(r1, 0);
            let dec_deg = declination.to_degrees();
            assert!(dec_deg > -15.0 && dec_deg < -10.0);

            let mut elements = C_MagneticElements {
                declination_rad: 0.0,
                inclination_rad: 0.0,
                horizontal_intensity_nt: 0.0,
                total_intensity_nt: 0.0,
                x_nt: 0.0,
                y_nt: 0.0,
                z_nt: 0.0,
            };
            let r2 = olayer_magnetic_get_elements(
                40.64_f64.to_radians(),
                -73.78_f64.to_radians(),
                0.0,
                2025.0,
                &mut elements,
            );
            assert_eq!(r2, 0);
            assert_eq!(elements.declination_rad, declination);
            assert!(elements.total_intensity_nt > 40000.0);

            let mut mag_bearing = 0.0;
            let r3 = olayer_magnetic_true_to_magnetic(
                1.0,
                40.64_f64.to_radians(),
                -73.78_f64.to_radians(),
                0.0,
                2025.0,
                &mut mag_bearing,
            );
            assert_eq!(r3, 0);

            let mut true_bearing = 0.0;
            let r4 = olayer_magnetic_magnetic_to_true(
                mag_bearing,
                40.64_f64.to_radians(),
                -73.78_f64.to_radians(),
                0.0,
                2025.0,
                &mut true_bearing,
            );
            assert_eq!(r4, 0);
            assert!((true_bearing - 1.0).abs() < 1e-10);

            // Test dynamic MagneticModel instance creation and COF parsing
            let default_model = olayer_magnetic_model_create_default();
            assert!(!default_model.is_null());
            let mut inst_dec = 0.0;
            let r5 = olayer_magnetic_model_get_declination(
                default_model,
                40.64_f64.to_radians(),
                -73.78_f64.to_radians(),
                0.0,
                2025.0,
                &mut inst_dec,
            );
            assert_eq!(r5, 0);
            assert_eq!(inst_dec, declination);
            olayer_magnetic_model_free(default_model);

            let mock_cof = std::ffi::CString::new(
                "2030.0 WMM-2030 11/20/2029\n1 0 -29396.6 0.0 11.6 0.0\n1 1 -1404.9 4589.6 12.3 -23.4",
            ).unwrap();
            let custom_model = olayer_magnetic_model_create_from_cof(mock_cof.as_ptr());
            assert!(!custom_model.is_null());
            let mut custom_dec = 0.0;
            let r6 = olayer_magnetic_model_get_declination(
                custom_model,
                40.64_f64.to_radians(),
                -73.78_f64.to_radians(),
                0.0,
                2030.0,
                &mut custom_dec,
            );
            assert_eq!(r6, 0);
            olayer_magnetic_model_free(custom_model);
        }
    }

    #[test]
    fn test_c_ffi_spatial_analysis() {
        // SAFETY: The polygon slice and all output values are valid local storage
        // with no overlap for every synchronous call.
        unsafe {
            let start = C_LatLon {
                lat: 0.0,
                lon: 0.0,
                height: 0.0,
            };
            let end = C_LatLon {
                lat: 0.0,
                lon: 0.1,
                height: 0.0,
            };
            let pos = C_LatLon {
                lat: 0.01,
                lon: 0.05,
                height: 0.0,
            };
            let mut dev = C_RouteDeviation {
                cross_track_error_meters: 0.0,
                along_track_distance_meters: 0.0,
                nearest_point: C_LatLon {
                    lat: 0.0,
                    lon: 0.0,
                    height: 0.0,
                },
            };
            let r1 = olayer_spatial_compute_route_deviation(start, end, pos, &mut dev);
            assert_eq!(r1, 0);
            assert!(dev.cross_track_error_meters < 0.0); // North of Eastbound track

            let p1 = C_LatLon {
                lat: 0.0,
                lon: -0.1,
                height: 0.0,
            };
            let p2 = C_LatLon {
                lat: 0.0,
                lon: 0.1,
                height: 0.0,
            };
            let p3 = C_LatLon {
                lat: -0.1,
                lon: 0.0,
                height: 0.0,
            };
            let p4 = C_LatLon {
                lat: 0.1,
                lon: 0.0,
                height: 0.0,
            };
            let mut inter = C_LatLon {
                lat: 0.0,
                lon: 0.0,
                height: 0.0,
            };
            let r2 = olayer_spatial_geodesic_intersection(p1, p2, p3, p4, &mut inter);
            assert_eq!(r2, 1);
            assert!(inter.lat.abs() < 1e-6);
            assert!(inter.lon.abs() < 1e-6);

            let poly_coords = [
                C_LatLon {
                    lat: 0.0,
                    lon: 0.0,
                    height: 0.0,
                },
                C_LatLon {
                    lat: 0.0,
                    lon: 0.1,
                    height: 0.0,
                },
                C_LatLon {
                    lat: 0.1,
                    lon: 0.1,
                    height: 0.0,
                },
                C_LatLon {
                    lat: 0.1,
                    lon: 0.0,
                    height: 0.0,
                },
            ];
            let mut contains = 0;
            let pt_in = C_LatLon {
                lat: 0.05,
                lon: 0.05,
                height: 0.0,
            };
            let r3 = olayer_spatial_polygon_contains_point(
                poly_coords.as_ptr(),
                poly_coords.len(),
                pt_in,
                &mut contains,
            );
            assert_eq!(r3, 0);
            assert_eq!(contains, 1);

            let pt_out = C_LatLon {
                lat: 0.5,
                lon: 0.5,
                height: 0.0,
            };
            let r4 = olayer_spatial_polygon_contains_point(
                poly_coords.as_ptr(),
                poly_coords.len(),
                pt_out,
                &mut contains,
            );
            assert_eq!(r4, 0);
            assert_eq!(contains, 0);
        }
    }

    #[test]
    fn test_c_ffi_tactical_tools() {
        // SAFETY: Each output buffer is sized for the requested capacity and all
        // scalar inputs are local; no returned pointer outlives this test.
        unsafe {
            // RBL
            let mut rbl = C_RblMeasurement {
                from_lat_deg: 0.0,
                from_lon_deg: 0.0,
                to_lat_deg: 0.0,
                to_lon_deg: 0.0,
                distance_nm: 0.0,
                distance_km: 0.0,
                true_bearing_deg: 0.0,
                magnetic_bearing_deg: 0.0,
                reciprocal_true_bearing_deg: 0.0,
                reciprocal_magnetic_bearing_deg: 0.0,
                estimated_time_enroute_sec: 0.0,
            };
            let r1 =
                olayer_tools_compute_rbl(40.64, -73.78, 42.36, -71.01, 450.0, 2025.0, &mut rbl);
            assert_eq!(r1, 0);
            assert!(rbl.distance_nm > 150.0 && rbl.distance_nm < 185.0);
            assert!(rbl.estimated_time_enroute_sec > 1000.0);

            // PPL
            let intervals = [1.0, 2.0, 5.0];
            let mut ticks = [C_PplTick {
                time_minutes: 0.0,
                distance_nm: 0.0,
                lat_deg: 0.0,
                lon_deg: 0.0,
            }; 3];
            let mut written = 0;
            let r2 = olayer_tools_generate_ppl(
                40.0,
                -74.0,
                480.0,
                90.0,
                intervals.as_ptr(),
                intervals.len(),
                ticks.as_mut_ptr(),
                3,
                &mut written,
            );
            assert_eq!(r2, 0);
            assert_eq!(written, 3);
            assert!((ticks[0].distance_nm - 8.0).abs() < 1e-3);

            // Holding Pattern
            let mut holding_coords = [C_LatLon {
                lat: 0.0,
                lon: 0.0,
                height: 0.0,
            }; 64];
            let mut holding_written = 0;
            let r3 = olayer_tools_generate_holding_pattern(
                51.5,
                -0.1,
                270.0,
                1,
                1.0,
                210.0,
                16,
                holding_coords.as_mut_ptr(),
                64,
                &mut holding_written,
            );
            assert_eq!(r3, 0);
            assert!(holding_written >= 34);

            // ILS Cone
            let mut ils_coords = [C_LatLon {
                lat: 0.0,
                lon: 0.0,
                height: 0.0,
            }; 32];
            let mut ils_written = 0;
            let r4 = olayer_tools_generate_ils_cone(
                51.4775,
                -0.4614,
                270.0,
                10.0,
                5.0,
                12,
                ils_coords.as_mut_ptr(),
                32,
                &mut ils_written,
            );
            assert_eq!(r4, 0);
            assert!(ils_written >= 14);

            // Range Rings
            let mut ring_coords = [C_LatLon {
                lat: 0.0,
                lon: 0.0,
                height: 0.0,
            }; 64];
            let mut ring_written = 0;
            let r5 = olayer_tools_generate_range_rings(
                0.0,
                0.0,
                10.0,
                36,
                ring_coords.as_mut_ptr(),
                64,
                &mut ring_written,
            );
            assert_eq!(r5, 0);
            assert_eq!(ring_written, 37);
        }
    }

    #[test]
    fn test_c_ffi_aeronautical_dataset() {
        // SAFETY: Dataset and C-string pointers are constructor/local values;
        // output buffers have the declared capacity and the dataset is freed once.
        unsafe {
            let geojson = std::ffi::CString::new(
                r#"{
                "type": "FeatureCollection",
                "features": [
                    {
                        "type": "Feature",
                        "properties": {
                            "aero_type": "Navaid",
                            "ident": "LON",
                            "name": "LONDON VOR",
                            "navaid_type": "VOR",
                            "frequency_mhz": 113.6
                        },
                        "geometry": {
                            "type": "Point",
                            "coordinates": [-0.46, 51.47, 25.0]
                        }
                    }
                ]
            }"#,
            )
            .unwrap();

            let ds = olayer_aeronautical_dataset_from_geojson(geojson.as_ptr());
            assert!(!ds.is_null());

            let mut asp = 0;
            let mut nav = 0;
            let mut rtes = 0;
            let mut apts = 0;
            let r1 =
                olayer_aeronautical_dataset_counts(ds, &mut asp, &mut nav, &mut rtes, &mut apts);
            assert_eq!(r1, 0);
            assert_eq!(asp, 0);
            assert_eq!(nav, 1);

            let ident = std::ffi::CString::new("LON").unwrap();
            let mut nav_summary = C_NavaidSummary {
                lat: 0.0,
                lon: 0.0,
                elevation_m: 0.0,
                frequency_mhz: 0.0,
                navaid_type_code: -1,
            };
            let r2 = olayer_aeronautical_dataset_find_navaid(ds, ident.as_ptr(), &mut nav_summary);
            assert_eq!(r2, 0);
            assert_eq!(nav_summary.navaid_type_code, 0); // VOR = 0
            assert_eq!(nav_summary.frequency_mhz, 113.6);

            let mut out_buf = vec![0 as c_char; 2048];
            let mut out_len = 0;
            let r3 = olayer_aeronautical_dataset_to_geojson(
                ds,
                out_buf.as_mut_ptr(),
                2048,
                &mut out_len,
            );
            assert_eq!(r3, 0);
            assert!(out_len > 50);

            olayer_aeronautical_dataset_free(ds);
        }
    }

    #[test]
    fn test_c_ffi_civil_terrain() {
        // SAFETY: Every engine handle and input byte slice is local and valid;
        // scalar outputs are writable and do not alias the engine.
        unsafe {
            let mut elev_mb = 0.0;
            let r1 = olayer_terrain_decode_mapbox_rgb(1, 134, 160, &mut elev_mb);
            assert_eq!(r1, 0);
            assert!((elev_mb - 0.0).abs() < 0.1);

            let mut elev_terr = 0.0;
            let r2 = olayer_terrain_decode_terrarium_rgb(128, 0, 0, &mut elev_terr);
            assert_eq!(r2, 0);
            assert!((elev_terr - 0.0).abs() < 1e-6);

            let engine = olayer_terrain_engine_create();
            assert!(!engine.is_null());

            let mut rgba_buf = Vec::new();
            for _ in 0..4 {
                // 500m in Mapbox RGB: [1, 154, 40, 255]
                rgba_buf.extend_from_slice(&[1, 154, 40, 255]);
            }

            let r3 = olayer_terrain_engine_load_rgb_tile(
                engine,
                10,
                512,
                512,
                0, // 0 = MapboxRgb
                rgba_buf.as_ptr(),
                rgba_buf.len(),
                2,
                2,
            );
            assert_eq!(r3, 0);

            let mut out_elev = 0.0;
            let r4 = olayer_terrain_engine_get_elevation(engine, -0.05, 0.05, &mut out_elev);
            assert_eq!(r4, 0);
            assert!((out_elev - 500.0).abs() < 0.2);

            olayer_terrain_engine_free(engine);
        }
    }

    #[test]
    fn test_c_ffi_weather_overlays() {
        // SAFETY: Grid and isovalue arrays are readable for their declared sizes,
        // outputs have sufficient capacity, and the dataset is freed exactly once.
        unsafe {
            // 1. dBZ to RGBA
            let mut rgba = [0u8; 4];
            let r1 = olayer_weather_dbz_to_rgba(45.0, 0, rgba.as_mut_ptr());
            assert_eq!(r1, 0);
            assert_eq!(rgba[0], 253); // Red-Orange

            // 2. Wind barb
            let mut lines = [0.0; 64];
            let mut count = 0;
            let r2 = olayer_weather_generate_wind_barb(
                51.5,
                -0.1,
                65.0,
                90.0,
                1000.0,
                false,
                lines.as_mut_ptr(),
                64,
                &mut count,
            );
            assert_eq!(r2, 0);
            assert!(count >= 12);

            // 3. Isolines
            let grid = [0.0, 20.0, 40.0, 60.0];
            let isos = [30.0];
            let mut segs = [0.0; 32];
            let mut seg_count = 0;
            let r3 = olayer_weather_generate_isolines(
                grid.as_ptr(),
                2,
                2,
                0.0,
                0.0,
                1.0,
                1.0,
                isos.as_ptr(),
                1,
                segs.as_mut_ptr(),
                32,
                &mut seg_count,
            );
            assert_eq!(r3, 0);
            assert_eq!(seg_count, 5);

            // 4. SIGMET dataset
            let geojson = std::ffi::CString::new(r#"{
                "type": "FeatureCollection",
                "features": [
                    {
                        "type": "Feature",
                        "properties": { "id": "SIG1", "hazard": "TS", "severity": "SEV" },
                        "geometry": {
                            "type": "Polygon",
                            "coordinates": [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.0]]]
                        }
                    }
                ]
            }"#).unwrap();

            let ds = olayer_sigmet_dataset_from_geojson(geojson.as_ptr());
            assert!(!ds.is_null());

            let mut total = 0;
            let r4 = olayer_sigmet_dataset_total_count(ds, &mut total);
            assert_eq!(r4, 0);
            assert_eq!(total, 1);

            olayer_sigmet_dataset_free(ds);
        }
    }

    #[test]
    fn test_c_ffi_volumetric_overlays() {
        // SAFETY: Polygon/waypoint/label inputs and vertex/index/output arrays
        // have the lengths passed to the ABI functions and do not overlap.
        unsafe {
            // 1. Volumetric Airspace Mesh
            let polygon = [
                C_LatLon {
                    lat: 51.0,
                    lon: -0.5,
                    height: 0.0,
                },
                C_LatLon {
                    lat: 51.0,
                    lon: 0.5,
                    height: 0.0,
                },
                C_LatLon {
                    lat: 51.5,
                    lon: 0.5,
                    height: 0.0,
                },
                C_LatLon {
                    lat: 51.5,
                    lon: -0.5,
                    height: 0.0,
                },
            ];

            let mut out_verts = [0.0f32; 512];
            let mut out_verts_count = 0;
            let mut out_indices = [0u32; 512];
            let mut out_indices_count = 0;

            let r1 = olayer_volumetric_generate_airspace_mesh(
                polygon.as_ptr(),
                polygon.len(),
                1000.0,
                5000.0,
                out_verts.as_mut_ptr(),
                512,
                &mut out_verts_count,
                out_indices.as_mut_ptr(),
                512,
                &mut out_indices_count,
            );
            assert_eq!(r1, 0);
            assert_eq!(out_verts_count, 28 * 8);
            assert_eq!(out_indices_count, 36);

            // 2. Trajectory Ribbon Mesh
            let waypoints = [
                C_LatLon {
                    lat: 40.0,
                    lon: -74.0,
                    height: 1000.0,
                },
                C_LatLon {
                    lat: 40.5,
                    lon: -73.5,
                    height: 5000.0,
                },
                C_LatLon {
                    lat: 41.0,
                    lon: -73.0,
                    height: 10000.0,
                },
            ];

            let mut out_ribbon_verts = [0.0f32; 256];
            let mut out_ribbon_verts_count = 0;
            let mut out_ribbon_indices = [0u32; 256];
            let mut out_ribbon_indices_count = 0;

            let r2 = olayer_volumetric_generate_trajectory_ribbon(
                waypoints.as_ptr(),
                waypoints.len(),
                200.0,
                std::ptr::null(),
                0,
                out_ribbon_verts.as_mut_ptr(),
                256,
                &mut out_ribbon_verts_count,
                out_ribbon_indices.as_mut_ptr(),
                256,
                &mut out_ribbon_indices_count,
            );
            assert_eq!(r2, 0);
            assert_eq!(out_ribbon_verts_count, 6 * 9);
            assert_eq!(out_ribbon_indices_count, 12);

            // 3. Label Anti-Cluttering
            let targets = [
                C_LabelTarget {
                    x: 100.0,
                    y: 100.0,
                    heading_rad: -1.0,
                    width: 50.0,
                    height: 20.0,
                    priority: 0,
                },
                C_LabelTarget {
                    x: 105.0,
                    y: 105.0,
                    heading_rad: -1.0,
                    width: 50.0,
                    height: 20.0,
                    priority: 1,
                },
            ];

            let mut placements = [C_LabelPlacement {
                rect_x: 0.0,
                rect_y: 0.0,
                rect_width: 0.0,
                rect_height: 0.0,
                leader_start_x: 0.0,
                leader_start_y: 0.0,
                leader_end_x: 0.0,
                leader_end_y: 0.0,
                octant: 0,
                cost: 0.0,
            }; 2];
            let mut placements_count = 0;

            let r3 = olayer_declutter_solve_labels(
                targets.as_ptr(),
                2,
                25.0,
                2.0,
                placements.as_mut_ptr(),
                2,
                &mut placements_count,
            );
            assert_eq!(r3, 0);
            assert_eq!(placements_count, 2);
            assert_ne!(placements[0].octant, placements[1].octant);
        }
    }
}
