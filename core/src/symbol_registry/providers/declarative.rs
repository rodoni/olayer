use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::sld::StyleRegistry;
use crate::symbol_registry::errors::SymbologyError;
use crate::symbol_registry::primitives::{ResolvedSymbol, SymbolPrimitive};
use crate::symbol_registry::providers::SymbologyProvider;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeclarativeLibraryDto {
    library_name: String,
    symbols: HashMap<String, DeclarativeSymbolDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeclarativeSymbolDto {
    bbox: (f64, f64, f64, f64),
    anchor: (f64, f64),
    primitives: Vec<SymbolPrimitive>,
}

/// A provider that loads symbols from a JSON declaration.
pub struct DeclarativeProvider {
    library_name: String,
    symbols: HashMap<String, ResolvedSymbol>,
}

impl DeclarativeProvider {
    /// Parses a JSON library declaration and creates the provider.
    #[inline]
    pub fn from_json(json_content: &str) -> Result<Self, SymbologyError> {
        let lib: DeclarativeLibraryDto = serde_json::from_str(json_content).map_err(|e| {
            SymbologyError::InvalidFormat(format!("Failed to parse JSON library: {}", e))
        })?;

        let mut symbols = HashMap::new();
        for (code, sym_dto) in lib.symbols {
            symbols.insert(
                code.clone(),
                ResolvedSymbol {
                    symbol_id: code,
                    primitives: sym_dto.primitives,
                    bbox: sym_dto.bbox,
                    anchor: sym_dto.anchor,
                },
            );
        }

        Ok(Self {
            library_name: lib.library_name,
            symbols,
        })
    }
}

impl SymbologyProvider for DeclarativeProvider {
    #[inline]
    fn name(&self) -> &str {
        &self.library_name
    }

    #[inline]
    fn can_resolve(&self, code: &str) -> bool {
        self.symbols.contains_key(code)
    }

    #[inline]
    fn resolve(&self, code: &str, _style: &StyleRegistry) -> Result<ResolvedSymbol, SymbologyError> {
        self.symbols
            .get(code)
            .cloned()
            .ok_or_else(|| SymbologyError::SymbolNotFound(code.to_string()))
    }
}
