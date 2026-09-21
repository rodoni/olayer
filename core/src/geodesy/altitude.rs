use crate::geodesy::errors::GeodesyError;
use serde::{Deserialize, Serialize};

/// Vertical reference used by a geodetic height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerticalDatum {
    /// Height above the reference ellipsoid.
    Ellipsoidal,
    /// Height above mean sea level/geoid. A geoid model is required for conversion.
    Orthometric,
}

/// A validated height together with its vertical reference.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Height {
    pub meters: f64,
    pub datum: VerticalDatum,
}

impl Height {
    /// Creates a finite height with the specified vertical datum.
    ///
    /// # Errors
    /// Returns [`GeodesyError::InvalidHeight`] when `meters` is not finite.
    pub fn new(meters: f64, datum: VerticalDatum) -> Result<Self, GeodesyError> {
        if !meters.is_finite() {
            return Err(GeodesyError::InvalidHeight(meters));
        }
        Ok(Self { meters, datum })
    }
}
