use crate::sld::StyleRegistry;
use crate::symbol_registry::errors::SymbologyError;
use crate::symbol_registry::primitives::ResolvedSymbol;

pub trait SymbologyProvider {
    fn name(&self) -> &str;
    fn can_resolve(&self, code: &str) -> bool;
    fn resolve(&self, code: &str, style: &StyleRegistry) -> Result<ResolvedSymbol, SymbologyError>;
}

pub mod declarative;
pub mod nato;
pub mod icao;

pub use declarative::DeclarativeProvider;
pub use nato::NatoProvider;
pub use icao::IcaoProvider;
