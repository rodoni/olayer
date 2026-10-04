use crate::error_code::ErrorCode;
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

impl SldError {
    /// Returns the stable machine-readable code for this error.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::XmlError(_) => ErrorCode::new("OL-CORE-SLD-0001"),
            Self::InvalidValue(_) => ErrorCode::new("OL-CORE-SLD-0002"),
            Self::DuplicateLayer(_) => ErrorCode::new("OL-CORE-SLD-0003"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SldError;

    #[test]
    fn every_variant_has_a_unique_code() {
        let errors = [
            SldError::XmlError(String::new()),
            SldError::InvalidValue(String::new()),
            SldError::DuplicateLayer(String::new()),
        ];
        let codes = errors.map(|error| error.code());

        assert_eq!(
            codes.map(|code| code.as_str()),
            ["OL-CORE-SLD-0001", "OL-CORE-SLD-0002", "OL-CORE-SLD-0003"]
        );
        for (index, code) in codes.iter().enumerate() {
            assert!(!codes[..index].contains(code));
        }
    }
}
