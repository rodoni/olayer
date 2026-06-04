use wasm_bindgen::prelude::*;
use olayer_core::geodesy::LatLon;
use olayer_core::terrain::TerrainEngine;
use olayer_core::interpolator::{InterpolationEngine, TargetState};

/// WASM compatible wrapper for LatLon geodetic coordinates.
#[wasm_bindgen]
pub struct WasmLatLon {
    pub lat: f64,
    pub lon: f64,
    pub height: f64,
}

#[wasm_bindgen]
impl WasmLatLon {
    #[wasm_bindgen(constructor)]
    pub fn new(lat: f64, lon: f64, height: f64) -> WasmLatLon {
        WasmLatLon { lat, lon, height }
    }
}

/// WASM wrapper for a parsed DTED tile key.
#[wasm_bindgen]
pub struct WasmTileKey {
    pub lat_deg: i32,
    pub lon_deg: i32,
}

#[wasm_bindgen]
impl WasmTileKey {
    #[wasm_bindgen(constructor)]
    pub fn new(lat_deg: i32, lon_deg: i32) -> WasmTileKey {
        WasmTileKey { lat_deg, lon_deg }
    }
}

/// WASM wrapper for TerrainEngine.
#[wasm_bindgen]
pub struct WasmTerrainEngine {
    inner: TerrainEngine,
}

