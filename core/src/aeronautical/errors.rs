use crate::error_code::ErrorCode;
use thiserror::Error;

/// Errors that can occur during aeronautical data parsing or processing.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum AeronauticalError {
    /// XML parsing error (e.g. malformed AIXM structure).
    #[error("AIXM XML parsing error: {0}")]
    XmlParseError(String),
    /// JSON parsing error (e.g. invalid GeoJSON).
    #[error("Aeronautical GeoJSON parsing error: {0}")]
    JsonParseError(String),
    /// Missing required attribute or field.
    #[error("Missing required aeronautical field: {0}")]
    MissingRequiredField(String),
    /// Coordinate string could not be parsed into numbers.
    #[error("Invalid coordinate string: {0}")]
    InvalidCoordinateString(String),
    /// Invalid altitude representation.
    #[error("Invalid altitude limit: {0}")]
    InvalidAltitude(String),
    /// Empty or incomplete dataset.
    #[error("Empty aeronautical dataset: {0}")]
    EmptyDataset(String),
    /// General format error.
    #[error("Aeronautical format error: {0}")]
    FormatError(String),
}

impl AeronauticalError {
    /// Returns the stable machine-readable code for this error variant.
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::XmlParseError(_) => ErrorCode::new("OL-CORE-AER-0001"),
            Self::JsonParseError(_) => ErrorCode::new("OL-CORE-AER-0002"),
            Self::MissingRequiredField(_) => ErrorCode::new("OL-CORE-AER-0003"),
            Self::InvalidCoordinateString(_) => ErrorCode::new("OL-CORE-AER-0004"),
            Self::InvalidAltitude(_) => ErrorCode::new("OL-CORE-AER-0005"),
            Self::EmptyDataset(_) => ErrorCode::new("OL-CORE-AER-0006"),
            Self::FormatError(_) => ErrorCode::new("OL-CORE-AER-0007"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AeronauticalError;

    #[test]
    fn codes_are_stable_and_unique_for_every_variant() {
        let errors = [
            AeronauticalError::XmlParseError(String::new()),
            AeronauticalError::JsonParseError(String::new()),
            AeronauticalError::MissingRequiredField(String::new()),
            AeronauticalError::InvalidCoordinateString(String::new()),
            AeronauticalError::InvalidAltitude(String::new()),
            AeronauticalError::EmptyDataset(String::new()),
            AeronauticalError::FormatError(String::new()),
        ];
        let codes = errors.map(|error| error.code().as_str());

        assert_eq!(
            codes,
            [
                "OL-CORE-AER-0001",
                "OL-CORE-AER-0002",
                "OL-CORE-AER-0003",
                "OL-CORE-AER-0004",
                "OL-CORE-AER-0005",
                "OL-CORE-AER-0006",
                "OL-CORE-AER-0007",
            ]
        );
        for (index, code) in codes.iter().enumerate() {
            assert!(!codes[..index].contains(code));
        }
    }
}
