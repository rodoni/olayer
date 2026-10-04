use crate::error_code::ErrorCode;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum GeodesyError {
    #[error("latitude is not finite")]
    NonFiniteLatitude,
    #[error("longitude is not finite")]
    NonFiniteLongitude,
    #[error("height is not finite")]
    NonFiniteHeight,
    #[error("latitude is out of range [-90, 90] degrees: {0} degrees")]
    LatitudeOutOfRange(f64),
    #[error("longitude is out of range [-180, 180] degrees: {0} degrees")]
    LongitudeOutOfRange(f64),
    #[error("height is not finite: {0}")]
    InvalidHeight(f64),
    #[error("ellipsoid semi-major axis is invalid: {0}")]
    InvalidSemiMajorAxis(f64),
    #[error("ellipsoid flattening is invalid: {0}")]
    InvalidFlattening(f64),
    #[error("ECEF coordinate is not a valid geodetic position")]
    InvalidEcef,
    #[error("bearing must be finite")]
    NonFiniteBearing,
    #[error("distance must be finite")]
    NonFiniteDistance,
    #[error("magnetic model error: {0}")]
    MagneticModelError(String),
}

impl GeodesyError {
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::NonFiniteLatitude => ErrorCode::new("OL-CORE-GEO-0001"),
            Self::NonFiniteLongitude => ErrorCode::new("OL-CORE-GEO-0002"),
            Self::NonFiniteHeight => ErrorCode::new("OL-CORE-GEO-0003"),
            Self::LatitudeOutOfRange(_) => ErrorCode::new("OL-CORE-GEO-0004"),
            Self::LongitudeOutOfRange(_) => ErrorCode::new("OL-CORE-GEO-0005"),
            Self::InvalidHeight(_) => ErrorCode::new("OL-CORE-GEO-0006"),
            Self::InvalidSemiMajorAxis(_) => ErrorCode::new("OL-CORE-GEO-0007"),
            Self::InvalidFlattening(_) => ErrorCode::new("OL-CORE-GEO-0008"),
            Self::InvalidEcef => ErrorCode::new("OL-CORE-GEO-0009"),
            Self::NonFiniteBearing => ErrorCode::new("OL-CORE-GEO-0010"),
            Self::NonFiniteDistance => ErrorCode::new("OL-CORE-GEO-0011"),
            Self::MagneticModelError(_) => ErrorCode::new("OL-CORE-GEO-0012"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GeodesyError;
    use crate::error_code::ErrorCode;
    use std::collections::HashSet;

    #[test]
    fn variants_have_stable_unique_codes() {
        let cases = [
            (GeodesyError::NonFiniteLatitude, "OL-CORE-GEO-0001"),
            (GeodesyError::NonFiniteLongitude, "OL-CORE-GEO-0002"),
            (GeodesyError::NonFiniteHeight, "OL-CORE-GEO-0003"),
            (GeodesyError::LatitudeOutOfRange(0.0), "OL-CORE-GEO-0004"),
            (GeodesyError::LongitudeOutOfRange(0.0), "OL-CORE-GEO-0005"),
            (GeodesyError::InvalidHeight(0.0), "OL-CORE-GEO-0006"),
            (GeodesyError::InvalidSemiMajorAxis(0.0), "OL-CORE-GEO-0007"),
            (GeodesyError::InvalidFlattening(0.0), "OL-CORE-GEO-0008"),
            (GeodesyError::InvalidEcef, "OL-CORE-GEO-0009"),
            (GeodesyError::NonFiniteBearing, "OL-CORE-GEO-0010"),
            (GeodesyError::NonFiniteDistance, "OL-CORE-GEO-0011"),
            (
                GeodesyError::MagneticModelError(String::new()),
                "OL-CORE-GEO-0012",
            ),
        ];
        let codes: Vec<ErrorCode> = cases.iter().map(|(error, _)| error.code()).collect();
        let unique_codes: HashSet<ErrorCode> = codes.iter().copied().collect();

        assert_eq!(codes.len(), cases.len());
        assert_eq!(unique_codes.len(), cases.len());
        for (error, expected) in cases {
            assert_eq!(error.code().as_str(), expected);
        }
    }
}
