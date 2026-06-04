#![allow(clippy::many_single_char_names)]

use crate::geodesy::coords::LatLon;
use crate::geodesy::errors::GeodesyError;
use crate::geodesy::ellipsoid::Ellipsoid;
use crate::geodesy::math::{normalize_bearing, normalize_longitude};
use crate::geodesy::solvers::haversine::HaversineSolver;
use crate::geodesy::solvers::{GeodeticResult, GeodeticSolver};

pub struct VincentySolver;

impl Default for VincentySolver {
    #[inline]
    fn default() -> Self {
        Self
    }
}

impl GeodeticSolver for VincentySolver {
    const IS_ELLIPSOIDAL: bool = true;
    const EXPECTED_ACCURACY_METERS: f64 = 1e-3;

    #[inline]
    fn inverse(&self, p1: &LatLon, p2: &LatLon, ellipsoid: &Ellipsoid) -> Result<GeodeticResult, GeodesyError> {
        debug_assert!(p1.validate().is_ok(), "Invalid start coordinate in Vincenty::inverse: {p1:?}");
        debug_assert!(p2.validate().is_ok(), "Invalid end coordinate in Vincenty::inverse: {p2:?}");

        let lat1 = p1.lat;
        let lon1 = p1.lon;
        let lat2 = p2.lat;
        let lon2 = p2.lon;

        // If points are coincident, return zero distance and zero bearing
        if (lat1 - lat2).abs() < 1e-12 && (lon1 - lon2).abs() < 1e-12 {
            return Ok(GeodeticResult {
                distance: 0.0,
                initial_bearing: 0.0,
                final_bearing: 0.0,
            });
        }

        let f = ellipsoid.f;
        let a = ellipsoid.a;
        let b = ellipsoid.b;

        // Reduced latitudes (guard against tan(pi/2) blow-up at poles)
        let u1 = if lat1.abs() >= std::f64::consts::FRAC_PI_2 - 1e-12 {
            lat1.signum() * std::f64::consts::FRAC_PI_2
        } else {
            ((1.0 - f) * lat1.tan()).atan()
        };

        let u2 = if lat2.abs() >= std::f64::consts::FRAC_PI_2 - 1e-12 {
            lat2.signum() * std::f64::consts::FRAC_PI_2
        } else {
            ((1.0 - f) * lat2.tan()).atan()
        };

        let l = lon2 - lon1;

        let sin_u1 = u1.sin();
        let cos_u1 = u1.cos();
        let sin_u2 = u2.sin();
        let cos_u2 = u2.cos();

        let mut lambda = l;
        let mut lambda_prev;

        let max_iterations = 200;
        let convergence_threshold = 1e-12;
        let mut converged = false;

        let mut sin_sigma = 0.0;
        let mut cos_sigma = 0.0;
        let mut sigma = 0.0;
        let mut cos2_alpha = 0.0;
        let mut cos2_sigma_m = 0.0;

        for _ in 0..max_iterations {
            lambda_prev = lambda;

            let sin_lambda = lambda.sin();
            let cos_lambda = lambda.cos();

            sin_sigma = ((cos_u2 * sin_lambda).powi(2)
                + (cos_u1 * sin_u2 - sin_u1 * cos_u2 * cos_lambda).powi(2))
            .sqrt();

            if sin_sigma == 0.0 {
                converged = true;
                break; // Coincident points
            }

            cos_sigma = sin_u1 * sin_u2 + cos_u1 * cos_u2 * cos_lambda;
            sigma = sin_sigma.atan2(cos_sigma);

            let sin_alpha = cos_u1 * cos_u2 * sin_lambda / sin_sigma;
            cos2_alpha = 1.0 - sin_alpha * sin_alpha;

            cos2_sigma_m = if cos2_alpha == 0.0 {
                0.0
            } else {
                cos_sigma - 2.0 * sin_u1 * sin_u2 / cos2_alpha
            };

            let c = f / 16.0 * cos2_alpha * (4.0 + f * (4.0 - 3.0 * cos2_alpha));
            lambda = l
                + (1.0 - c)
                    * f
                    * sin_alpha
                    * (sigma
                        + c * sin_sigma
                            * (cos2_sigma_m
                                + c * cos_sigma * (-1.0 + 2.0 * cos2_sigma_m * cos2_sigma_m)));

            if (lambda - lambda_prev).abs() < convergence_threshold {
                converged = true;
                break;
            }
        }

        if !converged || lambda.is_nan() {
            // Fallback to Haversine for antipodal points or non-convergence
            return HaversineSolver.inverse(p1, p2, ellipsoid);
        }

        let u_sq = cos2_alpha * (a * a - b * b) / (b * b);
        let a_coeff = 1.0 + u_sq / 16384.0 * (4096.0 + u_sq * (-768.0 + u_sq * (320.0 - 175.0 * u_sq)));
        let b_coeff = u_sq / 1024.0 * (256.0 + u_sq * (-128.0 + u_sq * (74.0 - 47.0 * u_sq)));

        let delta_sigma = b_coeff
            * sin_sigma
            * (cos2_sigma_m
                + 0.25
                    * b_coeff
                    * (cos_sigma * (-1.0 + 2.0 * cos2_sigma_m * cos2_sigma_m)
                        - 1.0 / 6.0
                            * b_coeff
                            * cos2_sigma_m
                            * (-3.0 + 4.0 * sin_sigma * sin_sigma)
                            * (-3.0 + 4.0 * cos2_sigma_m * cos2_sigma_m)));

        let distance = b * a_coeff * (sigma - delta_sigma);

        let initial_bearing = normalize_bearing((cos_u2 * lambda.sin()).atan2(
            cos_u1 * sin_u2 - sin_u1 * cos_u2 * lambda.cos(),
        ));

        let final_bearing = normalize_bearing((cos_u1 * lambda.sin()).atan2(
            -sin_u1 * cos_u2 + cos_u1 * sin_u2 * lambda.cos(),
        ));

        Ok(GeodeticResult::new(distance, initial_bearing, final_bearing))
    }

