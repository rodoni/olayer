use crate::geodesy::conversions::{ecef_to_enu, ecef_to_lla, enu_to_ecef, lla_to_ecef};
use crate::geodesy::coords::{Ecef, Enu, LatLon};
use crate::geodesy::ellipsoid::Ellipsoid;
use crate::geodesy::math::normalize_bearing;
use serde::{Deserialize, Serialize};

/// Standard effective Earth radius factor for radar atmospheric refraction in the troposphere.
pub const STANDARD_RADAR_K_FACTOR: f64 = 4.0 / 3.0;

/// Represents a point in a local East-North-Up (ENU) Cartesian frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnuPoint {
    /// East displacement in meters.
    pub east_m: f64,
    /// North displacement in meters.
    pub north_m: f64,
    /// Up displacement in meters.
    pub up_m: f64,
}

impl EnuPoint {
    /// Creates a new ENU point in meters.
    #[inline]
    pub const fn new(east_m: f64, north_m: f64, up_m: f64) -> Self {
        Self {
            east_m,
            north_m,
            up_m,
        }
    }

    /// Calculates 2D horizontal distance (ground plane distance) in meters.
    #[inline]
    pub fn distance_2d(&self) -> f64 {
        self.east_m.hypot(self.north_m)
    }

    /// Calculates 3D Euclidean distance (slant distance) in meters.
    #[inline]
    pub fn distance_3d(&self) -> f64 {
        self.east_m.hypot(self.north_m).hypot(self.up_m)
    }

    /// Converts this ENU point to a North-East-Down (NED) point.
    #[inline]
    pub const fn to_ned(&self) -> NedPoint {
        NedPoint {
            north_m: self.north_m,
            east_m: self.east_m,
            down_m: -self.up_m,
        }
    }

    /// Converts from a North-East-Down (NED) point.
    #[inline]
    pub const fn from_ned(ned: &NedPoint) -> Self {
        Self {
            east_m: ned.east_m,
            north_m: ned.north_m,
            up_m: -ned.down_m,
        }
    }
}

impl From<Enu> for EnuPoint {
    #[inline]
    fn from(enu: Enu) -> Self {
        Self {
            east_m: enu.east,
            north_m: enu.north,
            up_m: enu.up,
        }
    }
}

impl From<EnuPoint> for Enu {
    #[inline]
    fn from(pt: EnuPoint) -> Self {
        Self {
            east: pt.east_m,
            north: pt.north_m,
            up: pt.up_m,
        }
    }
}

/// Represents a point in a local North-East-Down (NED) Cartesian frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NedPoint {
    /// North displacement in meters.
    pub north_m: f64,
    /// East displacement in meters.
    pub east_m: f64,
    /// Down displacement in meters.
    pub down_m: f64,
}

impl NedPoint {
    /// Creates a new NED point in meters.
    #[inline]
    pub const fn new(north_m: f64, east_m: f64, down_m: f64) -> Self {
        Self {
            north_m,
            east_m,
            down_m,
        }
    }

    /// Calculates 2D horizontal distance in meters.
    #[inline]
    pub fn distance_2d(&self) -> f64 {
        self.north_m.hypot(self.east_m)
    }

    /// Calculates 3D Euclidean distance in meters.
    #[inline]
    pub fn distance_3d(&self) -> f64 {
        self.north_m.hypot(self.east_m).hypot(self.down_m)
    }

    /// Converts this NED point to an East-North-Up (ENU) point.
    #[inline]
    pub const fn to_enu(&self) -> EnuPoint {
        EnuPoint {
            east_m: self.east_m,
            north_m: self.north_m,
            up_m: -self.down_m,
        }
    }

    /// Converts from an East-North-Up (ENU) point.
    #[inline]
    pub const fn from_enu(enu: &EnuPoint) -> Self {
        Self {
            north_m: enu.north_m,
            east_m: enu.east_m,
            down_m: -enu.up_m,
        }
    }
}

/// A Local Tangent Frame centered at a reference origin (e.g. airport reference point, radar tower).
///
/// Converts geodetic LLA coordinates to/from local topocentric ENU and NED frames,
/// and calculates radar look angles (slant range, azimuth, elevation) with optional
/// 4/3 tropospheric refraction modeling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalTangentFrame {
    pub origin: LatLon,
    origin_ecef: Ecef,
    ellipsoid: Ellipsoid,
}

