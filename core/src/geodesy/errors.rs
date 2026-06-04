use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GeodesyError {
    LatitudeOutOfRange(f64),
    LongitudeOutOfRange(f64),
}

impl fmt::Display for GeodesyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LatitudeOutOfRange(val) => write!(
                f,
                "Latitude is out of range [-90, 90] degrees: {val} degrees"
            ),
            Self::LongitudeOutOfRange(val) => write!(
                f,
                "Longitude is out of range [-180, 180] degrees: {val} degrees"
            ),
        }
    }
}

impl std::error::Error for GeodesyError {}
