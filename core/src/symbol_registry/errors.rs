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
