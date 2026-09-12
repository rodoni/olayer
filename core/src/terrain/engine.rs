use std::cell::RefCell;
use std::num::NonZeroUsize;
use serde::{Deserialize, Serialize};
use lru::LruCache;
use crate::geodesy::coords::LatLon;
use crate::geodesy::ellipsoid::Ellipsoid;
use crate::geodesy::solvers::{GeodeticSolver, VincentySolver};
use crate::terrain::errors::TerrainError;
use crate::terrain::geotiff::GeoTiffTile;
use crate::terrain::rgb_decoder::RgbElevationEncoding;
use crate::terrain::rgb_tile::{RgbElevationTile, SlippyTileKey};
use crate::terrain::tile::DtedTile;
use crate::terrain::altitude::{resolve_altitude, AltitudeMode, AltitudeUnknownPolicy};

/// Default maximum number of DTED and RGB tiles kept in memory.
const DEFAULT_TILE_CAPACITY: usize = 64;

/// Tile lookup key based on integer degrees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileKey {
    pub lat_deg: i32,
    pub lon_deg: i32,
}

/// A single point in a vertical terrain profile.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfilePoint {
    pub distance_meters: f64,
    pub ground_elevation: f64,
    pub coords: LatLon,
}

/// Elevation result that distinguishes an unknown DTED sample from zero metres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ElevationSample {
    pub elevation_meters: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownTerrainPolicy {
    Propagate,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfilePointStatus {
    pub distance_meters: f64,
    pub ground_elevation: Option<f64>,
    pub coords: LatLon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MsawState {
    Safe,
    Warning,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ClearanceResult {
    pub clearance_meters: Option<f64>,
    pub state: MsawState,
}

/// Multi-source terrain elevation engine supporting DTED, Mapbox/Terrarium RGB tiles,
/// and Cloud-Optimized GeoTIFFs (COG) with sub-grid bilinear interpolation.
pub struct TerrainEngine {
    tiles: RefCell<LruCache<TileKey, DtedTile>>,
    rgb_tiles: RefCell<LruCache<SlippyTileKey, RgbElevationTile>>,
    geotiff_tiles: RefCell<Vec<GeoTiffTile>>,
}

impl TerrainEngine {
    /// Creates a new terrain engine with the default tile cache capacity.
    #[inline]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_TILE_CAPACITY)
    }

    /// Creates a new terrain engine with a custom tile cache capacity.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` is zero.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity)
            .expect("terrain tile cache capacity must be non-zero");
        Self {
            tiles: RefCell::new(LruCache::new(cap)),
            rgb_tiles: RefCell::new(LruCache::new(cap)),
            geotiff_tiles: RefCell::new(Vec::new()),
        }
    }

    /// Changes the tile cache capacity.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` is zero.
    #[inline]
    pub fn set_cache_capacity(&self, capacity: usize) {
        let cap = NonZeroUsize::new(capacity)
            .expect("terrain tile cache capacity must be non-zero");
        self.tiles.borrow_mut().resize(cap);
        self.rgb_tiles.borrow_mut().resize(cap);
    }

    /// Returns the current number of cached DTED tiles.
    #[inline]
    pub fn cache_size(&self) -> usize {
        self.tiles.borrow().len()
    }

    /// Returns the current number of cached RGB elevation tiles.
    #[inline]
    pub fn rgb_cache_size(&self) -> usize {
        self.rgb_tiles.borrow().len()
    }

    /// Returns the current number of registered GeoTIFF rasters.
    #[inline]
    pub fn geotiff_cache_size(&self) -> usize {
        self.geotiff_tiles.borrow().len()
    }

    /// Clears all cached DTED tiles.
    #[inline]
    pub fn clear_cache(&self) {
        self.tiles.borrow_mut().clear();
    }

    /// Clears all cached RGB elevation tiles.
    #[inline]
    pub fn clear_rgb_cache(&self) {
        self.rgb_tiles.borrow_mut().clear();
    }

    /// Clears all registered GeoTIFF rasters.
    #[inline]
    pub fn clear_geotiff_cache(&self) {
        self.geotiff_tiles.borrow_mut().clear();
    }

    /// Clears all terrain sources (DTED, RGB tiles, GeoTIFFs).
    #[inline]
    pub fn clear_all(&self) {
        self.clear_cache();
        self.clear_rgb_cache();
        self.clear_geotiff_cache();
    }

    /// Parses a raw DTED buffer and registers the resulting tile.
    #[inline]
    pub fn load_tile(&mut self, data: &[u8]) -> Result<TileKey, TerrainError> {
        let tile = DtedTile::from_bytes(data)?;
        let key = TileKey {
            lat_deg: tile.origin_lat,
            lon_deg: tile.origin_lon,
        };
        self.tiles.borrow_mut().put(key, tile);
        Ok(key)
    }

    /// Loads and registers a Web Mercator $(Z, X, Y)$ RGB elevation tile.
    #[allow(clippy::too_many_arguments)]
    pub fn load_rgb_tile(
        &self,
        z: u32,
        x: u32,
        y: u32,
        width: usize,
        height: usize,
        rgba_buffer: &[u8],
        encoding: RgbElevationEncoding,
    ) -> Result<SlippyTileKey, TerrainError> {
        let key = SlippyTileKey::new(z, x, y);
        let tile = RgbElevationTile::from_rgba(key, width, height, rgba_buffer, encoding)?;
        self.rgb_tiles.borrow_mut().put(key, tile);
        Ok(key)
    }

    /// Loads and registers a GeoTIFF / Cloud-Optimized GeoTIFF elevation raster.
    /// Returns the geographic bounding box `(min_lat_rad, min_lon_rad, max_lat_rad, max_lon_rad)`.
    pub fn load_geotiff_tile(&self, data: &[u8]) -> Result<(f64, f64, f64, f64), TerrainError> {
        let tile = GeoTiffTile::from_bytes(data)?;
        let bounds = tile.bounds_rad;
        self.geotiff_tiles.borrow_mut().push(tile);
        Ok(bounds)
    }

    /// Removes an RGB tile by its $(Z, X, Y)$ key. Returns `true` if the tile existed.
    #[inline]
    pub fn unload_rgb_tile(&self, key: &SlippyTileKey) -> bool {
        self.rgb_tiles.borrow_mut().pop(key).is_some()
    }

    /// Removes a tile from the engine.  Returns `true` if the tile existed.
    #[inline]
    pub fn unload_tile(&mut self, key: &TileKey) -> bool {
        self.tiles.borrow_mut().pop(key).is_some()
    }

    /// Returns the interpolated ground elevation (metres) for the given
    /// latitude and longitude in **degrees** using bilinear interpolation.
    #[inline]
    pub fn get_elevation(&self, lat_deg: f64, lon_deg: f64) -> Result<f64, TerrainError> {
        Ok(self
            .get_elevation_status(lat_deg.to_radians(), lon_deg.to_radians())?
            .elevation_meters
            .unwrap_or(0.0))
    }

    /// Returns the interpolated ground elevation (metres) for the given
    /// latitude and longitude in **radians** using bilinear interpolation.
    #[inline]
    pub fn get_elevation_rad(&self, lat_rad: f64, lon_rad: f64) -> Result<f64, TerrainError> {
        Ok(self.get_elevation_status(lat_rad, lon_rad)?.elevation_meters.unwrap_or(0.0))
    }

    /// Returns elevation quality without converting missing DTED samples to zero.
    /// Queries DTED cache first, followed by Web Mercator RGB elevation tiles, and GeoTIFF rasters.
    #[inline]
    pub fn get_elevation_status(&self, lat_rad: f64, lon_rad: f64) -> Result<ElevationSample, TerrainError> {
        let lat_deg = lat_rad.to_degrees();
        let lon_deg = lon_rad.to_degrees();
        let lat_floor = tile_key_floor(lat_deg);
        let lon_floor = tile_key_floor(lon_deg);

        let key = TileKey {
            lat_deg: lat_floor,
            lon_deg: lon_floor,
        };

        // 1. Try DTED cache
        let mut tiles = self.tiles.borrow_mut();
        if let Some(tile) = tiles.get(&key) {
            // Fraction within the tile
            let delta_lat = (lat_deg - lat_floor as f64).clamp(0.0, 1.0);
            let delta_lon = (lon_deg - lon_floor as f64).clamp(0.0, 1.0);

            let row_f = delta_lat * (tile.num_rows - 1) as f64;
            let col_f = delta_lon * (tile.num_cols - 1) as f64;

            let row0 = (row_f.floor() as usize).min(tile.num_rows - 1);
            let row1 = (row0 + 1).min(tile.num_rows - 1);

            let col0 = (col_f.floor() as usize).min(tile.num_cols - 1);
            let col1 = (col0 + 1).min(tile.num_cols - 1);

            let tx = (col_f - col0 as f64).clamp(0.0, 1.0);
            let ty = (row_f - row0 as f64).clamp(0.0, 1.0);

            // A bilinear result is unknown if any contributing DTED sample is null.
            let z00 = get_elevation_val(tile.get_cell_elevation(row0, col0));
            let z01 = get_elevation_val(tile.get_cell_elevation(row0, col1));
            let z10 = get_elevation_val(tile.get_cell_elevation(row1, col0));
            let z11 = get_elevation_val(tile.get_cell_elevation(row1, col1));
            if [z00, z01, z10, z11].iter().any(Option::is_none) {
                return Ok(ElevationSample { elevation_meters: None });
            }
            let z00 = z00.unwrap();
            let z01 = z01.unwrap();
            let z10 = z10.unwrap();
            let z11 = z11.unwrap();

            // Bilinear interpolation
            let z_left = z00 * (1.0 - ty) + z10 * ty;
            let z_right = z01 * (1.0 - ty) + z11 * ty;
            let z_final = z_left * (1.0 - tx) + z_right * tx;

            return Ok(ElevationSample { elevation_meters: Some(z_final) });
        }
        drop(tiles);

        // 2. Try RGB elevation tile cache (prefer highest zoom level covering the coordinate)
        let rgb_tiles = self.rgb_tiles.borrow();
        let mut best_rgb_sample: Option<(u32, f64)> = None;
        for (k, tile) in rgb_tiles.iter() {
            if let Some(elev) = tile.get_elevation_rad(lat_rad, lon_rad) {
                if best_rgb_sample.is_none() || k.z > best_rgb_sample.unwrap().0 {
                    best_rgb_sample = Some((k.z, elev));
                }
            }
        }
        if let Some((_, elev)) = best_rgb_sample {
            return Ok(ElevationSample { elevation_meters: Some(elev) });
        }
        drop(rgb_tiles);

        // 3. Try GeoTIFF rasters
        let geotiffs = self.geotiff_tiles.borrow();
        for gt in geotiffs.iter() {
            if let Some(elev) = gt.get_elevation_rad(lat_rad, lon_rad) {
                return Ok(ElevationSample { elevation_meters: Some(elev) });
            }
        }
        drop(geotiffs);

        // 4. Not found in any loaded terrain source
        Err(TerrainError::TileNotLoaded(
            lat_floor,
            lon_floor,
        ))
    }

    /// Applies an explicit policy to unknown terrain samples.
    #[inline]
    pub fn get_elevation_with_policy(
        &self,
        lat_rad: f64,
        lon_rad: f64,
        policy: UnknownTerrainPolicy,
    ) -> Result<Option<f64>, TerrainError> {
        let elevation = self.get_elevation_status(lat_rad, lon_rad)?.elevation_meters;
        if elevation.is_none() && policy == UnknownTerrainPolicy::Reject {
            return Err(TerrainError::MalformedData(
                "Unknown terrain elevation".to_string(),
            ));
        }
        Ok(elevation)
    }

    /// Resolves an object's height against the sampled terrain. Vertical
    /// exaggeration is intentionally not applied here because it is visual-only.
    pub fn resolve_altitude(
        &self,
        lat_rad: f64,
        lon_rad: f64,
        input_height: f64,
        mode: AltitudeMode,
        unknown_policy: AltitudeUnknownPolicy,
        mesh_height: Option<f64>,
    ) -> Result<f64, TerrainError> {
        if mode == AltitudeMode::Absolute {
            return resolve_altitude(input_height, None, mesh_height, mode, unknown_policy)
                .map_err(|error| TerrainError::MalformedData(error.to_string()));
        }

        let ground_height = match self.get_elevation_status(lat_rad, lon_rad) {
            Ok(sample) => sample.elevation_meters,
            Err(error) => match unknown_policy {
                AltitudeUnknownPolicy::Reject => return Err(error),
                AltitudeUnknownPolicy::UseAbsolute | AltitudeUnknownPolicy::UseZero => None,
            },
        };
        resolve_altitude(input_height, ground_height, mesh_height, mode, unknown_policy)
            .map_err(|error| TerrainError::MalformedData(error.to_string()))
    }

    /// Builds a profile while preserving unknown samples or rejecting them by policy.
    #[inline]
    pub fn get_vertical_profile_status(
        &self,
        route: &[LatLon],
        step_meters: f64,
        policy: UnknownTerrainPolicy,
    ) -> Result<Vec<ProfilePointStatus>, TerrainError> {
        if route.len() < 2 {
            return Err(TerrainError::MalformedData(
                "Route must contain at least 2 points".to_string(),
            ));
        }

        let solver = VincentySolver;
        let ellipsoid = Ellipsoid::wgs84();
        let mut profile = Vec::new();
        let mut accumulated_distance = 0.0;

        for i in 0..route.len() - 1 {
            let p1 = &route[i];
            let p2 = &route[i + 1];
            let res = solver.inverse(p1, p2, &ellipsoid).map_err(|e| {
                TerrainError::MalformedData(format!("Failed to compute route distance: {e}"))
            })?;
            let segment_dist = res.distance;
            let num_steps = (segment_dist / step_meters).floor() as usize;
            for s in 0..num_steps {
                let d = s as f64 * step_meters;
                let pt = solver.direct(p1, res.initial_bearing, d, &ellipsoid).map_err(|e| {
                    TerrainError::MalformedData(format!("Failed to interpolate route point: {e}"))
                })?;
                profile.push(ProfilePointStatus {
                    distance_meters: accumulated_distance + d,
                    ground_elevation: self.get_elevation_with_policy(pt.lat, pt.lon, policy)?,
                    coords: pt,
                });
            }
            accumulated_distance += segment_dist;
        }

        let last_pt = route.last().unwrap();
        profile.push(ProfilePointStatus {
            distance_meters: accumulated_distance,
            ground_elevation: self.get_elevation_with_policy(last_pt.lat, last_pt.lon, policy)?,
            coords: *last_pt,
        });
        Ok(profile)
    }

    /// Computes aircraft-to-ground clearance and applies an explicit unknown-terrain policy.
    #[inline]
    pub fn calculate_clearance(
        &self,
        lat_rad: f64,
        lon_rad: f64,
        aircraft_height_meters: f64,
        minimum_clearance_meters: f64,
        policy: UnknownTerrainPolicy,
    ) -> Result<ClearanceResult, TerrainError> {
        if !aircraft_height_meters.is_finite() || !minimum_clearance_meters.is_finite()
            || minimum_clearance_meters < 0.0
        {
            return Err(TerrainError::MalformedData(
                "Aircraft height and minimum clearance must be finite; minimum clearance must be non-negative".to_string(),
            ));
        }
        let ground = self.get_elevation_with_policy(lat_rad, lon_rad, policy)?;
        let Some(ground) = ground else {
            return Ok(ClearanceResult {
                clearance_meters: None,
                state: MsawState::Unknown,
            });
        };
        let clearance = aircraft_height_meters - ground;
        Ok(ClearanceResult {
            clearance_meters: Some(clearance),
            state: if clearance < minimum_clearance_meters {
                MsawState::Warning
            } else {
                MsawState::Safe
            },
        })
    }

    /// Generates a vertical terrain profile along a sequence of route points.
    ///
    /// For each segment, samples are taken every `step_meters`.  If a tile is
    /// missing for any intermediate point, the error is propagated instead of
    /// silently defaulting to sea level.
    #[inline]
    pub fn get_vertical_profile(
        &self,
        route: &[LatLon],
        step_meters: f64,
    ) -> Result<Vec<ProfilePoint>, TerrainError> {
        if route.len() < 2 {
            return Err(TerrainError::MalformedData(
                "Route must contain at least 2 points".to_string(),
            ));
        }

        let solver = VincentySolver;
        let ellipsoid = Ellipsoid::wgs84();
        let mut profile = Vec::new();
        let mut accumulated_distance = 0.0;

        for i in 0..route.len() - 1 {
            let p1 = &route[i];
            let p2 = &route[i + 1];

            let res = solver.inverse(p1, p2, &ellipsoid).map_err(|e| {
                TerrainError::MalformedData(format!("Failed to compute route distance: {e}"))
            })?;

            let segment_dist = res.distance;
            let num_steps = (segment_dist / step_meters).floor() as usize;

            for s in 0..num_steps {
                let d = s as f64 * step_meters;
                let pt = solver.direct(p1, res.initial_bearing, d, &ellipsoid).map_err(|e| {
                    TerrainError::MalformedData(format!("Failed to interpolate route point: {e}"))
                })?;

                let elev = self.get_elevation_rad(pt.lat, pt.lon)?;
                profile.push(ProfilePoint {
                    distance_meters: accumulated_distance + d,
                    ground_elevation: elev,
                    coords: pt,
                });
            }

            accumulated_distance += segment_dist;
        }

        // Add the exact final route point
        let last_pt = route.last().unwrap();
        let elev = self.get_elevation_rad(last_pt.lat, last_pt.lon)?;
        profile.push(ProfilePoint {
            distance_meters: accumulated_distance,
            ground_elevation: elev,
            coords: *last_pt,
        });

        Ok(profile)
    }
}

impl Default for TerrainEngine {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// Converts a raw DTED cell value to metres, treating null sentinels as 0.0.
#[inline]
fn get_elevation_val(val: i16) -> Option<f64> {
    if val <= -32767 {
        None
    } else {
        Some(f64::from(val))
    }
}

/// Computes the integer tile key coordinate, snapping values that are
/// within 1e-12 of an integer boundary to avoid floating-point errors
/// (e.g. -48.00000000000001 from `to_degrees()`).
#[inline]
fn tile_key_floor(x: f64) -> i32 {
    let r = x.round();
    if (x - r).abs() < 1e-12 {
        r as i32
    } else {
        x.floor() as i32
    }
}
