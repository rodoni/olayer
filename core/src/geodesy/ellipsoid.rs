#![allow(clippy::unreadable_literal)]

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ellipsoid {
    pub a: f64,                   // Semi-major axis in meters
    pub b: f64,                   // Semi-minor axis in meters
    pub f: f64,                   // Flattening
    pub e_sq: f64,                // First eccentricity squared
    pub e_prime_sq: f64,          // Second eccentricity squared
    pub authalic_radius: f64,     // Authalic (mean) spherical radius for Haversine approximations
}

impl Ellipsoid {
    /// Creates a new reference ellipsoid from the semi-major axis (a) and flattening (f).
    #[inline]
    pub const fn new(a: f64, f: f64) -> Self {
        let b = a * (1.0 - f);
        let e_sq = f * (2.0 - f);
        let e_prime_sq = e_sq / (1.0 - e_sq);
        // Authalic radius: radius of a sphere with the same surface area as the ellipsoid.
        // Approximation used for spherical distance formulas (Haversine).
        let authalic_radius = (2.0 * a + b) / 3.0;
        Self {
            a,
            b,
            f,
            e_sq,
            e_prime_sq,
            authalic_radius,
        }
    }

    /// Returns the standard WGS84 ellipsoid configuration.
    #[inline]
    pub const fn wgs84() -> Self {
        Self::new(6378137.0, 1.0 / 298.257223563)
    }

    /// Computes the radius of curvature in the prime vertical (N) for a given latitude in radians.
    #[inline]
    pub fn radius_of_curvature_prime_vertical(&self, lat_rad: f64) -> f64 {
        let sin_lat = lat_rad.sin();
        self.a / (1.0 - self.e_sq * sin_lat * sin_lat).sqrt()
    }
}
