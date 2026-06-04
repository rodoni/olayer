use crate::geodesy::coords::LatLon;
use crate::geodesy::ellipsoid::Ellipsoid;
use crate::geodesy::math::normalize_longitude;
use super::{Projection, ProjectionError};

/// Safe latitude limit for LCC to avoid numerical instability at the poles.
const CLAMP_LIMIT: f64 = 89.9_f64.to_radians();

/// Lambert Conformal Conic (LCC) projection.
///
/// A conic conformal projection defined by two standard parallels, an origin
/// latitude and a central meridian. Ideal for mid-latitude regions such as
/// continental En-Route charts.
pub struct LambertConformalConic {
    pub std_parallel_1: f64,
    pub std_parallel_2: f64,
    pub origin_lat: f64,
    pub origin_lon: f64,
    pub ellipsoid: Ellipsoid,
    // Cached projection constants
    n: f64,
    f_c: f64,
    rho_0: f64,
    e: f64, // cached sqrt(e_sq)
}

impl LambertConformalConic {
    /// Creates a new Lambert Conformal Conic projection.
    #[inline]
    pub fn new(
        std_parallel_1_rad: f64,
        std_parallel_2_rad: f64,
        origin_lat_rad: f64,
        origin_lon_rad: f64,
        ellipsoid: Ellipsoid,
    ) -> Self {
        let a = ellipsoid.a;
        let e_sq = ellipsoid.e_sq;
        let e = e_sq.sqrt();

        let phi1 = std_parallel_1_rad;
        let phi2 = std_parallel_2_rad;
        let phi0 = origin_lat_rad;
        let sin_phi0 = phi0.sin();
        let sin_phi1 = phi1.sin();
        let cos_phi1 = phi1.cos();
        let sin_phi2 = phi2.sin();
        let cos_phi2 = phi2.cos();

        let m1 = cos_phi1 / (1.0 - e_sq * sin_phi1 * sin_phi1).sqrt();
        let m2 = cos_phi2 / (1.0 - e_sq * sin_phi2 * sin_phi2).sqrt();

        let t1 = (std::f64::consts::FRAC_PI_4 - phi1 / 2.0).tan()
            * ((1.0 + e * sin_phi1) / (1.0 - e * sin_phi1)).powf(e / 2.0);
        let t2 = (std::f64::consts::FRAC_PI_4 - phi2 / 2.0).tan()
            * ((1.0 + e * sin_phi2) / (1.0 - e * sin_phi2)).powf(e / 2.0);
        let t0 = (std::f64::consts::FRAC_PI_4 - phi0 / 2.0).tan()
            * ((1.0 + e * sin_phi0) / (1.0 - e * sin_phi0)).powf(e / 2.0);

        let n = if (phi1 - phi2).abs() < 1e-10 {
            sin_phi1
        } else {
            (m1.ln() - m2.ln()) / (t1.ln() - t2.ln())
        };

        let f_c = m1 / (n * t1.powf(n));
        let rho_0 = a * f_c * t0.powf(n);

        Self {
            std_parallel_1: phi1,
            std_parallel_2: phi2,
            origin_lat: phi0,
            origin_lon: origin_lon_rad,
            ellipsoid,
            n,
            f_c,
            rho_0,
            e,
        }
    }
}

impl Projection for LambertConformalConic {
    #[inline]
    fn project(&self, lla: &LatLon) -> Result<(f64, f64), ProjectionError> {
        debug_assert!(lla.validate().is_ok(), "Invalid LLA in LCC::project: {lla:?}");

        let lat = lla.lat;
        let lon = lla.lon;

        // Guard against latitudes near the poles where the projection formula
        // becomes numerically unstable.
        debug_assert!(
            lat.abs() <= CLAMP_LIMIT,
            "LCC::project received latitude beyond safe limit (|{}| > {}); clamping will be applied",
            lat.to_degrees(),
            CLAMP_LIMIT.to_degrees()
        );
        let clamped_lat = lat.clamp(-CLAMP_LIMIT, CLAMP_LIMIT);
        let sin_clamped = clamped_lat.sin();

        let t = (std::f64::consts::FRAC_PI_4 - clamped_lat / 2.0).tan()
            * ((1.0 + self.e * sin_clamped) / (1.0 - self.e * sin_clamped)).powf(self.e / 2.0);

        let rho = self.ellipsoid.a * self.f_c * t.powf(self.n);
        let theta = self.n * (lon - self.origin_lon);

        let x = rho * theta.sin();
        let y = self.rho_0 - rho * theta.cos();

        Ok((x, y))
    }

    #[inline]
    fn unproject(&self, x: f64, y: f64) -> Result<LatLon, ProjectionError> {
        let rho_0_y = self.rho_0 - y;
        let rho = x.hypot(rho_0_y).copysign(self.n);

        let theta = if self.n < 0.0 {
            (-x).atan2(-rho_0_y)
        } else {
            x.atan2(rho_0_y)
        };

        let lon = self.origin_lon + theta / self.n;
        let lon_normalized = normalize_longitude(lon);

        let t = if rho.abs() < 1e-10 {
            0.0
        } else {
            (rho / (self.ellipsoid.a * self.f_c)).powf(1.0 / self.n)
        };

        // Iteratively solve for latitude
        let mut lat = std::f64::consts::FRAC_PI_2 - 2.0 * t.atan();
        let mut converged = false;

        for _ in 0..15 {
            let sin_lat = lat.sin();
            let con = self.e * sin_lat;
            let lat_next = std::f64::consts::FRAC_PI_2
                - 2.0 * (t * ((1.0 - con) / (1.0 + con)).powf(self.e / 2.0)).atan();
            if (lat_next - lat).abs() < 1e-12 {
                lat = lat_next;
                converged = true;
                break;
            }
            lat = lat_next;
        }

        if !converged {
            return Err(ProjectionError::ConvergenceFailed);
        }

        Ok(LatLon::new(lat, lon_normalized, 0.0))
    }
}
