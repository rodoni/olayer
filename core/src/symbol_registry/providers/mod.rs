use crate::sld::StyleRegistry;
use crate::symbol_registry::errors::SymbologyError;
use crate::symbol_registry::primitives::ResolvedSymbol;

pub trait SymbologyProvider {
    /// Returns the provider's stable human-readable name.
    fn name(&self) -> &str;
    /// Returns whether this provider accepts the supplied code.
    fn can_resolve(&self, code: &str) -> bool;
    /// Resolves a code into a renderable symbol.
    ///
    /// # Errors
    /// Returns a typed [`SymbologyError`] when the code is unsupported or the
    /// provider's symbol data is unavailable.
    fn resolve(&self, code: &str, style: &StyleRegistry) -> Result<ResolvedSymbol, SymbologyError>;
}

pub mod declarative;
pub mod icao;
pub mod nato;

pub use declarative::DeclarativeProvider;
pub use icao::IcaoProvider;
pub use nato::NatoProvider;