#[wasm_bindgen]
impl WasmTerrainEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmTerrainEngine {
        WasmTerrainEngine {
            inner: TerrainEngine::new(),
        }
    }

    /// Loads a raw DTED buffer slice and registers the resulting tile.
    /// Returns the parsed tile origin coordinates on success.
    pub fn load_tile(&mut self, data: &[u8]) -> Result<WasmTileKey, JsValue> {
        self.inner.load_tile(data)
            .map(|key| WasmTileKey { lat_deg: key.lat_deg, lon_deg: key.lon_deg })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Unloads a tile by its coordinate degrees.
    pub fn unload_tile(&mut self, lat_deg: i32, lon_deg: i32) -> bool {
        let key = olayer_core::terrain::engine::TileKey { lat_deg, lon_deg };
        self.inner.unload_tile(&key)
    }

    /// Returns the interpolated elevation at coordinate degrees.
    pub fn get_elevation(&self, lat_deg: f64, lon_deg: f64) -> Result<f64, JsValue> {
        self.inner.get_elevation(lat_deg, lon_deg)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Generates a vertical terrain profile along a sequence of route points.
    /// Route coordinates must be passed as a flat array of [lat0, lon0, height0, lat1, lon1, height1, ...] in **degrees**.
    /// Returns a flat array of profile points [distance0, elevation0, lat0, lon0, height0, ...] in **degrees**.
    pub fn get_vertical_profile(&self, route_coords: &[f64], step_meters: f64) -> Result<Vec<f64>, JsValue> {
        let route: Vec<LatLon> = route_coords.chunks_exact(3)
            .map(|c| LatLon::from_degrees(c[0], c[1], c[2]))
            .collect();

        let profile = self.inner.get_vertical_profile(&route, step_meters)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        // Flatten the result: 5 elements per point (distance, elevation, lat, lon, height)
        let mut flat = Vec::with_capacity(profile.len() * 5);
        for p in profile {
            flat.push(p.distance_meters);
            flat.push(p.ground_elevation);
            flat.push(p.coords.lat.to_degrees());
            flat.push(p.coords.lon.to_degrees());
            flat.push(p.coords.height);
        }
        Ok(flat)
    }
}

/// WASM wrapper for InterpolationEngine.
#[wasm_bindgen]
pub struct WasmInterpolationEngine {
    inner: InterpolationEngine,
}

#[wasm_bindgen]
impl WasmInterpolationEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmInterpolationEngine {
        WasmInterpolationEngine {
            inner: InterpolationEngine::new(),
        }
    }

    /// Creates a new WasmInterpolationEngine with a custom stale threshold in seconds.
    pub fn with_stale_threshold(stale_threshold: f64) -> WasmInterpolationEngine {
        WasmInterpolationEngine {
            inner: InterpolationEngine::with_stale_threshold(stale_threshold),
        }
    }

    /// Inserts or updates a target state.
    /// Coordinates are expected in **radians** (lat, lon), altitude in metres.
    pub fn update_target(
        &mut self,
        id: &str,
        lat_rad: f64,
        lon_rad: f64,
        height: f64,
        speed_mps: f64,
        track_heading_rad: f64,
        vertical_rate_mps: f64,
        last_ping_time: f64,
    ) -> Result<(), JsValue> {
        let state = TargetState {
            id: id.to_string(),
            last_position: LatLon::new(lat_rad, lon_rad, height),
            speed_mps,
            track_heading_rad,
            vertical_rate_mps,
            last_ping_time,
        };
        self.inner.update_target(state)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Removes a target by its identifier.
    pub fn remove_target(&mut self, id: &str) -> bool {
        self.inner.remove_target(id)
    }

    /// Interpolates positions of all active targets and returns the serialized JSON value.
    pub fn interpolate_all(&self, current_time: f64) -> Result<JsValue, JsValue> {
        let targets = self.inner.interpolate_all(current_time)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        serde_wasm_bindgen::to_value(&targets)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn test_wasm_latlon() {
        let coord = WasmLatLon::new(0.41, -0.81, 100.0);
        assert_eq!(coord.lat, 0.41);
        assert_eq!(coord.lon, -0.81);
        assert_eq!(coord.height, 100.0);
    }

    #[wasm_bindgen_test]
    fn test_wasm_interpolator_flow() {
        let mut engine = WasmInterpolationEngine::new();
        let update_res = engine.update_target(
            "FL456",
            -0.41, // lat (radians)
            -0.81, // lon (radians)
            5000.0,
            200.0,
            0.0,
            0.0,
            2000.0,
        );
        assert!(update_res.is_ok());

        let targets_val = engine.interpolate_all(2010.0);
        assert!(targets_val.is_ok());
        
        let removed = engine.remove_target("FL456");
        assert!(removed);
    }

    #[wasm_bindgen_test]
    fn test_wasm_interpolator_with_threshold() {
        let mut engine = WasmInterpolationEngine::with_stale_threshold(15.0);
        let update_res = engine.update_target(
            "TGT1", 0.0, 0.0, 100.0, 10.0, 0.0, 0.0, 100.0,
        );
        assert!(update_res.is_ok());

        // At t = 110.0 (dt = 10.0s <= 15.0s), target should be present
        let t1 = engine.interpolate_all(110.0);
        assert!(t1.is_ok());
        let val1 = t1.unwrap();
        let arr1 = js_sys::Array::from(&val1);
        assert_eq!(arr1.length(), 1);

        // At t = 120.0 (dt = 20.0s > 15.0s), target is stale
        let t2 = engine.interpolate_all(120.0);
        assert!(t2.is_ok());
        let val2 = t2.unwrap();
        let arr2 = js_sys::Array::from(&val2);
        assert_eq!(arr2.length(), 0);
    }

    #[wasm_bindgen_test]
    fn test_wasm_terrain_error_handling() {
        let engine = WasmTerrainEngine::new();
        let elev_res = engine.get_elevation(-23.0, -46.0);
        assert!(elev_res.is_err());
    }

    /// Builds a minimal mock DTED Level 0 tile (4x4) for WASM tests.
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

    #[wasm_bindgen_test]
    fn test_wasm_terrain_load_and_query() {
        let mut engine = WasmTerrainEngine::new();
        let mock = create_mock_dted0("230000S", "0480000W", 4, 4);

        let key = engine.load_tile(&mock);
        assert!(key.is_ok());
        let k = key.unwrap();
        assert_eq!(k.lat_deg, -23);
        assert_eq!(k.lon_deg, -48);

        // Query southwest corner (origin)
        let elev = engine.get_elevation(-23.0, -48.0);
        assert!(elev.is_ok());
        assert!((elev.unwrap() - 0.0).abs() < 1e-6);

        // Unload and verify
        let existed = engine.unload_tile(-23, -48);
        assert!(existed);

        let elev2 = engine.get_elevation(-23.0, -48.0);
        assert!(elev2.is_err());
    }

    #[wasm_bindgen_test]
    fn test_wasm_terrain_vertical_profile() {
        let mut engine = WasmTerrainEngine::new();
        let mock = create_mock_dted0("230000S", "0480000W", 121, 121);
        let key = engine.load_tile(&mock);
        assert!(key.is_ok());

        // Route in degrees: from (-22.9, -48.0) to (-22.9, -47.9)
        let route = [
            -22.9_f64, -48.0, 0.0,
            -22.9_f64, -47.9, 0.0,
        ];

        let profile = engine.get_vertical_profile(&route, 2000.0);
        assert!(profile.is_ok());
        let flat = profile.unwrap();
        assert!(flat.len() >= 10); // at least 2 points * 5 fields

        // First point lat/lon should be approximately -22.9, -48.0
        let first_lat = flat[2];
        let first_lon = flat[3];
        assert!((first_lat - -22.9).abs() < 1e-5);
        assert!((first_lon - -48.0).abs() < 1e-5);
    }
}