impl LocalTangentFrame {
    /// Creates a new `LocalTangentFrame` with the given origin on the WGS84 ellipsoid.
    pub fn new(origin: LatLon) -> Self {
        Self::with_ellipsoid(origin, Ellipsoid::wgs84())
    }

    /// Creates a new `LocalTangentFrame` with the given origin and custom reference ellipsoid.
    pub fn with_ellipsoid(origin: LatLon, ellipsoid: Ellipsoid) -> Self {
        let origin_ecef = lla_to_ecef(&origin, &ellipsoid);
        Self {
            origin,
            origin_ecef,
            ellipsoid,
        }
    }

    /// Returns the reference ellipsoid used by this frame.
    #[inline]
    pub fn ellipsoid(&self) -> &Ellipsoid {
        &self.ellipsoid
    }

    /// Returns the precomputed ECEF position of the frame origin.
    #[inline]
    pub fn origin_ecef(&self) -> &Ecef {
        &self.origin_ecef
    }

    /// Converts Geodetic (LLA) coordinates to local ENU coordinates relative to the origin.
    pub fn lla_to_enu(&self, lla: &LatLon) -> EnuPoint {
        let ecef = lla_to_ecef(lla, &self.ellipsoid);
        let enu = ecef_to_enu(&ecef, &self.origin, &self.ellipsoid);
        EnuPoint::from(enu)
    }

    /// Converts local ENU coordinates to Geodetic (LLA) coordinates relative to the origin.
    pub fn enu_to_lla(&self, enu: &EnuPoint) -> LatLon {
        let enu_core = Enu::from(*enu);
        let ecef = enu_to_ecef(&enu_core, &self.origin, &self.ellipsoid);
        ecef_to_lla(&ecef, &self.ellipsoid)
    }

    /// Converts Geodetic (LLA) coordinates to local NED coordinates relative to the origin.
    pub fn lla_to_ned(&self, lla: &LatLon) -> NedPoint {
        self.lla_to_enu(lla).to_ned()
    }

    /// Converts local NED coordinates to Geodetic (LLA) coordinates relative to the origin.
    pub fn ned_to_lla(&self, ned: &NedPoint) -> LatLon {
        self.enu_to_lla(&ned.to_enu())
    }

    /// Computes geometric radar look angles to a target coordinate without atmospheric refraction.
    ///
    /// # Returns
    /// A tuple `(slant_range_m, azimuth_rad, elevation_rad)`:
    /// - `slant_range_m`: Line-of-sight Euclidean distance in meters.
    /// - `azimuth_rad`: True azimuth angle clockwise from North in radians `[0, 2π)`.
    /// - `elevation_rad`: Elevation angle above local horizontal plane in radians `[-π/2, π/2]`.
    pub fn radar_look_angles(&self, target: &LatLon) -> (f64, f64, f64) {
        let enu = self.lla_to_enu(target);
        let slant_range = enu.distance_3d();
        if slant_range < 1e-12 {
            return (0.0, 0.0, 0.0);
        }

        let azimuth = normalize_bearing(enu.east_m.atan2(enu.north_m));
        let elevation = (enu.up_m / slant_range).clamp(-1.0, 1.0).asin();

        (slant_range, azimuth, elevation)
    }

    /// Computes radar look angles incorporating atmospheric refraction bending via the
    /// effective Earth radius factor (standard troposphere $k = 4/3$).
    ///
    /// # Arguments
    /// * `target` - Target geodetic coordinates (lat, lon, height).
    /// * `k_factor` - Effective Earth radius refraction multiplier ($k \approx 1.333333$).
    ///
    /// # Returns
    /// A tuple `(slant_range_m, azimuth_rad, refracted_elevation_rad)`.
    pub fn radar_look_angles_refracted(&self, target: &LatLon, k_factor: f64) -> (f64, f64, f64) {
        let enu = self.lla_to_enu(target);
        let slant_range = enu.distance_3d();
        if slant_range < 1e-12 {
            return (0.0, 0.0, 0.0);
        }

        let azimuth = normalize_bearing(enu.east_m.atan2(enu.north_m));
        let d_2d = enu.distance_2d();

        // Tropospheric downward bending: effective Earth radius R_eff = k * a
        let effective_radius = k_factor * self.ellipsoid.a;
        let delta_h = (d_2d * d_2d) / (2.0 * effective_radius);
        let effective_up = enu.up_m - delta_h;

        let elevation = (effective_up / slant_range).clamp(-1.0, 1.0).asin();

        (slant_range, azimuth, elevation)
    }
}
