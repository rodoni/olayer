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
