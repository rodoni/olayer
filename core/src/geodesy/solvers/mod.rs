pub mod haversine;
pub mod vincenty;

use super::coords::{GeodesyError, LatLon};
use super::ellipsoid::Ellipsoid;

pub use haversine::HaversineSolver;
pub use vincenty::VincentySolver;

/// Result of a geodetic inverse calculation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeodeticResult {
    /// Distance in meters along the geodetic path.
    pub distance: f64,
    /// Initial azimuth/bearing in radians, normalized to `[0, 2π)`.
    pub initial_bearing: f64,
    /// Final azimuth/bearing in radians, normalized to `[0, 2π)`.
    pub final_bearing: f64,
}

impl GeodeticResult {
    /// Creates a new [`GeodeticResult`].
    #[inline]
    pub const fn new(distance: f64, initial_bearing: f64, final_bearing: f64) -> Self {
        Self {
            distance,
            initial_bearing,
            final_bearing,
        }
    }
}

/// Trait for geodetic solvers that can compute the inverse and direct problems
/// on a given reference ellipsoid.
pub trait GeodeticSolver {
    /// `true` if the solver accounts for the ellipsoidal shape of the Earth.
    const IS_ELLIPSOIDAL: bool;

    /// Expected accuracy in meters for a single calculation.
    ///
    /// For ellipsoidal solvers this is typically sub-millimetric.
    /// For spherical approximations the error scales with distance (value
    /// shown here is a conservative order-of-magnitude).
    const EXPECTED_ACCURACY_METERS: f64;

    /// Computes the geodetic distance and bearings between two coordinates (Inverse Problem).
    fn inverse(&self, p1: &LatLon, p2: &LatLon, ellipsoid: &Ellipsoid) -> Result<GeodeticResult, GeodesyError>;

    /// Projects a new coordinate from a starting point, initial bearing (azimuth), and distance (Direct Problem).
    fn direct(&self, p1: &LatLon, bearing_rad: f64, distance_meters: f64, ellipsoid: &Ellipsoid) -> Result<LatLon, GeodesyError>;
}
