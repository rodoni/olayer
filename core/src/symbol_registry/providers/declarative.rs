use crate::sld::StyleRegistry;
use crate::symbol_registry::errors::SymbologyError;
use crate::symbol_registry::primitives::{ResolvedSymbol, SymbolPrimitive};
use crate::symbol_registry::providers::SymbologyProvider;
use ahash::AHashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
#[derive(Debug, Clone)]
pub struct DeclarativeProvider {
    library_name: String,
    symbols: AHashMap<String, ResolvedSymbol>,
}

impl DeclarativeProvider {
    /// Parses a JSON library declaration and creates the provider.
    ///
    /// # Errors
    /// Returns [`SymbologyError::InvalidFormat`] when the JSON cannot be
    /// deserialized or violates symbol geometry invariants.
    #[inline]
    pub fn from_json(json_content: &str) -> Result<Self, SymbologyError> {
        let lib: DeclarativeLibraryDto = serde_json::from_str(json_content).map_err(|e| {
            SymbologyError::InvalidFormat(format!("Failed to parse JSON library: {e}"))
        })?;

        let mut symbols = AHashMap::with_capacity(lib.symbols.len());
        for (code, sym_dto) in lib.symbols {
            validate_symbol(&code, &sym_dto)?;
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

fn validate_symbol(code: &str, symbol: &DeclarativeSymbolDto) -> Result<(), SymbologyError> {
    if code.trim().is_empty()
        || !symbol.bbox.0.is_finite()
        || !symbol.bbox.1.is_finite()
        || !symbol.bbox.2.is_finite()
        || !symbol.bbox.3.is_finite()
        || symbol.bbox.2 < symbol.bbox.0
        || symbol.bbox.3 < symbol.bbox.1
        || !symbol.anchor.0.is_finite()
        || !symbol.anchor.1.is_finite()
        || symbol.anchor.0 < symbol.bbox.0
        || symbol.anchor.0 > symbol.bbox.2
        || symbol.anchor.1 < symbol.bbox.1
        || symbol.anchor.1 > symbol.bbox.3
    {
        return Err(SymbologyError::InvalidFormat(format!(
            "invalid geometry for symbol '{code}'"
        )));
    }
    for primitive in &symbol.primitives {
        match primitive {
            SymbolPrimitive::Path {
                commands, stroke, ..
            } => {
                if !valid_path(commands) || stroke.as_ref().is_some_and(|s| !valid_stroke(s)) {
                    return Err(SymbologyError::InvalidFormat(format!(
                        "invalid path for symbol '{code}'"
                    )));
                }
            }
            SymbolPrimitive::Circle {
                cx, cy, r, stroke, ..
            } => {
                if !cx.is_finite()
                    || !cy.is_finite()
                    || !r.is_finite()
                    || *r < 0.0
                    || stroke.as_ref().is_some_and(|s| !valid_stroke(s))
                    || *cx - *r < symbol.bbox.0
                    || *cx + *r > symbol.bbox.2
                    || *cy - *r < symbol.bbox.1
                    || *cy + *r > symbol.bbox.3
                {
                    return Err(SymbologyError::InvalidFormat(format!(
                        "invalid circle for symbol '{code}'"
                    )));
                }
            }
            SymbolPrimitive::Text {
                offset_x,
                offset_y,
                font_size,
                content,
                ..
            } => {
                if !offset_x.is_finite()
                    || !offset_y.is_finite()
                    || !font_size.is_finite()
                    || *font_size < 0.0
                    || content.is_empty()
                {
                    return Err(SymbologyError::InvalidFormat(format!(
                        "invalid text for symbol '{code}'"
                    )));
                }
            }
        }
    }
    Ok(())
}

fn valid_path(commands: &str) -> bool {
    let normalized = commands.replace(',', " ");
    let mut tokens = normalized.split_whitespace();
    let mut saw_draw = false;
    while let Some(token) = tokens.next() {
        let bytes = token.as_bytes();
        if bytes.len() != 1 || !bytes[0].is_ascii_alphabetic() {
            return false;
        }
        let command = bytes[0].to_ascii_uppercase();
        let arity = match command {
            b'M' | b'L' | b'T' => 2,
            b'H' | b'V' => 1,
            b'C' => 6,
            b'S' | b'Q' => 4,
            b'A' => 7,
            b'Z' => 0,
            _ => return false,
        };
        saw_draw |= command != b'Z';
        for _ in 0..arity {
            let Some(number) = tokens.next() else {
                return false;
            };
            if number.parse::<f64>().ok().is_none_or(|v| !v.is_finite()) {
                return false;
            }
        }
    }
    saw_draw
}

fn valid_stroke(stroke: &crate::symbol_registry::primitives::Stroke) -> bool {
    stroke.width.is_finite()
        && stroke.width >= 0.0
        && stroke
            .dash_array
            .as_ref()
            .is_none_or(|dashes| dashes.iter().all(|dash| dash.is_finite() && *dash >= 0.0))
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
    fn resolve(
        &self,
        code: &str,
        _style: &StyleRegistry,
    ) -> Result<ResolvedSymbol, SymbologyError> {
        self.symbols
            .get(code)
            .cloned()
            .ok_or_else(|| SymbologyError::SymbolNotFound(code.to_string()))
    }
}
