use crate::geodesy::LatLon;
use crate::interpolator::errors::InterpolatorError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetState {
    pub id: String,
    pub last_position: LatLon, // Latitude/longitude in radians, altitude in metres
    pub speed_mps: f64,        // Horizontal speed in metres per second
    pub track_heading_rad: f64, // True track heading in radians [0, 2π)
    pub vertical_rate_mps: f64, // Vertical speed in metres per second
    pub last_ping_time: f64,   // Sensor timestamp in seconds
}

impl TargetState {
    /// Validates target physical parameters.
    ///
    /// # Errors
    ///
    /// Returns `InterpolatorError::InvalidState` when coordinates or physical
    /// parameters are non-finite or outside their valid ranges.
    #[inline]
    pub fn validate(&self) -> Result<(), InterpolatorError> {
        if self.id.trim().is_empty() {
            return Err(InterpolatorError::InvalidState(
                "Target identifier must not be empty".into(),
            ));
        }
        self.last_position
            .validate()
            .map_err(|error| InterpolatorError::InvalidState(error.to_string()))?;
        if !self.speed_mps.is_finite() || self.speed_mps < 0.0 {
            return Err(InterpolatorError::InvalidState(format!(
                "Speed must be non-negative: {} mps",
                self.speed_mps
            )));
        }
        if !self.vertical_rate_mps.is_finite() {
            return Err(InterpolatorError::InvalidState(format!(
                "Vertical rate must be finite: {} mps",
                self.vertical_rate_mps
            )));
        }
        if !self.last_ping_time.is_finite() {
            return Err(InterpolatorError::InvalidState(
                "Sensor timestamp must be finite".into(),
            ));
        }
        if !(0.0..std::f64::consts::TAU).contains(&self.track_heading_rad) {
            return Err(InterpolatorError::InvalidState(format!(
                "Heading must be in range [0, 2π]: {} rad",
                self.track_heading_rad
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterpolatedTarget {
    pub id: String,
    pub position: LatLon, // Posição tridimensional interpolada no globo WGS84
    pub heading_rad: f64, // Rumo interpolado em radianos
    pub quality: PredictionQuality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PredictionQuality {
    Valid,
    Stale,
    ClockSkewed,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkippedTarget {
    pub id: String,
    pub quality: PredictionQuality,
    pub age_seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterpolationBatch {
    pub targets: Vec<InterpolatedTarget>,
    pub skipped: Vec<SkippedTarget>,
}
