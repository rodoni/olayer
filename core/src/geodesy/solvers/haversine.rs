use crate::geodesy::coords::LatLon;
use crate::geodesy::ellipsoid::Ellipsoid;
use crate::geodesy::errors::GeodesyError;
use crate::geodesy::math::{normalize_bearing, normalize_longitude};
use crate::geodesy::solvers::{GeodeticResult, GeodeticSolver};

pub struct HaversineSolver;

impl Default for HaversineSolver {
    #[inline]
    fn default() -> Self {
        Self
    }
}

impl GeodeticSolver for HaversineSolver {
    const IS_ELLIPSOIDAL: bool = false;
    const EXPECTED_ACCURACY_METERS: f64 = 1.0;

    #[inline]
    fn inverse(
        &self,
        p1: &LatLon,
        p2: &LatLon,
        ellipsoid: &Ellipsoid,
    ) -> Result<GeodeticResult, GeodesyError> {
        p1.validate()?;
        p2.validate()?;

        let lat1 = p1.lat;
        let lon1 = p1.lon;
        let lat2 = p2.lat;
        let lon2 = p2.lon;

        let dlat = lat2 - lat1;
        let dlon = lon2 - lon1;

        // Haversine formula
        let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
        let c = 2.0 * a.clamp(0.0, 1.0).sqrt().asin();

        let distance = ellipsoid.authalic_radius * c;

        // Initial bearing
        let y = dlon.sin() * lat2.cos();
        let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
        let initial_bearing = normalize_bearing(y.atan2(x));

        // Final bearing (bearing from p2 to p1 + PI)
        let y_final = (-dlon).sin() * lat1.cos();
        let x_final = lat2.cos() * lat1.sin() - lat2.sin() * lat1.cos() * (-dlon).cos();
        let final_bearing = normalize_bearing(y_final.atan2(x_final) + std::f64::consts::PI);

        Ok(GeodeticResult::new(
            distance,
            initial_bearing,
            final_bearing,
        ))
    }

    #[inline]
    fn direct(
        &self,
        p1: &LatLon,
        bearing_rad: f64,
        distance_meters: f64,
        ellipsoid: &Ellipsoid,
    ) -> Result<LatLon, GeodesyError> {
        p1.validate()?;
        if !bearing_rad.is_finite() {
            return Err(GeodesyError::NonFiniteBearing);
        }
        if !distance_meters.is_finite() {
            return Err(GeodesyError::NonFiniteDistance);
        }
        let lat1 = p1.lat;
        let lon1 = p1.lon;
        let ad = distance_meters / ellipsoid.authalic_radius; // angular distance

        let lat2 = (lat1.sin() * ad.cos() + lat1.cos() * ad.sin() * bearing_rad.cos())
            .clamp(-1.0, 1.0)
            .asin();
        let y = bearing_rad.sin() * ad.sin() * lat1.cos();
        let x = ad.cos() - lat1.sin() * lat2.sin();
        let lon2 = lon1 + y.atan2(x);

        let lon2_normalized = normalize_longitude(lon2);

        Ok(LatLon::new(lat2, lon2_normalized, p1.height))
    }
}
