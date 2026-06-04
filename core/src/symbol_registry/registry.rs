use crate::sld::StyleRegistry;
use crate::symbol_registry::errors::SymbologyError;
use crate::symbol_registry::primitives::{Color, ResolvedSymbol, SymbolPrimitive};
use crate::symbol_registry::providers::SymbologyProvider;

/// Central registry that delegates symbol resolution to a chain of
/// [`SymbologyProvider`] implementations.
pub struct SymbolRegistry {
    providers: Vec<Box<dyn SymbologyProvider + Send + Sync>>,
}

impl SymbolRegistry {
    /// Creates an empty registry.
    #[inline]
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Registers a new provider.  Providers are queried in registration order.
    #[inline]
    pub fn register_provider(&mut self, provider: Box<dyn SymbologyProvider + Send + Sync>) {
        self.providers.push(provider);
    }

    /// Resolves a symbol code by querying each registered provider in order.
    /// If a provider can resolve the code, any matching SLD style rules are
    /// applied before the symbol is returned.
    #[inline]
    pub fn resolve_symbol(&self, code: &str, style: &StyleRegistry) -> Result<ResolvedSymbol, SymbologyError> {
        for provider in &self.providers {
            if provider.can_resolve(code) {
                let symbol = provider.resolve(code, style)?;
                return Ok(apply_sld_style(symbol, style));
            }
        }
        Err(SymbologyError::ProviderNotFound)
    }
}

impl Default for SymbolRegistry {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a CSS/SVG hex colour string.
///
/// Supported formats:
/// * `#RGB`  (short, opaque)
/// * `#RGBA` (short with alpha)
/// * `#RRGGBB`
/// * `#RRGGBBAA`
fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.trim().strip_prefix('#').unwrap_or(hex);

    let (r, g, b, a) = match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            (r * 17, g * 17, b * 17, 255)
        }
        4 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            let a = u8::from_str_radix(&hex[3..4], 16).ok()?;
            (r * 17, g * 17, b * 17, a * 17)
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            (r, g, b, 255)
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            (r, g, b, a)
        }
        _ => return None,
    };

    Some(Color::rgba(r, g, b, a))
}

/// Apply matching SLD style rules to a resolved symbol.
fn apply_sld_style(mut symbol: ResolvedSymbol, style: &StyleRegistry) -> ResolvedSymbol {
    let rules = style.layers.get(&symbol.symbol_id);
    if let Some(rules) = rules {
        for rule in rules {
            if let Some(ref sld_stroke) = rule.stroke {
                if let Some(sld_color) = parse_hex_color(&sld_stroke.color) {
                    for primitive in &mut symbol.primitives {
                        match primitive {
                            SymbolPrimitive::Path { ref mut stroke, .. } |
                            SymbolPrimitive::Circle { ref mut stroke, .. } => {
                                if let Some(ref mut stroke_val) = stroke {
                                    stroke_val.color = sld_color.clone();
                                    stroke_val.width = sld_stroke.width;
                                    stroke_val.dash_array.clone_from(&sld_stroke.dash_array);
                                }
                            }
                            SymbolPrimitive::Text { .. } => {}
                        }
                    }
                }
            }
            if let Some(ref sld_fill) = rule.fill {
                if let Some(mut sld_color) = parse_hex_color(&sld_fill.color) {
                    sld_color.a = (sld_fill.opacity * 255.0) as u8;
                    for primitive in &mut symbol.primitives {
                        match primitive {
                            SymbolPrimitive::Path { ref mut fill, .. } |
                            SymbolPrimitive::Circle { ref mut fill, .. } => {
                                if let Some(ref mut fill_val) = fill {
                                    *fill_val = sld_color.clone();
                                }
                            }
                            SymbolPrimitive::Text { .. } => {}
                        }
                    }
                }
            }
        }
    }
    symbol
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol_registry::primitives::Color;

    #[test]
    fn test_parse_hex_color_6_digit() {
        let c = parse_hex_color("#FF00AA").unwrap();
        assert_eq!(c, Color::rgb(255, 0, 170));
        assert_eq!(c.a, 255);
    }

    #[test]
    fn test_parse_hex_color_3_digit() {
        let c = parse_hex_color("#F0A").unwrap();
        assert_eq!(c, Color::rgb(255, 0, 170));
    }

    #[test]
    fn test_parse_hex_color_8_digit() {
        let c = parse_hex_color("#FF00AA80").unwrap();
        assert_eq!(c, Color::rgba(255, 0, 170, 128));
    }

    #[test]
    fn test_parse_hex_color_4_digit() {
        let c = parse_hex_color("#F0A8").unwrap();
        assert_eq!(c, Color::rgba(255, 0, 170, 136));
    }

    #[test]
    fn test_parse_hex_color_no_hash_prefix() {
        let c = parse_hex_color("FF00AA").unwrap();
        assert_eq!(c, Color::rgb(255, 0, 170));
    }

    #[test]
    fn test_parse_hex_color_mixed_case() {
        let c = parse_hex_color("#Ff00Aa").unwrap();
        assert_eq!(c, Color::rgb(255, 0, 170));
    }

    #[test]
    fn test_parse_hex_color_invalid() {
        assert!(parse_hex_color("#GGG").is_none());
        assert!(parse_hex_color("#XYZ").is_none());
        assert!(parse_hex_color("").is_none());
        assert!(parse_hex_color("not_a_color").is_none());
        // 5 and 7 chars are not valid hex colour lengths
        assert!(parse_hex_color("#FFFFF").is_none());
        assert!(parse_hex_color("#FFFFFFF").is_none());
        // Too long
        assert!(parse_hex_color("#FFFFFFFFFF").is_none());
    }
}
