use crate::error_code::ErrorCode;
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

impl WeatherError {
    /// Returns the stable machine-readable code for this error variant.
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::InvalidGridDimensions { .. } => ErrorCode::new("OL-CORE-WTH-0001"),
            Self::InvalidIsovalues(_) => ErrorCode::new("OL-CORE-WTH-0002"),
            Self::InvalidWindParameters(_) => ErrorCode::new("OL-CORE-WTH-0003"),
            Self::ParseError(_) => ErrorCode::new("OL-CORE-WTH-0004"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WeatherError;

    #[test]
    fn codes_are_stable_and_unique_for_every_variant() {
        let errors = [
            WeatherError::InvalidGridDimensions {
                width: 1,
                height: 1,
                actual_len: 1,
            },
            WeatherError::InvalidIsovalues(String::new()),
            WeatherError::InvalidWindParameters(String::new()),
            WeatherError::ParseError(String::new()),
        ];
        let codes = errors.map(|error| error.code().as_str());

        assert_eq!(
            codes,
            [
                "OL-CORE-WTH-0001",
                "OL-CORE-WTH-0002",
                "OL-CORE-WTH-0003",
                "OL-CORE-WTH-0004",
            ]
        );
        for (index, code) in codes.iter().enumerate() {
            assert!(!codes[..index].contains(code));
        }
    }
}
