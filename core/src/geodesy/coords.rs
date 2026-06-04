use crate::geodesy::errors::GeodesyError;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LatLon {
    pub lat: f64,    // Latitude in radians
    pub lon: f64,    // Longitude in radians
    pub height: f64, // Height above ellipsoid in meters
}

impl LatLon {
    /// Creates a new geodetic coordinate in radians.
    #[inline]
    pub const fn new(lat_rad: f64, lon_rad: f64, height_meters: f64) -> Self {
        Self {
            lat: lat_rad,
            lon: lon_rad,
            height: height_meters,
        }
    }

    /// Creates a new geodetic coordinate from degrees.
    #[inline]
    pub fn from_degrees(lat_deg: f64, lon_deg: f64, height_meters: f64) -> Self {
        Self {
            lat: lat_deg.to_radians(),
            lon: lon_deg.to_radians(),
            height: height_meters,
        }
    }

    /// Converts the geodetic coordinate to a tuple of (latitude, longitude, height) in degrees and meters.
    #[inline]
    pub fn to_degrees(&self) -> (f64, f64, f64) {
        (self.lat.to_degrees(), self.lon.to_degrees(), self.height)
    }

    /// Validates if the coordinates are in standard ranges.
    #[inline]
    pub fn validate(&self) -> Result<(), GeodesyError> {
        let lat_deg = self.lat.to_degrees();
        let lon_deg = self.lon.to_degrees();
        if !(-90.0..=90.0).contains(&lat_deg) {
            return Err(GeodesyError::LatitudeOutOfRange(lat_deg));
        }
        if !(-180.0..=180.0).contains(&lon_deg) {
            return Err(GeodesyError::LongitudeOutOfRange(lon_deg));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ecef {
    pub x: f64, // in meters
    pub y: f64, // in meters
    pub z: f64, // in meters
}

impl Ecef {
    /// Creates a new Earth-Centered, Earth-Fixed Cartesian coordinate in meters.
    #[inline]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Calculates the Euclidean (chord) distance to another ECEF point.
    ///
    /// **Note:** This measures the straight-line distance *through* the Earth,
    /// not the geodetic (surface) distance. For surface distance, use a
    /// [`GeodeticSolver`](crate::geodesy::solvers::GeodeticSolver).
    pub fn chord_distance(&self, other: &Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        dx.hypot(dy).hypot(dz)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Enu {
    pub east: f64,  // East displacement in meters
    pub north: f64, // North displacement in meters
    pub up: f64,    // Up displacement in meters
}

impl Enu {
    /// Creates a new East-North-Up coordinate.
    #[inline]
    pub const fn new(east: f64, north: f64, up: f64) -> Self {
        Self { east, north, up }
    }

    /// Calculates 2D distance (horizontal distance) in ENU coordinates.
    #[inline]
    pub fn distance_2d(&self) -> f64 {
        (self.east * self.east + self.north * self.north).sqrt()
    }

    /// Calculates 3D distance in ENU coordinates.
    #[inline]
    pub fn distance_3d(&self) -> f64 {
        (self.east * self.east + self.north * self.north + self.up * self.up).sqrt()
    }
}
