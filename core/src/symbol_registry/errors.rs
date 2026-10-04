use crate::error_code::ErrorCode;
use thiserror::Error;

/// Errors that can occur during symbol resolution.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum SymbologyError {
    /// No registered provider can resolve the requested code.
    #[error("No registered provider can resolve this code")]
    ProviderNotFound,
    /// The symbol was not found in the provider's library.
    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),
    /// The input format (e.g., JSON) is invalid or malformed.
    #[error("Invalid format: {0}")]
    InvalidFormat(String),
}

impl SymbologyError {
    /// Returns the stable machine-readable code for this error.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::ProviderNotFound => ErrorCode::new("OL-CORE-SYM-0001"),
            Self::SymbolNotFound(_) => ErrorCode::new("OL-CORE-SYM-0002"),
            Self::InvalidFormat(_) => ErrorCode::new("OL-CORE-SYM-0003"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SymbologyError;

    #[test]
    fn every_variant_has_a_unique_code() {
        let errors = [
            SymbologyError::ProviderNotFound,
            SymbologyError::SymbolNotFound(String::new()),
            SymbologyError::InvalidFormat(String::new()),
        ];
        let codes = errors.map(|error| error.code());

        assert_eq!(
            codes.map(|code| code.as_str()),
            ["OL-CORE-SYM-0001", "OL-CORE-SYM-0002", "OL-CORE-SYM-0003"]
        );
        for (index, code) in codes.iter().enumerate() {
            assert!(!codes[..index].contains(code));
        }
    }
}
