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
    pub fn new(meters: f64, datum: VerticalDatum) -> Result<Self, &'static str> {
        if !meters.is_finite() {
            return Err("height must be finite");
        }
        Ok(Self { meters, datum })
    }
}
