#![allow(clippy::unreadable_literal)]

use crate::geodesy::coords::LatLon;
use crate::geodesy::ellipsoid::Ellipsoid;
use crate::geodesy::math::normalize_longitude;
use super::{Projection, ProjectionError};

/// Latitude limit for Web Mercator (~85.05112878 degrees).
const WEB_MERCATOR_LIMIT: f64 = 85.05112878_f64.to_radians();

/// Web Mercator projection (EPSG:3857).
///
/// A cylindrical projection used by virtually all commercial tile providers.
/// It is mathematically defined on a sphere of radius `a` (WGS84 semi-major axis).
/// Passing an ellipsoid other than WGS84 is a logic error.
pub struct WebMercator {
    pub ellipsoid: Ellipsoid,
}

impl Default for WebMercator {
    #[inline]
    fn default() -> Self {
        Self::new(Ellipsoid::wgs84())
    }
}

impl WebMercator {
    /// Creates a new Web Mercator projection.
    #[inline]
    pub fn new(ellipsoid: Ellipsoid) -> Self {
        debug_assert!(
            (ellipsoid.a - 6378137.0).abs() < 1e-3 && (ellipsoid.f - 1.0 / 298.257223563).abs() < 1e-12,
            "WebMercator is defined on the WGS84 sphere; non-WGS84 ellipsoid passed"
        );
        Self { ellipsoid }
    }
}

impl Projection for WebMercator {
    #[inline]
    fn project(&self, lla: &LatLon) -> Result<(f64, f64), ProjectionError> {
        debug_assert!(lla.validate().is_ok(), "Invalid LLA in WebMercator::project: {lla:?}");

        let lat = lla.lat;
        let lon = lla.lon;
        let a = self.ellipsoid.a;

        // Clamp latitude to standard Web Mercator limits to avoid infinite y values at the poles.
        debug_assert!(
            lat.abs() <= WEB_MERCATOR_LIMIT,
            "WebMercator::project latitude {} exceeds limit {}; clamping applied",
            lat.to_degrees(),
            WEB_MERCATOR_LIMIT.to_degrees()
        );
        let clamped_lat = lat.clamp(-WEB_MERCATOR_LIMIT, WEB_MERCATOR_LIMIT);

        let x = a * lon;
        let y = a * (std::f64::consts::FRAC_PI_4 + clamped_lat / 2.0).tan().ln();

        Ok((x, y))
    }

    #[inline]
    fn unproject(&self, x: f64, y: f64) -> Result<LatLon, ProjectionError> {
        let a = self.ellipsoid.a;

        let lon = x / a;
        let lat = 2.0 * (y / a).exp().atan() - std::f64::consts::FRAC_PI_2;

        // Clamp back to valid Web Mercator limits
        let clamped_lat = lat.clamp(-WEB_MERCATOR_LIMIT, WEB_MERCATOR_LIMIT);
        let lon_normalized = normalize_longitude(lon);

        Ok(LatLon::new(clamped_lat, lon_normalized, 0.0))
    }
}
