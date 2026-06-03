use crate::geodesy::coords::LatLon;
use crate::geodesy::ellipsoid::Ellipsoid;
use crate::geodesy::math::normalize_longitude;
use super::{Projection, ProjectionError};

/// Ellipsoidal Stereographic (Azimuthal) projection.
///
/// Preserves angles locally around a center of projection. Commonly used for
/// terminal radar displays (TMA) where the antenna location is the tangent point.
pub struct Stereographic {
    pub center_lat: f64,
    pub center_lon: f64,
    pub ellipsoid: Ellipsoid,
    // Cached constants
    chi_c: f64,
    r_c: f64,
    e: f64, // cached sqrt(e_sq)
}

impl Stereographic {
    /// Creates a new Ellipsoidal Stereographic projection.
    #[inline]
    pub fn new(center_lat_rad: f64, center_lon_rad: f64, ellipsoid: Ellipsoid) -> Self {
        let a = ellipsoid.a;
        let e_sq = ellipsoid.e_sq;
        let e = e_sq.sqrt();

        let phi_c = center_lat_rad;
        let sin_phi_c = phi_c.sin();

        // Conformal latitude of the origin (chi_c)
        let t_c = (std::f64::consts::FRAC_PI_4 + phi_c / 2.0).tan()
            * ((1.0 - e * sin_phi_c) / (1.0 + e * sin_phi_c)).powf(e / 2.0);
        let chi_c = 2.0 * t_c.atan() - std::f64::consts::FRAC_PI_2;

        // Radius of conformal sphere (R_c)
        let r_c = (a * (1.0 - e_sq).sqrt()) / (1.0 - e_sq * sin_phi_c * sin_phi_c);

        Self {
            center_lat: phi_c,
            center_lon: center_lon_rad,
            ellipsoid,
            chi_c,
            r_c,
            e,
        }
    }
}

impl Projection for Stereographic {
    #[inline]
    fn project(&self, lla: &LatLon) -> Result<(f64, f64), ProjectionError> {
        debug_assert!(lla.validate().is_ok(), "Invalid LLA in Stereographic::project: {:?}", lla);

        let lat = lla.lat;
        let lon = lla.lon;

        let sin_lat = lat.sin();

        // Conformal latitude of target (chi)
        let t = (std::f64::consts::FRAC_PI_4 + lat / 2.0).tan()
            * ((1.0 - self.e * sin_lat) / (1.0 + self.e * sin_lat)).powf(self.e / 2.0);
        let chi = 2.0 * t.atan() - std::f64::consts::FRAC_PI_2;

        let dlon = lon - self.center_lon;

        let cos_chi = chi.cos();
        let sin_chi = chi.sin();
        let cos_chi_c = self.chi_c.cos();
        let sin_chi_c = self.chi_c.sin();
        let cos_dlon = dlon.cos();
        let sin_dlon = dlon.sin();

        let denom = 1.0 + sin_chi_c * sin_chi + cos_chi_c * cos_chi * cos_dlon;

        // Singular case: antipodal point to the center of projection
        if denom.abs() < 1e-10 {
            return Err(ProjectionError::Singularity);
        }

        let k_prime = 2.0 * self.r_c / denom;
        let x = k_prime * cos_chi * sin_dlon;
        let y = k_prime * (cos_chi_c * sin_chi - sin_chi_c * cos_chi * cos_dlon);

        Ok((x, y))
    }

    #[inline]
    fn unproject(&self, x: f64, y: f64) -> Result<LatLon, ProjectionError> {
        let rho = x.hypot(y);

        if rho.abs() < 1e-10 {
            return Ok(LatLon::new(self.center_lat, self.center_lon, 0.0));
        }

        let c = 2.0 * (rho / (2.0 * self.r_c)).atan();
        let sin_c = c.sin();
        let cos_c = c.cos();
        let sin_chi_c = self.chi_c.sin();
        let cos_chi_c = self.chi_c.cos();

        // Conformal latitude chi
        let chi = (cos_c * sin_chi_c + (y * sin_c * cos_chi_c) / rho).asin();

        // Longitude
        let dlon = (x * sin_c).atan2(rho * cos_chi_c * cos_c - y * sin_chi_c * sin_c);
        let lon = self.center_lon + dlon;
        let lon_normalized = normalize_longitude(lon);

        // Solve for geodetic latitude iteratively from conformal latitude
        let t_prime = (std::f64::consts::FRAC_PI_4 - chi / 2.0).tan();
        let mut lat = chi; // initial guess
        let mut converged = false;

        for _ in 0..15 {
            let sin_lat = lat.sin();
            let con = self.e * sin_lat;
            let lat_next = std::f64::consts::FRAC_PI_2
                - 2.0 * (t_prime * ((1.0 - con) / (1.0 + con)).powf(self.e / 2.0)).atan();
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
