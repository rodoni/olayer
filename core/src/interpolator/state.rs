use serde::{Deserialize, Serialize};
use crate::geodesy::LatLon;
use crate::interpolator::errors::InterpolatorError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetState {
    pub id: String,
    pub last_position: LatLon,   // Latitude/longitude in radians, altitude in metres
    pub speed_mps: f64,          // Horizontal speed in metres per second
    pub track_heading_rad: f64,  // True track heading in radians [0, 2π)
    pub vertical_rate_mps: f64,  // Vertical speed in metres per second
    pub last_ping_time: f64,     // Sensor timestamp in seconds
}

impl TargetState {
    /// Validates target physical parameters.
    ///
    /// # Errors
    ///
    /// Returns `InterpolatorError::InvalidState` if `speed_mps` is negative or
    /// `track_heading_rad` is outside `[0, 2π]`.
    #[inline]
    pub fn validate(&self) -> Result<(), InterpolatorError> {
        if self.speed_mps < 0.0 {
            return Err(InterpolatorError::InvalidState(format!(
                "Speed must be non-negative: {} mps",
                self.speed_mps
            )));
        }
        if !(0.0..=std::f64::consts::TAU).contains(&self.track_heading_rad) {
            return Err(InterpolatorError::InvalidState(format!(
                "Heading must be in range [0, 2π]: {} rad",
                self.track_heading_rad
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterpolatedTarget {
    pub id: String,
    pub position: LatLon,  // Posição tridimensional interpolada no globo WGS84
    pub heading_rad: f64,  // Rumo interpolado em radianos
}
