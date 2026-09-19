use thiserror::Error;

/// Errors that can occur during meteorological computation and overlay processing.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum WeatherError {
    /// Invalid grid dimensions (e.g., width or height is 0 or buffer length mismatch).
    #[error("Invalid grid dimensions: {width}x{height} does not match buffer length {actual_len}")]
    InvalidGridDimensions {
        width: usize,
        height: usize,
        actual_len: usize,
    },
    /// Invalid isovalues for contour generation (e.g., empty array or non-finite values).
    #[error("Invalid isovalues: {0}")]
    InvalidIsovalues(String),
    /// Invalid wind parameters (e.g., negative wind speed).
    #[error("Invalid wind parameters: {0}")]
    InvalidWindParameters(String),
    /// GeoJSON or format parsing failure for meteorological warnings (SIGMET/AIRMET).
    #[error("Weather data parse error: {0}")]
    ParseError(String),
}
