use std::fmt;

/// Errors that can occur during symbol resolution.
#[derive(Debug, Clone, PartialEq)]
pub enum SymbologyError {
    /// No registered provider can resolve the requested code.
    ProviderNotFound,
    /// The symbol was not found in the provider's library.
    SymbolNotFound(String),
    /// The input format (e.g., JSON) is invalid or malformed.
    InvalidFormat(String),
}

impl fmt::Display for SymbologyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SymbologyError::ProviderNotFound => write!(f, "No registered provider can resolve this code"),
            SymbologyError::SymbolNotFound(code) => write!(f, "Symbol not found: {code}"),
            SymbologyError::InvalidFormat(err) => write!(f, "Invalid format: {err}"),
        }
    }
}

impl std::error::Error for SymbologyError {}