    #[inline]
    fn direct(&self, p1: &LatLon, bearing_rad: f64, distance_meters: f64, ellipsoid: &Ellipsoid) -> Result<LatLon, GeodesyError> {
        debug_assert!(p1.validate().is_ok(), "Invalid start coordinate in Vincenty::direct: {p1:?}");

        if distance_meters.abs() < 1e-12 {
            return Ok(*p1);
        }

        let f = ellipsoid.f;
        let a = ellipsoid.a;
        let b = ellipsoid.b;

        let alpha1 = bearing_rad;
        let sin_alpha1 = alpha1.sin();
        let cos_alpha1 = alpha1.cos();

        // Reduced latitudes (guard against tan(pi/2) blow-up at poles)
        let u1 = if p1.lat.abs() >= std::f64::consts::FRAC_PI_2 - 1e-12 {
            p1.lat.signum() * std::f64::consts::FRAC_PI_2
        } else {
            ((1.0 - f) * p1.lat.tan()).atan()
        };

        let sin_u1 = u1.sin();
        let cos_u1 = u1.cos();

        let sigma1 = sin_u1.atan2(cos_u1 * cos_alpha1);
        let sin_alpha = cos_u1 * sin_alpha1;
        let cos2_alpha = 1.0 - sin_alpha * sin_alpha;

        let u_sq = cos2_alpha * (a * a - b * b) / (b * b);
        let a_coeff = 1.0 + u_sq / 16384.0 * (4096.0 + u_sq * (-768.0 + u_sq * (320.0 - 175.0 * u_sq)));
        let b_coeff = u_sq / 1024.0 * (256.0 + u_sq * (-128.0 + u_sq * (74.0 - 47.0 * u_sq)));

        let mut sigma = distance_meters / (b * a_coeff);
        let mut sigma_prev;
        let mut cos2_sigma_m = 0.0;

        let max_iterations = 200;
        let convergence_threshold = 1e-12;
        let mut converged = false;

        for _ in 0..max_iterations {
            sigma_prev = sigma;

            cos2_sigma_m = (2.0 * sigma1 + sigma).cos();
            let sin_sigma = sigma.sin();
            let cos_sigma = sigma.cos();

            let delta_sigma = b_coeff
                * sin_sigma
                * (cos2_sigma_m
                    + 0.25
                        * b_coeff
                        * (cos_sigma * (-1.0 + 2.0 * cos2_sigma_m * cos2_sigma_m)
                            - 1.0 / 6.0
                                * b_coeff
                                * cos2_sigma_m
                                * (-3.0 + 4.0 * sin_sigma * sin_sigma)
                                * (-3.0 + 4.0 * cos2_sigma_m * cos2_sigma_m)));

            sigma = distance_meters / (b * a_coeff) + delta_sigma;

            if (sigma - sigma_prev).abs() < convergence_threshold {
                converged = true;
                break;
            }
        }

        if !converged || sigma.is_nan() {
            // Fallback to Haversine in case of convergence issues
            return HaversineSolver.direct(p1, bearing_rad, distance_meters, ellipsoid);
        }

        let sin_sigma = sigma.sin();
        let cos_sigma = sigma.cos();

        let lat2 = (sin_u1 * cos_sigma + cos_u1 * sin_sigma * cos_alpha1).atan2(
            (1.0 - f)
                * (sin_alpha * sin_alpha
                    + (sin_u1 * sin_sigma - cos_u1 * cos_sigma * cos_alpha1).powi(2))
                .sqrt(),
        );

        let lambda = (sin_sigma * sin_alpha1).atan2(cos_u1 * cos_sigma - sin_u1 * sin_sigma * cos_alpha1);
        let c = f / 16.0 * cos2_alpha * (4.0 + f * (4.0 - 3.0 * cos2_alpha));
        let l = lambda
            - (1.0 - c)
                * f
                * sin_alpha
                * (sigma
                    + c * sin_sigma
                        * (cos2_sigma_m
                            + c * cos_sigma * (-1.0 + 2.0 * cos2_sigma_m * cos2_sigma_m)));

        let lon2 = p1.lon + l;

        let lon2_normalized = normalize_longitude(lon2);

        Ok(LatLon::new(lat2, lon2_normalized, p1.height))
    }
}
