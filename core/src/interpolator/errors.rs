use crate::error_code::ErrorCode;
use crate::geodesy::GeodesyError;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum InterpolatorError {
    InvalidState(String),
    GeodesyFailure(GeodesyError),
}

impl InterpolatorError {
    /// Returns the stable machine-readable code for this error.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::InvalidState(_) => ErrorCode::new("OL-CORE-INT-0001"),
            Self::GeodesyFailure(_) => ErrorCode::new("OL-CORE-INT-0002"),
        }
    }
}

impl fmt::Display for InterpolatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidState(msg) => write!(f, "Invalid target state: {msg}"),
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

#[cfg(test)]
mod tests {
    use super::InterpolatorError;
    use crate::geodesy::GeodesyError;

    #[test]
    fn every_variant_has_a_unique_code() {
        let errors = [
            InterpolatorError::InvalidState(String::new()),
            InterpolatorError::GeodesyFailure(GeodesyError::InvalidEcef),
        ];
        let codes = errors.map(|error| error.code());

        assert_eq!(
            codes.map(|code| code.as_str()),
            ["OL-CORE-INT-0001", "OL-CORE-INT-0002"]
        );
        for (index, code) in codes.iter().enumerate() {
            assert!(!codes[..index].contains(code));
        }
    }
}
