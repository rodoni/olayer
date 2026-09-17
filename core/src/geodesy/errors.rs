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
