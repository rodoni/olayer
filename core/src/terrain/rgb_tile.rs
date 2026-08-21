use serde::{Deserialize, Serialize};
use crate::terrain::errors::TerrainError;
use crate::terrain::rgb_decoder::{decode_rgba_buffer, RgbElevationEncoding};

/// Slippy Map (Web Mercator) tile identifier $(Z, X, Y)$.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SlippyTileKey {
    /// Zoom level (e.g. 0 to 20).
    pub z: u32,
    /// Column index ($0 \le X < 2^Z$).
    pub x: u32,
    /// Row index ($0 \le Y < 2^Z$).
    pub y: u32,
}

impl SlippyTileKey {
    /// Creates a new Slippy Map tile key.
    #[inline]
    pub const fn new(z: u32, x: u32, y: u32) -> Self {
        Self { z, x, y }
    }

    /// Computes the Slippy tile key for a given geodetic coordinate (lat/lon in radians) at a specified zoom level.
    pub fn from_lat_lon(lat_rad: f64, lon_rad: f64, zoom: u32) -> Self {
        let lat_deg = lat_rad.to_degrees().clamp(-85.05112878, 85.05112878);
        let lon_deg = lon_rad.to_degrees();

        let n = 2.0_f64.powi(zoom as i32);
        let x = (((lon_deg + 180.0) / 360.0) * n).floor() as u32;
        let x = x.min((1 << zoom) - 1);

        let lat_rad_clamped = lat_deg.to_radians();
        let y_val = (1.0 - (lat_rad_clamped.tan() + 1.0 / lat_rad_clamped.cos()).ln() / std::f64::consts::PI) / 2.0 * n;
        let y = y_val.floor().max(0.0) as u32;
        let y = y.min((1 << zoom) - 1);

        Self { z: zoom, x, y }
    }

    /// Computes the WGS84 geographic bounding box `(min_lat_rad, min_lon_rad, max_lat_rad, max_lon_rad)`
    /// for this Web Mercator tile.
    pub fn bounds_rad(&self) -> (f64, f64, f64, f64) {
        let n = 2.0_f64.powi(self.z as i32);

        // Longitude bounds
        let min_lon_deg = (self.x as f64 / n) * 360.0 - 180.0;
        let max_lon_deg = ((self.x + 1) as f64 / n) * 360.0 - 180.0;

        // Latitude bounds using inverse Web Mercator gudermannian
        let n_north = std::f64::consts::PI * (1.0 - 2.0 * (self.y as f64) / n);
        let max_lat_rad = n_north.sinh().atan();

        let n_south = std::f64::consts::PI * (1.0 - 2.0 * ((self.y + 1) as f64) / n);
        let min_lat_rad = n_south.sinh().atan();

        (min_lat_rad, min_lon_deg.to_radians(), max_lat_rad, max_lon_deg.to_radians())
    }
}

/// A decoded RGB elevation raster tile defined on a Web Mercator $(Z, X, Y)$ grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RgbElevationTile {
    /// Tile coordinate key.
    pub key: SlippyTileKey,
    /// Raster width in pixels (e.g. 256 or 512).
    pub width: usize,
    /// Raster height in pixels (e.g. 256 or 512).
    pub height: usize,
    /// Decoded elevation grid in meters (row-major from North to South, West to East).
    pub elevations: Vec<f32>,
    /// Bounding box `(min_lat_rad, min_lon_rad, max_lat_rad, max_lon_rad)`.
    pub bounds_rad: (f64, f64, f64, f64),
}

impl RgbElevationTile {
    /// Creates an `RgbElevationTile` from a raw RGBA byte buffer and encoding format.
    pub fn from_rgba(
        key: SlippyTileKey,
        width: usize,
        height: usize,
        rgba_buffer: &[u8],
        encoding: RgbElevationEncoding,
    ) -> Result<Self, TerrainError> {
        let elevations = decode_rgba_buffer(rgba_buffer, width, height, encoding)?;
        let bounds_rad = key.bounds_rad();
        Ok(Self {
            key,
            width,
            height,
            elevations,
            bounds_rad,
        })
    }

    /// Checks if a geodetic point (lat/lon in radians) falls within the tile's bounding box.
    #[inline]
    pub fn contains_point_rad(&self, lat_rad: f64, lon_rad: f64) -> bool {
        let (min_lat, min_lon, max_lat, max_lon) = self.bounds_rad;
        lat_rad >= min_lat && lat_rad <= max_lat && lon_rad >= min_lon && lon_rad <= max_lon
    }

    /// Samples elevation in meters at a geodetic coordinate (lat/lon in radians) using bilinear interpolation.
    /// Returns `None` if the point is outside the tile boundaries.
    pub fn get_elevation_rad(&self, lat_rad: f64, lon_rad: f64) -> Option<f64> {
        if !self.contains_point_rad(lat_rad, lon_rad) {
            return None;
        }

        let (min_lat, min_lon, max_lat, max_lon) = self.bounds_rad;

        // Longitude fraction (u) in [0, 1]
        let u = if (max_lon - min_lon).abs() > 1e-12 {
            ((lon_rad - min_lon) / (max_lon - min_lon)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // Web Mercator latitude fraction (v) in [0, 1] where 0 is North (top) and 1 is South (bottom)
        let lat_clamped = lat_rad.clamp(-1.4844, 1.4844); // ~85.051 deg
        let q = (std::f64::consts::FRAC_PI_4 + lat_clamped / 2.0).tan().ln();
        let q_min = (std::f64::consts::FRAC_PI_4 + min_lat.clamp(-1.4844, 1.4844) / 2.0).tan().ln();
        let q_max = (std::f64::consts::FRAC_PI_4 + max_lat.clamp(-1.4844, 1.4844) / 2.0).tan().ln();

        let v = if (q_max - q_min).abs() > 1e-12 {
            ((q_max - q) / (q_max - q_min)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let col_f = u * ((self.width - 1) as f64);
        let row_f = v * ((self.height - 1) as f64);

        let col0 = (col_f.floor() as usize).min(self.width - 1);
        let col1 = (col0 + 1).min(self.width - 1);
        let row0 = (row_f.floor() as usize).min(self.height - 1);
        let row1 = (row0 + 1).min(self.height - 1);

        let tx = (col_f - col0 as f64).clamp(0.0, 1.0);
        let ty = (row_f - row0 as f64).clamp(0.0, 1.0);

        let z00 = self.elevations[row0 * self.width + col0] as f64;
        let z01 = self.elevations[row0 * self.width + col1] as f64;
        let z10 = self.elevations[row1 * self.width + col0] as f64;
        let z11 = self.elevations[row1 * self.width + col1] as f64;

        // Bilinear interpolation
        let z_top = z00 * (1.0 - tx) + z01 * tx;
        let z_bottom = z10 * (1.0 - tx) + z11 * tx;
        let z_final = z_top * (1.0 - ty) + z_bottom * ty;

        Some(z_final)
    }
}
