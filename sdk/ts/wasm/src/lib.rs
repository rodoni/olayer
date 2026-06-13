use wasm_bindgen::prelude::*;
use olayer_core::geodesy::LatLon;
use olayer_core::geodesy::ellipsoid::Ellipsoid;
use olayer_core::terrain::TerrainEngine;
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
        }
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
}
