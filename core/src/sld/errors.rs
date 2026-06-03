use std::fmt;

/// Errors that can occur while parsing SLD XML documents.
#[derive(Debug, Clone, PartialEq)]
pub enum SldError {
    /// Low-level XML parsing failure.
    XmlError(String),
    /// A numeric or enum value could not be parsed.
    InvalidValue(String),
}

impl fmt::Display for SldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SldError::XmlError(err) => write!(f, "XML error: {}", err),
            SldError::InvalidValue(detail) => write!(f, "Invalid value: {}", detail),
        }
    }
}

impl std::error::Error for SldError {}
