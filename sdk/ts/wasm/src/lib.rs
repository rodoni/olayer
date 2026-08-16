#![allow(clippy::missing_safety_doc)]
#![allow(clippy::too_many_arguments)]

use wasm_bindgen::prelude::*;
use olayer_core::geodesy::{GeodeticSolver, LatLon};
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

    /// Returns `{ elevation_meters: number | null }`, preserving DTED null samples.
    pub fn get_elevation_status(&self, lat_rad: f64, lon_rad: f64) -> Result<JsValue, JsValue> {
        let sample = self.inner.get_elevation_status(lat_rad, lon_rad)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        serde_wasm_bindgen::to_value(&sample)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Returns profile points with nullable elevations. Set reject_unknown to reject null DTED samples.
    pub fn get_vertical_profile_status(
        &self,
        route_coords: &[f64],
        step_meters: f64,
        reject_unknown: bool,
    ) -> Result<JsValue, JsValue> {
        let route: Vec<LatLon> = route_coords
            .chunks_exact(3)
            .map(|c| LatLon::from_degrees(c[0], c[1], c[2]))
            .collect();
        let policy = if reject_unknown {
            olayer_core::terrain::UnknownTerrainPolicy::Reject
        } else {
            olayer_core::terrain::UnknownTerrainPolicy::Propagate
        };
        let profile = self.inner.get_vertical_profile_status(&route, step_meters, policy)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        serde_wasm_bindgen::to_value(&profile)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Computes MSAW clearance. Unknown terrain is propagated when reject_unknown is false.
    pub fn calculate_clearance(
        &self,
        lat_rad: f64,
        lon_rad: f64,
        aircraft_height_meters: f64,
        minimum_clearance_meters: f64,
        reject_unknown: bool,
    ) -> Result<JsValue, JsValue> {
        let policy = if reject_unknown {
            olayer_core::terrain::UnknownTerrainPolicy::Reject
        } else {
            olayer_core::terrain::UnknownTerrainPolicy::Propagate
        };
        let result = self.inner.calculate_clearance(
            lat_rad,
            lon_rad,
            aircraft_height_meters,
            minimum_clearance_meters,
            policy,
        ).map_err(|e| JsValue::from_str(&e.to_string()))?;
        serde_wasm_bindgen::to_value(&result)
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

    /// Interpolates targets and returns valid predictions plus skipped-target status.
    pub fn interpolate_all_with_status(&self, current_time: f64) -> Result<JsValue, JsValue> {
        let batch = self.inner.interpolate_all_with_status(current_time)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        serde_wasm_bindgen::to_value(&batch)
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

// ============================================================================
// GIS-PROP-001 WASM Bindings: Local Tangent Frame & WMM-2025
// ============================================================================

/// WASM wrapper for an East-North-Up (ENU) topocentric coordinate.
#[wasm_bindgen]
pub struct WasmEnuPoint {
    pub east_m: f64,
    pub north_m: f64,
    pub up_m: f64,
}

#[wasm_bindgen]
impl WasmEnuPoint {
    #[wasm_bindgen(constructor)]
    pub fn new(east_m: f64, north_m: f64, up_m: f64) -> WasmEnuPoint {
        WasmEnuPoint {
            east_m,
            north_m,
            up_m,
        }
    }

    pub fn distance_2d(&self) -> f64 {
        self.east_m.hypot(self.north_m)
    }

    pub fn distance_3d(&self) -> f64 {
        self.east_m.hypot(self.north_m).hypot(self.up_m)
    }
}

/// WASM wrapper for a Local Tangent Frame centered at a geodetic origin.
#[wasm_bindgen]
pub struct WasmLocalTangentFrame {
    inner: olayer_core::geodesy::LocalTangentFrame,
}

#[wasm_bindgen]
impl WasmLocalTangentFrame {
    #[wasm_bindgen(constructor)]
    pub fn new(origin_lat_rad: f64, origin_lon_rad: f64, origin_height_m: f64) -> WasmLocalTangentFrame {
        let origin = LatLon::new(origin_lat_rad, origin_lon_rad, origin_height_m);
        WasmLocalTangentFrame {
            inner: olayer_core::geodesy::LocalTangentFrame::new(origin),
        }
    }

    /// Converts geodetic LLA coordinates to local ENU coordinates.
    pub fn lla_to_enu(&self, lat_rad: f64, lon_rad: f64, height_m: f64) -> WasmEnuPoint {
        let lla = LatLon::new(lat_rad, lon_rad, height_m);
        let pt = self.inner.lla_to_enu(&lla);
        WasmEnuPoint {
            east_m: pt.east_m,
            north_m: pt.north_m,
            up_m: pt.up_m,
        }
    }

    /// Converts local ENU coordinates to geodetic LLA coordinates.
    pub fn enu_to_lla(&self, east_m: f64, north_m: f64, up_m: f64) -> WasmLatLon {
        let enu = olayer_core::geodesy::EnuPoint::new(east_m, north_m, up_m);
        let lla = self.inner.enu_to_lla(&enu);
        WasmLatLon {
            lat: lla.lat,
            lon: lla.lon,
            height: lla.height,
        }
    }

    /// Returns `[slant_range_m, azimuth_rad, elevation_rad]` without atmospheric refraction.
    pub fn radar_look_angles(&self, target_lat_rad: f64, target_lon_rad: f64, target_height_m: f64) -> Vec<f64> {
        let target = LatLon::new(target_lat_rad, target_lon_rad, target_height_m);
        let (slant, az, el) = self.inner.radar_look_angles(&target);
        vec![slant, az, el]
    }

    /// Returns `[slant_range_m, azimuth_rad, elevation_rad]` with tropospheric refraction (k_factor ~1.333).
    pub fn radar_look_angles_refracted(&self, target_lat_rad: f64, target_lon_rad: f64, target_height_m: f64, k_factor: f64) -> Vec<f64> {
        let target = LatLon::new(target_lat_rad, target_lon_rad, target_height_m);
        let (slant, az, el) = self.inner.radar_look_angles_refracted(&target, k_factor);
        vec![slant, az, el]
    }
}

/// WASM wrapper for World Magnetic Model (WMM) queries.
#[wasm_bindgen]
pub struct WasmMagneticModel {
    inner: olayer_core::geodesy::MagneticModel,
}

#[wasm_bindgen]
impl WasmMagneticModel {
    /// Creates a default magnetic model initialized with built-in WMM-2025.
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmMagneticModel {
        WasmMagneticModel {
            inner: olayer_core::geodesy::MagneticModel::wmm2025(),
        }
    }

    /// Loads a magnetic model from a `WMM.COF` formatted string.
    pub fn from_cof(cof_content: &str) -> Result<WasmMagneticModel, JsValue> {
        let model = olayer_core::geodesy::MagneticModel::from_cof_str(cof_content)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(WasmMagneticModel { inner: model })
    }

    /// Returns the model base epoch in decimal years.
    pub fn epoch(&self) -> f64 {
        self.inner.epoch()
    }

    /// Returns the model identifier name.
    pub fn model_name(&self) -> String {
        self.inner.model_name().to_string()
    }

    /// Returns the maximum degree of the spherical harmonic model.
    pub fn max_degree(&self) -> usize {
        self.inner.max_degree()
    }

    /// Computes magnetic declination (variation) in radians for given WGS84 coordinate and epoch.
    pub fn get_declination(&self, lat_rad: f64, lon_rad: f64, height_m: f64, epoch: f64) -> f64 {
        let pt = LatLon::new(lat_rad, lon_rad, height_m);
        self.inner.get_declination(&pt, epoch)
    }

    /// Converts True North bearing to Magnetic North bearing in radians.
    pub fn true_to_magnetic(&self, true_bearing_rad: f64, lat_rad: f64, lon_rad: f64, height_m: f64, epoch: f64) -> f64 {
        let pt = LatLon::new(lat_rad, lon_rad, height_m);
        self.inner.true_to_magnetic(true_bearing_rad, &pt, epoch)
    }

    /// Converts Magnetic North bearing to True North bearing in radians.
    pub fn magnetic_to_true(&self, mag_bearing_rad: f64, lat_rad: f64, lon_rad: f64, height_m: f64, epoch: f64) -> f64 {
        let pt = LatLon::new(lat_rad, lon_rad, height_m);
        self.inner.magnetic_to_true(mag_bearing_rad, &pt, epoch)
    }

    /// Computes all magnetic field elements and returns serialized JSON.
    pub fn get_magnetic_elements(&self, lat_rad: f64, lon_rad: f64, height_m: f64, epoch: f64) -> Result<JsValue, JsValue> {
        let pt = LatLon::new(lat_rad, lon_rad, height_m);
        let elements = self.inner.get_magnetic_elements(&pt, epoch);
        serde_wasm_bindgen::to_value(&elements)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

impl Default for WasmMagneticModel {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// GIS-PROP-002 WASM Bindings: Geodesic Spatial Analysis Engine
// ============================================================================

/// WASM wrapper for route deviation metrics (XTK / ATD).
#[wasm_bindgen]
pub struct WasmRouteDeviation {
    pub cross_track_error_meters: f64,
    pub along_track_distance_meters: f64,
    pub nearest_lat: f64,
    pub nearest_lon: f64,
    pub nearest_height: f64,
}

/// Computes cross-track error (XTK) and along-track distance (ATD) in meters.
#[wasm_bindgen]
pub fn compute_route_deviation(
    start_lat_rad: f64,
    start_lon_rad: f64,
    end_lat_rad: f64,
    end_lon_rad: f64,
    pos_lat_rad: f64,
    pos_lon_rad: f64,
) -> WasmRouteDeviation {
    let start = LatLon::new(start_lat_rad, start_lon_rad, 0.0);
    let end = LatLon::new(end_lat_rad, end_lon_rad, 0.0);
    let pos = LatLon::new(pos_lat_rad, pos_lon_rad, 0.0);
    let dev = olayer_core::geodesy::compute_route_deviation(&start, &end, &pos);
    WasmRouteDeviation {
        cross_track_error_meters: dev.cross_track_error_meters,
        along_track_distance_meters: dev.along_track_distance_meters,
        nearest_lat: dev.nearest_point_on_route.lat,
        nearest_lon: dev.nearest_point_on_route.lon,
        nearest_height: dev.nearest_point_on_route.height,
    }
}

/// Computes the intersection of two geodesic segments on the globe, if one exists.
#[wasm_bindgen]
pub fn geodesic_intersection(
    p1_lat: f64,
    p1_lon: f64,
    p2_lat: f64,
    p2_lon: f64,
    p3_lat: f64,
    p3_lon: f64,
    p4_lat: f64,
    p4_lon: f64,
) -> Option<WasmLatLon> {
    let p1 = LatLon::new(p1_lat, p1_lon, 0.0);
    let p2 = LatLon::new(p2_lat, p2_lon, 0.0);
    let p3 = LatLon::new(p3_lat, p3_lon, 0.0);
    let p4 = LatLon::new(p4_lat, p4_lon, 0.0);
    olayer_core::geodesy::geodesic_intersection(&p1, &p2, &p3, &p4)
        .map(|pt| WasmLatLon { lat: pt.lat, lon: pt.lon, height: pt.height })
}

/// WASM wrapper for a GeodesicPolygon representing airspaces, FIR sectors, or geofences.
#[wasm_bindgen]
pub struct WasmGeodesicPolygon {
    inner: olayer_core::geodesy::GeodesicPolygon,
}

#[wasm_bindgen]
impl WasmGeodesicPolygon {
    /// Creates a new `WasmGeodesicPolygon` from a flat array of coordinates `[lat0, lon0, h0, lat1, lon1, h1, ...]`.
    #[wasm_bindgen(constructor)]
    pub fn new(coords: &[f64]) -> WasmGeodesicPolygon {
        let vertices: Vec<LatLon> = coords.chunks_exact(3)
            .map(|c| LatLon::new(c[0], c[1], c[2]))
            .collect();
        WasmGeodesicPolygon {
            inner: olayer_core::geodesy::GeodesicPolygon::new(vertices),
        }
    }

    /// Evaluates spherical winding number containment for target `(lat_rad, lon_rad)`.
    pub fn contains_point(&self, lat_rad: f64, lon_rad: f64) -> bool {
        let pt = LatLon::new(lat_rad, lon_rad, 0.0);
        self.inner.contains_point(&pt)
    }

    /// Computes minimum geodesic distance in meters from point to polygon boundary.
    pub fn distance_to_boundary(&self, lat_rad: f64, lon_rad: f64) -> f64 {
        let pt = LatLon::new(lat_rad, lon_rad, 0.0);
        self.inner.distance_to_boundary(&pt)
    }

    /// Generates a constant-width geodesic buffer polygon around this polygon.
    pub fn generate_buffer(&self, radius_meters: f64, num_segments: usize) -> WasmGeodesicPolygon {
        let buffered = self.inner.generate_buffer(radius_meters, num_segments);
        WasmGeodesicPolygon {
            inner: buffered,
        }
    }

    /// Returns a flat array of vertex coordinates `[lat0, lon0, h0, lat1, lon1, h1, ...]`.
    pub fn get_vertices(&self) -> Vec<f64> {
        let mut flat = Vec::with_capacity(self.inner.vertices.len() * 3);
        for v in &self.inner.vertices {
            flat.push(v.lat);
            flat.push(v.lon);
            flat.push(v.height);
        }
        flat
    }
}

// ============================================================================
// GIS-PROP-003 WASM Bindings: Tactical Aeronautical Measurement Tools
// ============================================================================

/// WASM representation of Range and Bearing Line (RBL / CRSR) measurement.
#[wasm_bindgen]
pub struct WasmRblMeasurement {
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
    pub estimated_time_enroute_sec: f64,
}

/// Computes Range and Bearing Line (RBL / CRSR) measurement between coordinates.
#[wasm_bindgen]
pub fn compute_tactical_rbl(
    from_lat_deg: f64,
    from_lon_deg: f64,
    to_lat_deg: f64,
    to_lon_deg: f64,
    speed_knots: f64,
    epoch_year: f64,
) -> WasmRblMeasurement {
    let ell = olayer_core::geodesy::Ellipsoid::wgs84();
    let solver = olayer_core::geodesy::VincentySolver;
    let from_pt = LatLon::from_degrees(from_lat_deg, from_lon_deg, 0.0);
    let to_pt = LatLon::from_degrees(to_lat_deg, to_lon_deg, 0.0);

    let inv = solver.inverse(&from_pt, &to_pt, &ell).unwrap_or_else(|_| {
        olayer_core::geodesy::HaversineSolver.inverse(&from_pt, &to_pt, &ell).unwrap()
    });

    let dist_m = inv.distance;
    let dist_nm = dist_m / 1852.0;
    let dist_km = dist_m / 1000.0;

    let true_bearing_rad = olayer_core::geodesy::normalize_bearing(inv.initial_bearing);
    let true_bearing_deg = true_bearing_rad.to_degrees();

    let epoch = if epoch_year > 1900.0 { epoch_year } else { 2025.0 };
    let mag_model = olayer_core::geodesy::MagneticModel::wmm2025();
    let mag_bearing_rad = mag_model.true_to_magnetic(true_bearing_rad, &from_pt, epoch);
    let mag_bearing_deg = mag_bearing_rad.to_degrees();

    let recip_true_deg = olayer_core::geodesy::normalize_bearing(true_bearing_rad + std::f64::consts::PI).to_degrees();
    let recip_mag_deg = olayer_core::geodesy::normalize_bearing(mag_bearing_rad + std::f64::consts::PI).to_degrees();

    let ete_sec = if speed_knots > 0.001 {
        (dist_nm / speed_knots) * 3600.0
    } else {
        -1.0
    };

    WasmRblMeasurement {
        from_lat_deg,
        from_lon_deg,
        to_lat_deg,
        to_lon_deg,
        distance_nm: dist_nm,
        distance_km: dist_km,
        true_bearing_deg,
        magnetic_bearing_deg: mag_bearing_deg,
        reciprocal_true_bearing_deg: recip_true_deg,
        reciprocal_magnetic_bearing_deg: recip_mag_deg,
        estimated_time_enroute_sec: ete_sec,
    }
}

/// Generates Projected Position Leader (PPL) ticks as a flat array `[time_min, dist_nm, lat_deg, lon_deg, ...]`.
#[wasm_bindgen]
pub fn generate_tactical_ppl(
    lat_deg: f64,
    lon_deg: f64,
    ground_speed_knots: f64,
    track_deg: f64,
    intervals_minutes: &[f64],
) -> Vec<f64> {
    let ell = olayer_core::geodesy::Ellipsoid::wgs84();
    let solver = olayer_core::geodesy::VincentySolver;
    let origin = LatLon::from_degrees(lat_deg, lon_deg, 0.0);
    let track_rad = track_deg.to_radians();

    let mut out = Vec::with_capacity(intervals_minutes.len() * 4);
    for &t_min in intervals_minutes {
        let dist_nm = ground_speed_knots * (t_min / 60.0);
        let dist_m = dist_nm * 1852.0;
        let projected = solver.direct(&origin, track_rad, dist_m, &ell).unwrap_or(origin);
        let (p_lat, p_lon, _) = projected.to_degrees();
        out.push(t_min);
        out.push(dist_nm);
        out.push(p_lat);
        out.push(p_lon);
    }
    out
}

/// Generates standard racetrack holding pattern polyline coordinates as flat array `[lat0, lon0, lat1, lon1, ...]`.
#[wasm_bindgen]
pub fn generate_tactical_holding_pattern(
    fix_lat_deg: f64,
    fix_lon_deg: f64,
    inbound_bearing_deg: f64,
    is_standard_right: bool,
    leg_time_minutes: f64,
    airspeed_knots: f64,
    points_per_turn: usize,
) -> Vec<f64> {
    let ell = olayer_core::geodesy::Ellipsoid::wgs84();
    let solver = olayer_core::geodesy::VincentySolver;
    let fix = LatLon::from_degrees(fix_lat_deg, fix_lon_deg, 0.0);

    let speed_mps = airspeed_knots * (1852.0 / 3600.0);
    let omega_rad_s = 3.0_f64.to_radians(); // 3 deg/s
    let turn_radius_m = speed_mps / omega_rad_s;
    let leg_dist_m = speed_mps * (leg_time_minutes * 60.0);

    let theta_inbound = inbound_bearing_deg.to_radians();
    let perp_sign = if is_standard_right { 1.0 } else { -1.0 };
    let bearing_to_turn1_center = olayer_core::geodesy::normalize_bearing(theta_inbound + perp_sign * (std::f64::consts::PI / 2.0));
    let turn1_center = solver.direct(&fix, bearing_to_turn1_center, turn_radius_m, &ell).unwrap_or(fix);

    let steps = points_per_turn.max(4);
    let mut poly = Vec::with_capacity((steps * 2 + 6) * 2);

    // 1. Fix point
    poly.push(fix_lat_deg);
    poly.push(fix_lon_deg);

    // 2. Turn 1
    let start_angle_1 = olayer_core::geodesy::normalize_bearing(bearing_to_turn1_center + std::f64::consts::PI);
    for i in 1..=steps {
        let frac = i as f64 / steps as f64;
        let sweep = if is_standard_right { frac * std::f64::consts::PI } else { -frac * std::f64::consts::PI };
        let angle = olayer_core::geodesy::normalize_bearing(start_angle_1 + sweep);
        let pt = solver.direct(&turn1_center, angle, turn_radius_m, &ell).unwrap_or(turn1_center);
        let (lat_d, lon_d, _) = pt.to_degrees();
        poly.push(lat_d);
        poly.push(lon_d);
    }

    // 3. Outbound leg
    let outbound_start_lat = poly[poly.len() - 2];
    let outbound_start_lon = poly[poly.len() - 1];
    let outbound_start = LatLon::from_degrees(outbound_start_lat, outbound_start_lon, 0.0);
    let theta_outbound = olayer_core::geodesy::normalize_bearing(theta_inbound + std::f64::consts::PI);
    let outbound_end = solver.direct(&outbound_start, theta_outbound, leg_dist_m, &ell).unwrap_or(outbound_start);
    let (ob_lat, ob_lon, _) = outbound_end.to_degrees();
    poly.push(ob_lat);
    poly.push(ob_lon);

    // 4. Turn 2
    let bearing_to_turn2_center = olayer_core::geodesy::normalize_bearing(theta_outbound + perp_sign * (std::f64::consts::PI / 2.0));
    let turn2_center = solver.direct(&outbound_end, bearing_to_turn2_center, turn_radius_m, &ell).unwrap_or(outbound_end);
    let start_angle_2 = olayer_core::geodesy::normalize_bearing(bearing_to_turn2_center + std::f64::consts::PI);

    for i in 1..=steps {
        let frac = i as f64 / steps as f64;
        let sweep = if is_standard_right { frac * std::f64::consts::PI } else { -frac * std::f64::consts::PI };
        let angle = olayer_core::geodesy::normalize_bearing(start_angle_2 + sweep);
        let pt = solver.direct(&turn2_center, angle, turn_radius_m, &ell).unwrap_or(turn2_center);
        let (lat_d, lon_d, _) = pt.to_degrees();
        poly.push(lat_d);
        poly.push(lon_d);
    }

    // 5. Close to Fix
    poly.push(fix_lat_deg);
    poly.push(fix_lon_deg);

    poly
}

/// Generates ILS approach funnel polygon coordinates as flat array `[lat0, lon0, lat1, lon1, ...]`.
#[wasm_bindgen]
pub fn generate_tactical_ils_cone(
    threshold_lat_deg: f64,
    threshold_lon_deg: f64,
    runway_heading_deg: f64,
    length_nm: f64,
    fov_deg: f64,
    arc_steps: usize,
) -> Vec<f64> {
    let ell = olayer_core::geodesy::Ellipsoid::wgs84();
    let solver = olayer_core::geodesy::VincentySolver;
    let threshold = LatLon::from_degrees(threshold_lat_deg, threshold_lon_deg, 0.0);

    let rwy_heading_rad = runway_heading_deg.to_radians();
    let approach_back_rad = olayer_core::geodesy::normalize_bearing(rwy_heading_rad + std::f64::consts::PI);
    let half_fov_rad = (fov_deg / 2.0).to_radians();
    let cone_dist_m = length_nm * 1852.0;

    let mut poly = Vec::new();
    poly.push(threshold_lat_deg);
    poly.push(threshold_lon_deg);

    let left_bearing = olayer_core::geodesy::normalize_bearing(approach_back_rad - half_fov_rad);
    let right_bearing = olayer_core::geodesy::normalize_bearing(approach_back_rad + half_fov_rad);

    let left_pt = solver.direct(&threshold, left_bearing, cone_dist_m, &ell).unwrap_or(threshold);
    let (left_lat, left_lon, _) = left_pt.to_degrees();
    poly.push(left_lat);
    poly.push(left_lon);

    let steps = arc_steps.max(2);
    for i in 1..steps {
        let frac = i as f64 / steps as f64;
        let angle = olayer_core::geodesy::normalize_bearing(left_bearing + frac * (fov_deg.to_radians()));
        let arc_pt = solver.direct(&threshold, angle, cone_dist_m, &ell).unwrap_or(threshold);
        let (a_lat, a_lon, _) = arc_pt.to_degrees();
        poly.push(a_lat);
        poly.push(a_lon);
    }

    let right_pt = solver.direct(&threshold, right_bearing, cone_dist_m, &ell).unwrap_or(threshold);
    let (right_lat, right_lon, _) = right_pt.to_degrees();
    poly.push(right_lat);
    poly.push(right_lon);

    // Close to threshold
    poly.push(threshold_lat_deg);
    poly.push(threshold_lon_deg);

    poly
}

/// Generates concentric range ring coordinates as a flat array of closed ring segments.
#[wasm_bindgen]
pub fn generate_tactical_range_rings(
    center_lat_deg: f64,
    center_lon_deg: f64,
    radii_nm: &[f64],
    points_per_ring: usize,
) -> Vec<f64> {
    let ell = olayer_core::geodesy::Ellipsoid::wgs84();
    let solver = olayer_core::geodesy::VincentySolver;
    let center = LatLon::from_degrees(center_lat_deg, center_lon_deg, 0.0);
    let steps = points_per_ring.max(12);

    let mut out = Vec::new();
    for &r_nm in radii_nm {
        let r_m = r_nm * 1852.0;
        for i in 0..=steps {
            let angle = (i as f64 / steps as f64) * 2.0 * std::f64::consts::PI;
            let pt = solver.direct(&center, angle, r_m, &ell).unwrap_or(center);
            let (lat_d, lon_d, _) = pt.to_degrees();
            out.push(lat_d);
            out.push(lon_d);
        }
    }
    out
}

// ============================================================================
// GIS-PROP-004 WASM Bindings: Aeronautical Data Ingestion (AIXM 5.1 & GeoJSON)
// ============================================================================

/// WASM wrapper for an in-memory repository of aeronautical information.
#[wasm_bindgen]
pub struct WasmAeronauticalDataset {
    inner: olayer_core::aeronautical::AeronauticalDataset,
}

#[wasm_bindgen]
impl WasmAeronauticalDataset {
    /// Creates a new empty aeronautical dataset.
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmAeronauticalDataset {
        WasmAeronauticalDataset {
            inner: olayer_core::aeronautical::AeronauticalDataset::new(),
        }
    }

    /// Parses an AIXM 5.1 formatted XML string.
    pub fn from_aixm_51(xml_content: &str) -> Result<WasmAeronauticalDataset, JsValue> {
        let ds = olayer_core::aeronautical::parse_aixm_51_str(xml_content)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(WasmAeronauticalDataset { inner: ds })
    }

    /// Parses a GeoJSON-Aviation formatted JSON string.
    pub fn from_geojson(json_content: &str) -> Result<WasmAeronauticalDataset, JsValue> {
        let ds = olayer_core::aeronautical::parse_geojson_aviation_str(json_content)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(WasmAeronauticalDataset { inner: ds })
    }

    /// Serializes the dataset into a standard GeoJSON FeatureCollection string.
    pub fn to_geojson(&self) -> Result<String, JsValue> {
        olayer_core::aeronautical::export_dataset_to_geojson(&self.inner)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Number of airspaces in the dataset.
    pub fn airspace_count(&self) -> usize {
        self.inner.airspaces.len()
    }

    /// Number of radio navaids and fixes in the dataset.
    pub fn navaid_count(&self) -> usize {
        self.inner.navaids.len()
    }

    /// Number of airway routes in the dataset.
    pub fn airway_count(&self) -> usize {
        self.inner.airways.len()
    }

    /// Number of aerodromes in the dataset.
    pub fn airport_count(&self) -> usize {
        self.inner.airports.len()
    }

    /// Total count of all aeronautical features.
    pub fn total_feature_count(&self) -> usize {
        self.inner.total_feature_count()
    }

    /// Finds a navaid by its identification code, returned as a JSON object.
    pub fn find_navaid(&self, ident: &str) -> Result<JsValue, JsValue> {
        let navaid = self.inner.find_navaid(ident);
        serde_wasm_bindgen::to_value(&navaid)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Finds navaids within a radius in meters around a geodetic point (lat/lon in radians).
    pub fn find_navaids_within_radius(&self, lat_rad: f64, lon_rad: f64, radius_meters: f64) -> Result<JsValue, JsValue> {
        let center = LatLon::new(lat_rad, lon_rad, 0.0);
        let navaids = self.inner.find_navaids_within_radius(&center, radius_meters);
        serde_wasm_bindgen::to_value(&navaids)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Finds airspaces containing the given coordinate (lat/lon in radians).
    pub fn find_airspaces_containing_point(&self, lat_rad: f64, lon_rad: f64) -> Result<JsValue, JsValue> {
        let pt = LatLon::new(lat_rad, lon_rad, 0.0);
        let airspaces = self.inner.find_airspaces_containing_point(&pt);
        serde_wasm_bindgen::to_value(&airspaces)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

impl Default for WasmAeronauticalDataset {
    fn default() -> Self {
        Self::new()
    }
}

/// Parses an AIXM 5.1 XML string into a [`WasmAeronauticalDataset`].
#[wasm_bindgen]
pub fn parse_aixm_51(xml_content: &str) -> Result<WasmAeronauticalDataset, JsValue> {
    WasmAeronauticalDataset::from_aixm_51(xml_content)
}

/// Parses a GeoJSON-Aviation string into a [`WasmAeronauticalDataset`].
#[wasm_bindgen]
pub fn parse_geojson_aviation(json_content: &str) -> Result<WasmAeronauticalDataset, JsValue> {
    WasmAeronauticalDataset::from_geojson(json_content)
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

    #[test]
    fn test_wasm_local_tangent_frame_pure() {
        let frame = WasmLocalTangentFrame::new(0.0, 0.0, 0.0);
        let enu = frame.lla_to_enu(0.01, 0.01, 100.0);
        assert!(enu.distance_2d() > 1000.0);
        let lla = frame.enu_to_lla(enu.east_m, enu.north_m, enu.up_m);
        assert!((lla.lat - 0.01).abs() < 1e-6);
        assert!((lla.lon - 0.01).abs() < 1e-6);

        let angles = frame.radar_look_angles(0.01, 0.01, 100.0);
        assert_eq!(angles.len(), 3);
        assert!(angles[0] > 1000.0); // slant range
    }

    #[test]
    fn test_wasm_magnetic_model_pure() {
        let model = WasmMagneticModel::new();
        assert_eq!(model.epoch(), 2025.0);
        assert_eq!(model.model_name(), "WMM-2025");
        let dec = model.get_declination(
            40.64_f64.to_radians(),
            -73.78_f64.to_radians(),
            0.0,
            2025.0,
        );
        let dec_deg = dec.to_degrees();
        assert!(dec_deg > -15.0 && dec_deg < -10.0);

        // Test dynamic from_cof
        let cof_str = "2030.0 WMM-2030 11/20/2029\n1 0 -29396.6 0.0 11.6 0.0\n1 1 -1404.9 4589.6 12.3 -23.4";
        let custom_model = WasmMagneticModel::from_cof(cof_str).unwrap();
        assert_eq!(custom_model.epoch(), 2030.0);
        assert_eq!(custom_model.model_name(), "WMM-2030");
    }

    #[test]
    fn test_wasm_spatial_deviation_and_polygon_pure() {
        let dev = compute_route_deviation(0.0, 0.0, 0.0, 0.1, 0.01, 0.05);
        assert!(dev.cross_track_error_meters < 0.0); // North of Eastbound track -> negative XTK
        assert!(dev.along_track_distance_meters > 0.0);

        let coords = [
            0.0, 0.0, 0.0,
            0.0, 0.1, 0.0,
            0.1, 0.1, 0.0,
            0.1, 0.0, 0.0,
        ];
        let poly = WasmGeodesicPolygon::new(&coords);
        assert!(poly.contains_point(0.05, 0.05));
        assert!(!poly.contains_point(0.2, 0.2));

        let inter = geodesic_intersection(
            0.0, -0.1, 0.0, 0.1,
            -0.1, 0.0, 0.1, 0.0,
        );
        assert!(inter.is_some());
        let pt = inter.unwrap();
        assert!(pt.lat.abs() < 1e-6);
        assert!(pt.lon.abs() < 1e-6);
    }

    #[test]
    fn test_wasm_tactical_tools_pure() {
        // RBL
        let rbl = compute_tactical_rbl(40.64, -73.78, 42.36, -71.01, 450.0, 2025.0);
        assert!(rbl.distance_nm > 150.0 && rbl.distance_nm < 185.0);
        assert!(rbl.estimated_time_enroute_sec > 1000.0);

        // PPL
        let ppl = generate_tactical_ppl(40.0, -74.0, 480.0, 90.0, &[1.0, 2.0, 5.0]);
        assert_eq!(ppl.len(), 12); // 3 ticks * 4 values
        assert!((ppl[1] - 8.0).abs() < 1e-3); // dist_nm = 8.0

        // Holding Pattern
        let holding = generate_tactical_holding_pattern(51.5, -0.1, 270.0, true, 1.0, 210.0, 16);
        assert!(holding.len() >= 68); // 34+ points * 2

        // ILS Cone
        let ils = generate_tactical_ils_cone(51.4775, -0.4614, 270.0, 10.0, 5.0, 12);
        assert!(ils.len() >= 28); // 14+ points * 2

        // Range Rings
        let rings = generate_tactical_range_rings(0.0, 0.0, &[10.0], 36);
        assert_eq!(rings.len(), 74); // 37 points * 2
    }

    #[test]
    fn test_wasm_aeronautical_dataset_pure() {
        let geojson = r#"{
            "type": "FeatureCollection",
            "features": [
                {
                    "type": "Feature",
                    "properties": {
                        "aero_type": "Airspace",
                        "uid": "TEST_TMA",
                        "name": "TEST TMA",
                        "airspace_type": "TMA",
                        "lower_limit_m": 500.0,
                        "upper_limit_fl": 150
                    },
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [[
                            [0.0, 50.0, 0.0],
                            [1.0, 50.0, 0.0],
                            [1.0, 51.0, 0.0],
                            [0.0, 51.0, 0.0],
                            [0.0, 50.0, 0.0]
                        ]]
                    }
                },
                {
                    "type": "Feature",
                    "properties": {
                        "aero_type": "Navaid",
                        "ident": "TST",
                        "name": "TEST VOR",
                        "navaid_type": "VOR",
                        "frequency_mhz": 114.5
                    },
                    "geometry": {
                        "type": "Point",
                        "coordinates": [0.5, 50.5, 100.0]
                    }
                }
            ]
        }"#;

        let ds = WasmAeronauticalDataset::from_geojson(geojson).expect("Parse GeoJSON in WASM failed");
        assert_eq!(ds.airspace_count(), 1);
        assert_eq!(ds.navaid_count(), 1);
        assert_eq!(ds.total_feature_count(), 2);

        let exported = ds.to_geojson().expect("To GeoJSON in WASM failed");
        assert!(exported.contains("TEST_TMA"));
        assert!(exported.contains("TST"));
    }
}
