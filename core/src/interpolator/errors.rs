use crate::geodesy::GeodesyError;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum InterpolatorError {
    InvalidState(String),
    NegativeTimeDelta(String),
    GeodesyFailure(GeodesyError),
}

impl fmt::Display for InterpolatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidState(msg) => write!(f, "Invalid target state: {msg}"),
            Self::NegativeTimeDelta(msg) => write!(f, "Negative time delta: {msg}"),
            Self::GeodesyFailure(err) => write!(f, "Geodesy calculation failed: {err}"),
        }
    }
}

impl std::error::Error for InterpolatorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::GeodesyFailure(err) => Some(err),
            _ => None,
        }
    }
}

impl From<GeodesyError> for InterpolatorError {
    #[inline]
    fn from(err: GeodesyError) -> Self {
        Self::GeodesyFailure(err)
    }
}
