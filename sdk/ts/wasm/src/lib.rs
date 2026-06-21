#![allow(clippy::missing_safety_doc)]
#![allow(clippy::too_many_arguments)]

use wasm_bindgen::prelude::*;
use olayer_core::geodesy::LatLon;
use olayer_core::geodesy::ellipsoid::Ellipsoid;
use olayer_core::terrain::TerrainEngine;
use std::sync::Arc;
use olayer_core::interpolator::{InterpolationEngine, TargetState};
use olayer_core::projections::{LambertConformalConic, WebMercator, Stereographic, Projection, CameraState};
use olayer_core::sld::StyleRegistry;
use olayer_core::symbol_registry::{SymbolRegistry, providers::DeclarativeProvider};


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
        let key = olayer_core::terrain::TileKey { lat_deg, lon_deg };
        self.inner.unload_tile(&key)
    }

    /// Returns the interpolated elevation at coordinate degrees.
    pub fn get_elevation(&self, lat_deg: f64, lon_deg: f64) -> Result<f64, JsValue> {
        self.inner.get_elevation(lat_deg, lon_deg)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Returns the interpolated elevation at coordinate radians.
    pub fn get_elevation_rad(&self, lat_rad: f64, lon_rad: f64) -> Result<f64, JsValue> {
        self.inner.get_elevation_rad(lat_rad, lon_rad)
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

    /// Sets the maximum number of DTED tiles to keep in memory.
    ///
    /// # Errors
    ///
    /// Returns an error if `capacity` is zero.
    pub fn set_cache_capacity(&self, capacity: usize) -> Result<(), JsValue> {
        if capacity == 0 {
            return Err(JsValue::from_str("terrain tile cache capacity must be non-zero"));
        }
        self.inner.set_cache_capacity(capacity);
        Ok(())
    }

    /// Returns the current number of cached tiles.
    pub fn cache_size(&self) -> usize {
        self.inner.cache_size()
    }

    /// Clears all cached tiles.
    pub fn clear_cache(&self) {
        self.inner.clear_cache();
    }
}

impl Default for WasmTerrainEngine {
    fn default() -> Self {
        Self::new()
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
            id: Arc::from(id),
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

impl Default for WasmInterpolationEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// WASM compatible camera parameters.
#[wasm_bindgen]
pub struct WasmCameraState {
    pub center_lat: f64, // radians
    pub center_lon: f64, // radians
    pub center_height: f64, // meters
    pub zoom: f64,
    pub rotation: f64, // radians
    pub pitch: f64, // radians
    pub roll: f64, // radians
    pub aspect_ratio: f64,
    pub viewport_base_meters: f64,
}

#[wasm_bindgen]
impl WasmCameraState {
    #[wasm_bindgen(constructor)]
    pub fn new(
        center_lat: f64,
        center_lon: f64,
        center_height: f64,
        zoom: f64,
        rotation: f64,
        pitch: f64,
        roll: f64,
        aspect_ratio: f64,
        viewport_base_meters: f64,
    ) -> WasmCameraState {
        WasmCameraState {
            center_lat,
            center_lon,
            center_height,
            zoom,
            rotation,
            pitch,
            roll,
            aspect_ratio,
            viewport_base_meters,
        }
    }
}

#[wasm_bindgen]
pub enum WasmProjectionType {
    Lcc,
    Stereographic,
    WebMercator,
}

/// WASM wrapper to compute map projections and View-Projection matrices.
#[wasm_bindgen]
pub struct WasmProjection {
    projection_type: WasmProjectionType,
    lcc_std_par1: f64,
    lcc_std_par2: f64,
    lcc_origin_lat: f64,
    lcc_origin_lon: f64,
    stereo_center_lat: f64,
    stereo_center_lon: f64,
    version: u32,
}

#[wasm_bindgen]
impl WasmProjection {
    #[wasm_bindgen]
    pub fn new_lcc(std_par1: f64, std_par2: f64, origin_lat: f64, origin_lon: f64) -> WasmProjection {
        WasmProjection {
            projection_type: WasmProjectionType::Lcc,
            lcc_std_par1: std_par1,
            lcc_std_par2: std_par2,
            lcc_origin_lat: origin_lat,
            lcc_origin_lon: origin_lon,
            stereo_center_lat: 0.0,
            stereo_center_lon: 0.0,
            version: 0,
        }
    }

    #[wasm_bindgen]
    pub fn new_stereographic(center_lat: f64, center_lon: f64) -> WasmProjection {
        WasmProjection {
            projection_type: WasmProjectionType::Stereographic,
            lcc_std_par1: 0.0,
            lcc_std_par2: 0.0,
            lcc_origin_lat: 0.0,
            lcc_origin_lon: 0.0,
            stereo_center_lat: center_lat,
            stereo_center_lon: center_lon,
            version: 0,
        }
    }

    #[wasm_bindgen]
    pub fn new_web_mercator() -> WasmProjection {
        WasmProjection {
            projection_type: WasmProjectionType::WebMercator,
            lcc_std_par1: 0.0,
            lcc_std_par2: 0.0,
            lcc_origin_lat: 0.0,
            lcc_origin_lon: 0.0,
            stereo_center_lat: 0.0,
            stereo_center_lon: 0.0,
            version: 0,
        }
    }

    #[wasm_bindgen]
    pub fn version(&self) -> u32 {
        self.version
    }

    #[wasm_bindgen]
    pub fn update_center(&mut self, center_lat: f64, center_lon: f64) {
        self.stereo_center_lat = center_lat;
        self.stereo_center_lon = center_lon;
        self.lcc_origin_lat = center_lat;
        self.lcc_origin_lon = center_lon;
        self.version += 1;
    }

    fn get_projection(&self) -> Box<dyn Projection> {
        match self.projection_type {
            WasmProjectionType::Lcc => {
                let lcc = LambertConformalConic::new(
                    self.lcc_std_par1,
                    self.lcc_std_par2,
                    self.lcc_origin_lat,
                    self.lcc_origin_lon,
                    Ellipsoid::wgs84(),
                );
                Box::new(lcc)
            }
            WasmProjectionType::Stereographic => {
                let stereo = Stereographic::new(
                    self.stereo_center_lat,
                    self.stereo_center_lon,
                    Ellipsoid::wgs84(),
                );
                Box::new(stereo)
            }
            WasmProjectionType::WebMercator => {
                Box::new(WebMercator::new(Ellipsoid::wgs84()))
            }
        }
    }

    /// Projects geodetic coordinates to planar meters [x, y].
    pub fn project(&self, lat_rad: f64, lon_rad: f64, height: f64) -> Result<Vec<f64>, JsValue> {
        let proj = self.get_projection();
        let lla = LatLon::new(lat_rad, lon_rad, height);
        proj.project(&lla)
            .map(|(x, y)| vec![x, y])
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Unprojects planar meters (x, y) to geodetic coordinates.
    pub fn unproject(&self, x: f64, y: f64) -> Result<WasmLatLon, JsValue> {
        let proj = self.get_projection();
        proj.unproject(x, y)
            .map(|lla| WasmLatLon { lat: lla.lat, lon: lla.lon, height: lla.height })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Generates a flat 4x4 View-Projection matrix [f32; 16].
    pub fn get_view_proj_matrix(&self, camera: &WasmCameraState) -> Result<Vec<f32>, JsValue> {
        let proj = self.get_projection();
        let cam = CameraState::with_attitude(
            LatLon::new(camera.center_lat, camera.center_lon, camera.center_height),
            camera.zoom,
            camera.rotation,
            camera.pitch,
            camera.roll,
            camera.aspect_ratio,
            camera.viewport_base_meters,
        );
        cam.get_2d_view_proj_matrix(proj.as_ref())
            .map(|m| m.to_vec())
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Generates a flat 4x4 Perspective View-Projection matrix for 3D globe visualization.
    pub fn get_3d_view_proj_matrix(&self, camera: &WasmCameraState) -> Result<Vec<f32>, JsValue> {
        let cam = CameraState::with_attitude(
            LatLon::new(camera.center_lat, camera.center_lon, camera.center_height),
            camera.zoom,
            camera.rotation,
            camera.pitch,
            camera.roll,
            camera.aspect_ratio,
            camera.viewport_base_meters,
        );
        cam.get_3d_view_proj_matrix()
            .map(|m| m.to_vec())
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Generates a flat 4x4 Perspective View-Projection matrix for a 2.5D tilted flat map.
    pub fn get_25d_view_proj_matrix(&self, camera: &WasmCameraState) -> Result<Vec<f32>, JsValue> {
        let proj = self.get_projection();
        let cam = CameraState::with_attitude(
            LatLon::new(camera.center_lat, camera.center_lon, camera.center_height),
            camera.zoom,
            camera.rotation,
            camera.pitch,
            camera.roll,
            camera.aspect_ratio,
            camera.viewport_base_meters,
        );
        cam.get_25d_view_proj_matrix(proj.as_ref())
            .map(|m| m.to_vec())
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

/// Converts Geodetic LLA coordinates to ECEF 3D Cartesian coordinates [X, Y, Z] in meters.
#[wasm_bindgen]
pub fn lla_to_ecef(lat_rad: f64, lon_rad: f64, height: f64) -> Vec<f64> {
    let lla = LatLon::new(lat_rad, lon_rad, height);
    let ecef = olayer_core::geodesy::lla_to_ecef(&lla, &Ellipsoid::wgs84());
    vec![ecef.x, ecef.y, ecef.z]
}

/// Converts ECEF 3D Cartesian coordinates (X, Y, Z) in meters to Geodetic LLA coordinates.
#[wasm_bindgen]
pub fn ecef_to_lla(x: f64, y: f64, z: f64) -> WasmLatLon {
    let ecef = olayer_core::geodesy::coords::Ecef::new(x, y, z);
    let lla = olayer_core::geodesy::ecef_to_lla(&ecef, &Ellipsoid::wgs84());
    WasmLatLon {
        lat: lla.lat,
        lon: lla.lon,
        height: lla.height,
    }
}

#[wasm_bindgen]
pub struct WasmStyleRegistry {
    pub(crate) inner: StyleRegistry,
}

#[wasm_bindgen]
impl WasmStyleRegistry {
    #[wasm_bindgen]
    pub fn parse(xml: &str) -> Result<WasmStyleRegistry, JsValue> {
        olayer_core::sld::parser::parse(xml)
            .map(|inner| WasmStyleRegistry { inner })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

#[wasm_bindgen]
pub struct WasmSymbolRegistry {
    inner: SymbolRegistry,
}

#[wasm_bindgen]
impl WasmSymbolRegistry {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmSymbolRegistry {
        WasmSymbolRegistry {
            inner: SymbolRegistry::new(),
        }
    }

    #[wasm_bindgen]
    pub fn register_declarative_provider(&mut self, json_content: &str) -> Result<(), JsValue> {
        let provider = DeclarativeProvider::from_json(json_content)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.inner.register_provider(Box::new(provider));
        Ok(())
    }

    #[wasm_bindgen]
    pub fn resolve_symbol(&self, code: &str, style: &WasmStyleRegistry) -> Result<JsValue, JsValue> {
        let resolved = self.inner.resolve_symbol(code, &style.inner)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        serde_wasm_bindgen::to_value(&resolved)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

impl Default for WasmSymbolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn test_wasm_style_and_symbol_registry() {
        let sld = r#"<?xml version="1.0" encoding="UTF-8"?>
        <StyledLayerDescriptor version="1.0.0">
            <NamedLayer>
                <Name>civil:vor</Name>
                <UserStyle>
                    <FeatureTypeStyle>
                        <Rule>
                            <PointSymbolizer>
                                <Graphic>
                                    <Mark>
                                        <Fill>
                                            <CssParameter name="fill">#FF00FF</CssParameter>
                                        </Fill>
                                    </Mark>
                                </Graphic>
                            </PointSymbolizer>
                        </Rule>
                    </FeatureTypeStyle>
                </UserStyle>
            </NamedLayer>
        </StyledLayerDescriptor>"#;

        let style = WasmStyleRegistry::parse(sld).unwrap();

        let json = r#"{
            "library_name": "TestLib",
            "symbols": {
                "civil:vor": {
                    "bbox": [-10.0, -10.0, 10.0, 10.0],
                    "anchor": [0.0, 0.0],
                    "primitives": [
                        {
                            "type": "Circle",
                            "cx": 0.0,
                            "cy": 0.0,
                            "r": 5.0,
                            "fill": { "r": 255, "g": 255, "b": 255, "a": 255 }
                        }
                    ]
                }
            }
        }"#;

        let mut registry = WasmSymbolRegistry::new();
        registry.register_declarative_provider(json).unwrap();

        let resolved_val = registry.resolve_symbol("civil:vor", &style).unwrap();
        assert!(!resolved_val.is_null() && !resolved_val.is_undefined());
    }

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

    #[wasm_bindgen_test]
    fn test_wasm_tile_key() {
        let key = WasmTileKey::new(-23, -48);
        assert_eq!(key.lat_deg, -23);
        assert_eq!(key.lon_deg, -48);
    }

    #[wasm_bindgen_test]
    fn test_wasm_camera_state() {
        let cam = WasmCameraState::new(
            0.41, -0.81, 1000.0, 2.0, 0.5, 0.35, 0.0, 1.6, 250000.0,
        );
        assert_eq!(cam.center_lat, 0.41);
        assert_eq!(cam.center_lon, -0.81);
        assert_eq!(cam.center_height, 1000.0);
        assert_eq!(cam.zoom, 2.0);
        assert_eq!(cam.rotation, 0.5);
        assert_eq!(cam.pitch, 0.35);
        assert_eq!(cam.roll, 0.0);
        assert_eq!(cam.aspect_ratio, 1.6);
        assert_eq!(cam.viewport_base_meters, 250000.0);
    }

    #[wasm_bindgen_test]
    fn test_wasm_projection_stereographic() {
        let proj = WasmProjection::new_stereographic(0.0, 0.0);
        let xy = proj.project(0.0, 0.0, 0.0);
        assert!(xy.is_ok());
        let coords = xy.unwrap();
        assert!(coords.len() == 2);
        assert!(coords[0].abs() < 1e-6);
        assert!(coords[1].abs() < 1e-6);

        let lla = proj.unproject(0.0, 0.0);
        assert!(lla.is_ok());
        let lla = lla.unwrap();
        assert!((lla.lat - 0.0).abs() < 1e-6);
        assert!((lla.lon - 0.0).abs() < 1e-6);
    }

    #[wasm_bindgen_test]
    fn test_wasm_projection_lcc() {
        let proj = WasmProjection::new_lcc(
            -20.0_f64.to_radians(), -25.0_f64.to_radians(),
            -23.0_f64.to_radians(), -46.0_f64.to_radians(),
        );
        let xy = proj.project(-23.0_f64.to_radians(), -46.0_f64.to_radians(), 0.0);
        assert!(xy.is_ok());
        let coords = xy.unwrap();
        assert!(coords.len() == 2);
        // Center point should project near (0, 0)
        assert!(coords[0].abs() < 1e-3);
        assert!(coords[1].abs() < 1e-3);
    }

    #[wasm_bindgen_test]
    fn test_wasm_projection_web_mercator() {
        let proj = WasmProjection::new_web_mercator();
        let xy = proj.project(0.0, 0.0, 0.0);
        assert!(xy.is_ok());
        let coords = xy.unwrap();
        assert!(coords.len() == 2);
        assert!(coords[0].abs() < 1e-6);
        assert!(coords[1].abs() < 1e-6);
    }

    #[wasm_bindgen_test]
    fn test_wasm_projection_2d_view_proj_matrix() {
        let proj = WasmProjection::new_stereographic(0.0, 0.0);
        let cam = WasmCameraState::new(
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 100000.0,
        );
        let m = proj.get_view_proj_matrix(&cam);
        assert!(m.is_ok());
        let flat = m.unwrap();
        assert_eq!(flat.len(), 16);
    }

    #[wasm_bindgen_test]
    fn test_wasm_projection_3d_view_proj_matrix() {
        let proj = WasmProjection::new_stereographic(0.0, 0.0);
        let cam = WasmCameraState::new(
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 100000.0,
        );
        let m = proj.get_3d_view_proj_matrix(&cam);
        assert!(m.is_ok());
        let flat = m.unwrap();
        assert_eq!(flat.len(), 16);
    }

    #[wasm_bindgen_test]
    fn test_wasm_projection_25d_view_proj_matrix() {
        let proj = WasmProjection::new_web_mercator();
        let cam = WasmCameraState::new(
            0.0, 0.0, 0.0, 1.0, 0.0, 0.35, 0.0, 1.0, 100000.0,
        );
        let m = proj.get_25d_view_proj_matrix(&cam);
        assert!(m.is_ok());
        let flat = m.unwrap();
        assert_eq!(flat.len(), 16);
    }

    #[wasm_bindgen_test]
    fn test_wasm_lla_to_ecef() {
        let ecef = lla_to_ecef(0.0, 0.0, 0.0);
        assert_eq!(ecef.len(), 3);
        // At equator, prime meridian, sea level: x = R_earth, y = 0, z = 0
        assert!((ecef[0] - 6378137.0).abs() < 1.0);
        assert!(ecef[1].abs() < 1.0);
        assert!(ecef[2].abs() < 1.0);
    }

    #[wasm_bindgen_test]
    fn test_wasm_ecef_to_lla() {
        let lla = ecef_to_lla(6378137.0, 0.0, 0.0);
        assert!((lla.lat - 0.0).abs() < 1e-6);
        assert!((lla.lon - 0.0).abs() < 1e-6);
        assert!((lla.height - 0.0).abs() < 1.0);
    }

    #[wasm_bindgen_test]
    fn test_wasm_ecef_to_lla_roundtrip() {
        let lat = 0.41;
        let lon = -0.81;
        let height = 1000.0;
        let ecef = lla_to_ecef(lat, lon, height);
        let lla = ecef_to_lla(ecef[0], ecef[1], ecef[2]);
        assert!((lla.lat - lat).abs() < 1e-6);
        assert!((lla.lon - lon).abs() < 1e-6);
        assert!((lla.height - height).abs() < 1e-3);
    }

    #[wasm_bindgen_test]
    fn test_wasm_interpolator_multiple_targets() {
        let mut engine = WasmInterpolationEngine::new();
        for i in 0..5 {
            let id = format!("TGT{}", i);
            let res = engine.update_target(
                &id,
                0.0 + i as f64 * 0.01,
                0.0,
                1000.0 + i as f64 * 100.0,
                100.0,
                0.0,
                0.0,
                0.0,
            );
            assert!(res.is_ok());
        }

        let val = engine.interpolate_all(5.0);
        assert!(val.is_ok());
        let arr = js_sys::Array::from(&val.unwrap());
        assert_eq!(arr.length(), 5);
    }

    #[wasm_bindgen_test]
    fn test_wasm_interpolator_remove_nonexistent() {
        let mut engine = WasmInterpolationEngine::new();
        let removed = engine.remove_target("NOEXIST");
        assert!(!removed);
    }

    #[wasm_bindgen_test]
    fn test_wasm_terrain_unload_nonexistent() {
        let mut engine = WasmTerrainEngine::new();
        let existed = engine.unload_tile(0, 0);
        assert!(!existed);
    }

    #[wasm_bindgen_test]
    fn test_wasm_terrain_multiple_tiles() {
        let mut engine = WasmTerrainEngine::new();
        let mock1 = create_mock_dted0("230000S", "0480000W", 4, 4);
        let mock2 = create_mock_dted0("240000S", "0480000W", 4, 4);

        let k1 = engine.load_tile(&mock1);
        assert!(k1.is_ok());
        assert_eq!(k1.unwrap().lat_deg, -23);

        let k2 = engine.load_tile(&mock2);
        assert!(k2.is_ok());
        assert_eq!(k2.unwrap().lat_deg, -24);

        let e1 = engine.get_elevation(-23.0, -48.0);
        assert!(e1.is_ok());
        let e2 = engine.get_elevation(-24.0, -48.0);
        assert!(e2.is_ok());

        engine.unload_tile(-23, -48);
        let e1_removed = engine.get_elevation(-23.0, -48.0);
        assert!(e1_removed.is_err());
        let e2_still = engine.get_elevation(-24.0, -48.0);
        assert!(e2_still.is_ok());
    }

    #[wasm_bindgen_test]
    fn test_wasm_terrain_vertical_profile_error_no_tile() {
        let engine = WasmTerrainEngine::new();
        let route = [-23.0_f64, -46.0, 0.0, -23.0, -45.0, 0.0];
        let profile = engine.get_vertical_profile(&route, 2000.0);
        assert!(profile.is_err());
    }

    #[wasm_bindgen_test]
    fn test_wasm_projection_type_roundtrip() {
        let proj = WasmProjection::new_stereographic(0.0, 0.0);
        let cam = WasmCameraState::new(
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 100000.0,
        );
        let m = proj.get_view_proj_matrix(&cam);
        assert!(m.is_ok());
        let flat = m.unwrap();
        // Verify it's a valid matrix (last element of 4x4 should be 1 for orthographic)
        assert!(flat[15].abs() > 0.0);
    }

    #[wasm_bindgen_test]
    fn test_wasm_latlon_default() {
        let coord = WasmLatLon::new(0.0, 0.0, 0.0);
        assert_eq!(coord.lat, 0.0);
        assert_eq!(coord.lon, 0.0);
        assert_eq!(coord.height, 0.0);
    }

    #[wasm_bindgen_test]
    fn test_wasm_terrain_elevation_boundary() {
        let mut engine = WasmTerrainEngine::new();
        let mock = create_mock_dted0("230000S", "0480000W", 4, 4);
        engine.load_tile(&mock).unwrap();

        // Test boundary coordinates
        let elev = engine.get_elevation(-23.0, -48.0);
        assert!(elev.is_ok());
        // Northeast corner
        let elev_ne = engine.get_elevation(-22.0, -47.0);
        assert!(elev_ne.is_ok());
    }
}

// ============================================================================
// Additional non-wasm unit tests (run with cargo test, no browser needed)
// ============================================================================

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_wasm_latlon_pure() {
        let coord = WasmLatLon::new(0.41, -0.81, 100.0);
        assert_eq!(coord.lat, 0.41);
        assert_eq!(coord.lon, -0.81);
        assert_eq!(coord.height, 100.0);
    }

    #[test]
    fn test_wasm_tile_key_pure() {
        let key = WasmTileKey::new(-23, -48);
        assert_eq!(key.lat_deg, -23);
        assert_eq!(key.lon_deg, -48);
    }

    #[test]
    fn test_wasm_camera_state_pure() {
        let cam = WasmCameraState::new(
            0.41, -0.81, 1000.0, 2.0, 0.5, 0.35, 0.0, 1.6, 250000.0,
        );
        assert_eq!(cam.center_lat, 0.41);
        assert_eq!(cam.center_lon, -0.81);
        assert_eq!(cam.center_height, 1000.0);
        assert_eq!(cam.zoom, 2.0);
        assert_eq!(cam.rotation, 0.5);
        assert_eq!(cam.pitch, 0.35);
        assert_eq!(cam.roll, 0.0);
        assert_eq!(cam.aspect_ratio, 1.6);
        assert_eq!(cam.viewport_base_meters, 250000.0);
    }

    #[test]
    fn test_wasm_interpolation_engine_with_stale_threshold() {
        let engine = WasmInterpolationEngine::with_stale_threshold(10.0);
        // Just verify it constructs without error
        let _ = engine;
    }

    #[test]
    fn test_wasm_lla_to_ecef_pure() {
        let ecef = lla_to_ecef(0.0, 0.0, 0.0);
        assert_eq!(ecef.len(), 3);
        assert!((ecef[0] - 6378137.0).abs() < 1.0);
        assert!(ecef[1].abs() < 1.0);
        assert!(ecef[2].abs() < 1.0);
    }

    #[test]
    fn test_wasm_ecef_to_lla_pure() {
        let lla = ecef_to_lla(6378137.0, 0.0, 0.0);
        assert!((lla.lat - 0.0).abs() < 1e-6);
        assert!((lla.lon - 0.0).abs() < 1e-6);
        assert!((lla.height - 0.0).abs() < 1.0);
    }

    #[test]
    fn test_wasm_ecef_lla_roundtrip_pure() {
        let lat = 0.41;
        let lon = -0.81;
        let height = 1000.0;
        let ecef = lla_to_ecef(lat, lon, height);
        let lla = ecef_to_lla(ecef[0], ecef[1], ecef[2]);
        assert!((lla.lat - lat).abs() < 1e-6);
        assert!((lla.lon - lon).abs() < 1e-6);
        assert!((lla.height - height).abs() < 1e-3);
    }
}
