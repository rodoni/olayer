use std::fmt;

/// Errors that can occur during aeronautical data parsing or processing.
#[derive(Debug, Clone, PartialEq)]
pub enum AeronauticalError {
    /// XML parsing error (e.g. malformed AIXM structure).
    XmlParseError(String),
    /// JSON parsing error (e.g. invalid GeoJSON).
    JsonParseError(String),
    /// Missing required attribute or field.
    MissingRequiredField(String),
    /// Coordinate string could not be parsed into numbers.
    InvalidCoordinateString(String),
    /// Invalid altitude representation.
    InvalidAltitude(String),
    /// Empty or incomplete dataset.
    EmptyDataset(String),
    /// General format error.
    FormatError(String),
}

impl fmt::Display for AeronauticalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::XmlParseError(msg) => write!(f, "AIXM XML parsing error: {msg}"),
            Self::JsonParseError(msg) => write!(f, "Aeronautical GeoJSON parsing error: {msg}"),
            Self::MissingRequiredField(field) => write!(f, "Missing required aeronautical field: {field}"),
            Self::InvalidCoordinateString(msg) => write!(f, "Invalid coordinate string: {msg}"),
            Self::InvalidAltitude(msg) => write!(f, "Invalid altitude limit: {msg}"),
            Self::EmptyDataset(msg) => write!(f, "Empty aeronautical dataset: {msg}"),
            Self::FormatError(msg) => write!(f, "Aeronautical format error: {msg}"),
        }
    }
}

impl std::error::Error for AeronauticalError {}
