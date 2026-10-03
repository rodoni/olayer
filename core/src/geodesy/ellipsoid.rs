#![expect(
    clippy::unreadable_literal,
    reason = "Published geodetic constants preserve precision"
)]

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ellipsoid {
    a: f64,
    b: f64,
    f: f64,
    e_sq: f64,
    e_prime_sq: f64,
    authalic_radius: f64,
}

impl Ellipsoid {
    /// Creates a new reference ellipsoid from the semi-major axis (a) and flattening (f).
    ///
    /// # Errors
    /// Returns [`GeodesyError::InvalidSemiMajorAxis`] or
    /// [`GeodesyError::InvalidFlattening`] when the parameters are non-finite
    /// or outside their physical ranges.
    #[inline]
    pub fn new(a: f64, f: f64) -> Result<Self, GeodesyError> {
        if !a.is_finite() || a <= 0.0 {
            return Err(GeodesyError::InvalidSemiMajorAxis(a));
        }
        if !f.is_finite() || !(0.0..1.0).contains(&f) {
            return Err(GeodesyError::InvalidFlattening(f));
        }
        let b = a * (1.0 - f);
        let e_sq = f * (2.0 - f);
        let e_prime_sq = e_sq / (1.0 - e_sq);
        // Authalic radius: radius of a sphere with the same surface area as the ellipsoid.
        // Approximation used for spherical distance formulas (Haversine).
        let authalic_radius = (2.0 * a + b) / 3.0;
        if !b.is_finite()
            || !e_sq.is_finite()
            || !e_prime_sq.is_finite()
            || !authalic_radius.is_finite()
        {
            return Err(GeodesyError::InvalidSemiMajorAxis(a));
        }
        Ok(Self {
            a,
            b,
            f,
            e_sq,
            e_prime_sq,
            authalic_radius,
        })
    }

    /// Returns the standard WGS84 ellipsoid configuration.
    #[inline]
    pub const fn wgs84() -> Self {
        let a = 6378137.0;
        let f = 1.0 / 298.257223563;
        let b = a * (1.0 - f);
        let e_sq = f * (2.0 - f);
        let e_prime_sq = e_sq / (1.0 - e_sq);
        Self {
            a,
            b,
            f,
            e_sq,
            e_prime_sq,
            authalic_radius: (2.0 * a + b) / 3.0,
        }
    }

    /// Computes the radius of curvature in the prime vertical (N) for a given latitude in radians.
    #[inline]
    pub fn radius_of_curvature_prime_vertical(&self, lat_rad: f64) -> f64 {
        let sin_lat = lat_rad.sin();
        self.a / (1.0 - self.e_sq * sin_lat * sin_lat).sqrt()
    }

    #[inline]
    pub const fn a(&self) -> f64 {
        self.a
    }
    #[inline]
    pub const fn b(&self) -> f64 {
        self.b
    }
    #[inline]
    pub const fn f(&self) -> f64 {
        self.f
    }
    #[inline]
    pub const fn e_sq(&self) -> f64 {
        self.e_sq
    }
    #[inline]
    pub const fn e_prime_sq(&self) -> f64 {
        self.e_prime_sq
    }
    #[inline]
    pub const fn authalic_radius(&self) -> f64 {
        self.authalic_radius
    }
}
use crate::geodesy::errors::GeodesyError;
