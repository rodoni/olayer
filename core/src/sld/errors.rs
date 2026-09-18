use thiserror::Error;

/// Errors that can occur while parsing SLD XML documents.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum SldError {
    /// Low-level XML parsing failure.
    #[error("XML error: {0}")]
    XmlError(String),
    /// A numeric or enum value could not be parsed.
    #[error("Invalid value: {0}")]
    InvalidValue(String),
    #[error("duplicate layer: {0}")]
    DuplicateLayer(String),
}
