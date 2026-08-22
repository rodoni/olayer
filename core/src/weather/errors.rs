use std::fmt;

/// Errors that can occur during meteorological computation and overlay processing.
#[derive(Debug, Clone, PartialEq)]
pub enum WeatherError {
    /// Invalid grid dimensions (e.g., width or height is 0 or buffer length mismatch).
    InvalidGridDimensions { width: usize, height: usize, actual_len: usize },
    /// Invalid isovalues for contour generation (e.g., empty array or non-finite values).
    InvalidIsovalues(String),
    /// Invalid wind parameters (e.g., negative wind speed).
    InvalidWindParameters(String),
    /// GeoJSON or format parsing failure for meteorological warnings (SIGMET/AIRMET).
    ParseError(String),
}

impl fmt::Display for WeatherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WeatherError::InvalidGridDimensions { width, height, actual_len } => {
                write!(f, "Invalid grid dimensions: {width}x{height} does not match buffer length {actual_len}")
            }
            WeatherError::InvalidIsovalues(msg) => write!(f, "Invalid isovalues: {msg}"),
            WeatherError::InvalidWindParameters(msg) => write!(f, "Invalid wind parameters: {msg}"),
            WeatherError::ParseError(msg) => write!(f, "Weather data parse error: {msg}"),
        }
    }
}

impl std::error::Error for WeatherError {}
