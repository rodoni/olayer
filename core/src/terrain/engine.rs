use std::cell::RefCell;
use std::num::NonZeroUsize;
use lru::LruCache;
use crate::geodesy::coords::LatLon;
use crate::geodesy::ellipsoid::Ellipsoid;
use crate::geodesy::solvers::{GeodeticSolver, VincentySolver};
use crate::terrain::errors::TerrainError;
use crate::terrain::tile::DtedTile;

/// Default maximum number of DTED tiles kept in memory.
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

/// DTED terrain engine supporting O(1) elevation lookups and
/// vertical profile generation.
pub struct TerrainEngine {
    tiles: RefCell<LruCache<TileKey, DtedTile>>,
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
    }

    /// Returns the current number of cached tiles.
    #[inline]
    pub fn cache_size(&self) -> usize {
        self.tiles.borrow().len()
    }

    /// Clears all cached tiles.
    #[inline]
    pub fn clear_cache(&self) {
        self.tiles.borrow_mut().clear();
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

    /// Removes a tile from the engine.  Returns `true` if the tile existed.
    #[inline]
    pub fn unload_tile(&mut self, key: &TileKey) -> bool {
        self.tiles.borrow_mut().pop(key).is_some()
    }

    /// Returns the interpolated ground elevation (metres) for the given
    /// latitude and longitude in **degrees** using bilinear interpolation.
    #[inline]
    pub fn get_elevation(&self, lat_deg: f64, lon_deg: f64) -> Result<f64, TerrainError> {
        self.get_elevation_rad(lat_deg.to_radians(), lon_deg.to_radians())
    }

    /// Returns the interpolated ground elevation (metres) for the given
    /// latitude and longitude in **radians** using bilinear interpolation.
    #[inline]
    pub fn get_elevation_rad(&self, lat_rad: f64, lon_rad: f64) -> Result<f64, TerrainError> {
        let lat_deg = lat_rad.to_degrees();
        let lon_deg = lon_rad.to_degrees();
        let lat_floor = tile_key_floor(lat_deg);
        let lon_floor = tile_key_floor(lon_deg);

        let key = TileKey {
            lat_deg: lat_floor,
            lon_deg: lon_floor,
        };

        let mut tiles = self.tiles.borrow_mut();
        let tile = tiles.get(&key).ok_or(TerrainError::TileNotLoaded(
            lat_floor,
            lon_floor,
        ))?;

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

        // Look up the four neighbouring elevations; treat null sentinels as 0.0
        let z00 = get_elevation_val(tile.get_cell_elevation(row0, col0));
        let z01 = get_elevation_val(tile.get_cell_elevation(row0, col1));
        let z10 = get_elevation_val(tile.get_cell_elevation(row1, col0));
        let z11 = get_elevation_val(tile.get_cell_elevation(row1, col1));

        // Bilinear interpolation
        let z_left = z00 * (1.0 - ty) + z10 * ty;
        let z_right = z01 * (1.0 - ty) + z11 * ty;
        let z_final = z_left * (1.0 - tx) + z_right * tx;

        Ok(z_final)
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
fn get_elevation_val(val: i16) -> f64 {
    if val <= -32767 {
        0.0
    } else {
        f64::from(val)
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
